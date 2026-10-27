//! Streaming parser for the GemStone IV wire format.
//!
//! # What the original does
//!
//! The original `parse_line` (6352 B, `0x101c1a760`) is a left-to-right
//! scanner over one line: text between tags accumulates into one
//! [`Frame::Text`] run per segment, every markup tag flushes the pending text
//! and emits a [`Frame::Structural`], and every other tag is handed to
//! `parser::dispatch::dispatch` (6656 B, `0x101c1d7f8`), which turns it into
//! one of the 59 dedicated `Frame` variants.  A line with no tags at all
//! produces exactly one `Text` frame, even when it is empty.
//!
//! Three families of tag behave differently:
//!
//! * **inline markup** (`parser::markup::is_markup`: `a`, `b`, `d`, `style`,
//!   `preset`, `output`, `pushBold`, `popBold`) — one `Structural` per
//!   occurrence, open and close alike;
//! * **elements** whose body is swallowed by the tag itself
//!   (`parser::is_paired` plus `compDef`) — the whole `<tag …>body</tag>` is
//!   one frame; a missing `</tag>` on the same line is a `MalformedTag`;
//! * **everything else** — a frame on the opening/self-closing tag, a
//!   `Structural` on the closing tag, and the body scanned normally.
//!
//! Bodies of the consumed elements are scanned by `parser::inner`
//! (`other_children`, `body_tags`, `inside_each`), which is *not* the top-level
//! dispatcher: inline markup is dropped, almost every known tag becomes a
//! `Structural`, and only the parent's own children (`crtrStatus` for a
//! component, `objective` for `<objectives>`, `mi` for `<menu>`) get frames.
//!
//! Three pieces of state cross lines: the bare `<settings …>` block (handled a
//! level down, in [`read`]), the stream stack (`pushStream`/`<stream>`/
//! `popStream`/`</stream>`/`prompt`), and the unknown-tag tally the summary
//! footer prints.

pub mod compass;
pub mod inventory;
pub mod markup;
pub mod read;
pub mod wire;

/// `parser::is_paired` lives in [`markup`] in this reconstruction because it
/// shares the tag-name tables with `is_markup`; the original had it directly
/// in `parser`.
pub use markup::is_paired;

use std::collections::BTreeMap;

use crate::frame::payload::Link as PayloadLink;
use crate::frame::Frame;
use crate::runs::{Run, Runs};
use crate::text::{self, entities};

/// Maximum number of bytes buffered for a single line before it is emitted
/// as a [`Frame::Raw`].  The binary used `0x40000`.
pub const MAX_LINE_BYTES: usize = 0x40000;

/// The 15-byte prefix of a Lich client region; `wire::client_region` slices
/// past exactly this many bytes.
const CLIENT_PREFIX: &str = "<!-- CLIENT -->";

/// A streaming parser.
#[derive(Clone, Debug, Default)]
pub struct Parser {
    buffer: Vec<u8>,
    /// Dropping the remainder of an over-long line until the next newline.
    skip_until_newline: bool,
    /// Inside a multi-line `<settings>...</settings>` block.
    ///
    /// While set, [`Parser::push_bytes`] diverts bytes into
    /// [`Parser::settings_window`] instead of the line buffer.
    in_settings: bool,
    /// Rolling window of the last 11 bytes seen inside a settings block, used
    /// to spot `</settings>` (`Parser + 0x60` in the binary).
    settings_window: Vec<u8>,
    /// `</settings>` was seen but the current line has not ended yet
    /// (state `2` at `Parser + 0x169`).
    settings_closed: bool,
    /// Bold state carried across lines.
    bold: bool,
    /// Current `<preset>` style id.
    preset: Option<String>,
    /// Open streams, innermost last: `(id, opened_by_an_element)`.
    ///
    /// `<stream id="x">` pushes `(x, true)` and is closed by `</stream>`;
    /// `<pushStream id="x"/>` pushes `(x, false)` and is closed by
    /// `<popStream/>`.  `<prompt>` empties the stack, emitting one
    /// `StreamPopForced` per entry.
    streams: Vec<(String, bool)>,
    /// Every tag name that was not in `tags::is_known`, with its occurrence
    /// count — the `== unknown tags` footer's data.
    unknown: BTreeMap<String, usize>,
    /// Dialogs that are open across lines (`<openDialog>` / `<dialogData>`).
    ///
    /// While one is open the original drops whitespace-only text outright
    /// (`emit::flush` trims the line and returns when it is blank), which is
    /// what an indented `<openDialog …> </openDialog>` line relies on.
    dialogs: Vec<String>,
}

/// Which child scanner a consumed element's body uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
    /// `parser::inner::other_children`: markup dropped, `crtrStatus` framed,
    /// everything else known → `Structural`.
    Other,
    /// `parser::inner::body_tags`: every known tag → `Structural`.
    Struct,
    /// `<objectives>`: `objective` elements are consumed, the rest as [`Body::Struct`].
    Objective,
    /// `<menu>`: `mi` elements are consumed, the rest as [`Body::Struct`].
    MenuItem,
    /// The whole body belongs to the frame; nothing is scanned.
    Swallowed,
}

impl Parser {
    pub fn new() -> Self {
        Parser::default()
    }

    /// The unknown tags seen so far, sorted by name.
    pub fn unknown_tags(&self) -> &BTreeMap<String, usize> {
        &self.unknown
    }

    /// Parse one complete line (without its trailing newline).
    ///
    /// The returned vector is usually empty or contains a single frame, but a
    /// line may produce many — one per tag plus one per text segment.
    pub fn parse_line(&mut self, line: &str) -> Vec<Frame> {
        let line = line.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start();

        if self.in_settings {
            if trimmed.contains("</settings") {
                self.in_settings = false;
            }
            return Vec::new();
        }
        // Only the exact `<settings ...>` / `<settings>` forms start a
        // settings block; `<settingsInfo>` is a different tag.
        if trimmed.starts_with("<settings ") || trimmed.starts_with("<settings>") {
            let self_closing = trimmed
                .find('>')
                .map(|end| trimmed[..end].trim_end().ends_with('/'))
                .unwrap_or(false);
            if !self_closing && !trimmed.contains("</settings") {
                self.in_settings = true;
            }
            return Vec::new();
        }

        let mut out = Vec::new();
        let mut pending = Runs::new();
        let saw_tag = self.scan(line, &mut pending, &mut out);

        // A line with no tags is always exactly one text frame, even when it
        // is empty; otherwise only non-empty pending text is flushed.  Either
        // way a blank inside an open dialog is dropped.
        if self.blank(pending.plain().as_str()) && !self.dialogs.is_empty() {
            return out;
        }
        if !saw_tag || !pending.is_empty() {
            out.push(Frame::Text(pending));
        }
        out
    }

    /// True when `text` is blank and a dialog is open, i.e. when the original's
    /// `emit::flush` drops it.
    fn blank(&self, text: &str) -> bool {
        text.trim().is_empty()
    }

    /// Scan `line`, appending text to `pending` and frames to `out`.
    ///
    /// Returns `true` when the line contained at least one tag, comment or
    /// client region (i.e. when the "bare text line" rule does not apply).
    fn scan(&mut self, line: &str, pending: &mut Runs, out: &mut Vec<Frame>) -> bool {
        let mut saw_tag = false;
        let mut i = 0usize;
        // The link an open `<a>`/`<d>` element puts on its text.
        let mut link: Option<PayloadLink> = None;

        while i < line.len() {
            let rest = &line[i..];
            let next = match (text::find_tag_start(rest), rest.find("<!--")) {
                (Some(tag), Some(comment)) if comment < tag => Some(Either::Comment(comment)),
                (Some(tag), _) => Some(Either::Tag(tag)),
                (None, Some(comment)) => Some(Either::Comment(comment)),
                (None, None) => None,
            };
            let Some(next) = next else {
                self.push_text(pending, rest, link.as_ref());
                break;
            };

            match next {
                Either::Comment(at) => {
                    if at > 0 {
                        self.push_text(pending, &rest[..at], link.as_ref());
                    }
                    // A comment does not flush the pending text (the original
                    // drops `<!-- … -->` in place), but a client region does.
                    let tail = &rest[at..];
                    saw_tag = true;
                    if tail.starts_with(CLIENT_PREFIX) {
                        let (frame, after) = wire::client_region(tail);
                        if let Frame::ClientText(command) = frame {
                            self.flush(pending, out);
                            out.push(Frame::event_text("ClientCommand", command));
                        }
                        i += at + (tail.len() - after.len());
                    } else {
                        match tail.find("-->") {
                            Some(end) => i += at + end + 3,
                            None => {
                                self.flush(pending, out);
                                out.push(Frame::event("MalformedTag"));
                                return true;
                            }
                        }
                    }
                }
                Either::Tag(at) => {
                    if at > 0 {
                        self.push_text(pending, &rest[..at], link.as_ref());
                    }
                    // Every tag flushes the pending text.
                    self.flush(pending, out);
                    let Some(gt) = rest[at..].find('>') else {
                        // `find_tag_start` guarantees a `>`, but stay total.
                        self.push_text(pending, &rest[at..], None);
                        break;
                    };
                    let tag = &rest[at..=at + gt];
                    i += at + gt + 1;
                    saw_tag = true;
                    match self.dispatch(tag, &line[i..], &mut link, out) {
                        Some(consumed) => i += consumed,
                        None => {
                            // An unterminated element swallows the line: one
                            // MalformedTag replaces everything after the text.
                            out.push(Frame::event("MalformedTag"));
                            return true;
                        }
                    }
                }
            }
        }
        saw_tag
    }

    /// Handle one tag.  `after` is the rest of the line.
    ///
    /// Returns how many bytes of `after` the tag consumed, or `None` when the
    /// tag is malformed and the rest of the line belongs to it.
    fn dispatch(
        &mut self,
        tag: &str,
        after: &str,
        link: &mut Option<PayloadLink>,
        out: &mut Vec<Frame>,
    ) -> Option<usize> {
        let name = text::tag_name(tag);
        let closing = tag.starts_with("</");
        let self_closing = tag.len() >= 2 && tag[..tag.len() - 1].trim_end().ends_with('/');

        if closing {
            if matches!(name, "a" | "d") {
                *link = None;
            }
            if name == "stream" && self.close_element_stream(out) {
                return Some(0);
            }
            if matches!(name, "openDialog" | "opendialog" | "dialogData") {
                self.dialogs.pop();
            }
            self.structural_or_unknown(name, tag, out);
            return Some(0);
        }

        // 1. Inline markup.
        if markup::is_markup(name) {
            if matches!(name, "a" | "d") && !self_closing {
                *link = text::link_from_tag(tag);
            }
            self.markup_state(name, tag);
            out.push(Frame::structural(name, tag));
            return Some(0);
        }

        // 2. Elements whose body the tag owns.
        if let Some((kind, body)) = consumed_element(name) {
            // `<prompt>` ends the current line: every open stream is popped
            // before the prompt itself is framed.
            if kind == "Prompt" {
                self.force_pop_streams(out);
            }
            if self_closing {
                out.push(self.element_frame(kind, ""));
                return Some(0);
            }
            let close = format!("</{name}>");
            let end = after.find(&close)?;
            out.push(self.element_frame(kind, &after[..end]));
            if body != Body::Swallowed {
                self.scan_body(&after[..end], body, out);
            }
            return Some(end + close.len());
        }

        // 3. The stream stack.
        match name {
            "pushStream" => {
                let id = self.attribute_text(tag, "id");
                self.streams.push((id.clone(), false));
                out.push(Frame::event_text("StreamPush", id));
                return Some(0);
            }
            "popStream" => {
                self.streams.pop();
                out.push(Frame::event("StreamPop"));
                self.resume_stream(out);
                return Some(0);
            }
            "stream" | "dynaStream" if !self_closing => {
                let id = self.attribute_text(tag, "id");
                self.streams.push((id.clone(), true));
                out.push(Frame::event_text("StreamPush", id));
                return Some(0);
            }
            _ => {}
        }

        // 4. Everything else: a frame now, a `Structural` at the close.
        let kind = match name {
            "app" => "AppInfo",
            "castTime" => return Some(self.timed(tag, "CastTime", out)),
            "clearContainer" => "ClearContainer",
            "clearDialogData" => "ClearDialogData",
            "clearDynaStream" | "clearStream" => "ClearStream",
            "closeDialog" | "closedialog" => "CloseDialog",
            "cmdtimestamp" => "CmdTimestamp",
            "container" => "Container",
            "crtrStatus" => "CreatureStatus",
            "deleteContainer" => "DeleteContainer",
            "endSetup" => "EndSetup",
            "exposeContainer" | "exposeDialog" | "exposeStream" => "Expose",
            "image" => "InjuryImage",
            "indicator" => "StatusIndicator",
            "label" => "Label",
            "nav" => "RoomId",
            "playerID" => "PlayerId",
            "progressBar" => "ProgressBar",
            "pulse" => "Pulse",
            "roommeta" => "RoomMeta",
            "roundTime" => return Some(self.timed(tag, "RoundTime", out)),
            "settingsInfo" => "SettingsInfo",
            "streamId" | "streamWindow" => "StreamWindow",
            "switchQuickBar" => "QuickbarSwitch",
            "timer" => return Some(self.timed(tag, "AimTime", out)),
            "celebration" => "ActiveEffect",
            "sagaImage" | "sagaimage" => "InlineImage",
            "LaunchURL" | "launchURL" => "LaunchUrl",
            "PantheonStatus" => "PantheonStatus",
            "objective" => "ObjectivesUpdate",
            "mi" => "MenuResponse",
            "openDialog" | "opendialog" => {
                if !self_closing {
                    let id = self.attribute_text(tag, "id");
                    self.dialogs.push(id);
                }
                if text::attribute(tag, "resident").as_deref() == Some("true") {
                    "DialogPanelOpen"
                } else {
                    "DialogOpen"
                }
            }
            "dialogData" => {
                // `<dialogData …/>` clears the panel; a paired one opens it,
                // possibly clearing it first.
                if self_closing {
                    out.push(Frame::event("ClearDialogData"));
                    return Some(0);
                }
                if clears(tag) {
                    out.push(Frame::event("ClearDialogData"));
                }
                let id = self.attribute_text(tag, "id");
                self.dialogs.push(id);
                "DialogOpen"
            }
            "resource" => {
                if text::attribute(tag, "picture").is_some() {
                    "RoomPicture"
                } else {
                    // A `resource` without a `picture` is plain structure.
                    self.structural_or_unknown(name, tag, out);
                    return Some(0);
                }
            }
            "stream" => "Structural",
            "dynaStream" => "Structural",
            // `link`, `sep`, `skin`, `radio`, ... — the dialog widgets.
            _ if is_widget(name) => "DialogWidgets",
            _ => "",
        };
        if !kind.is_empty() {
            out.push(self.element_frame(kind, ""));
        } else if crate::tags::is_known(name) {
            out.push(Frame::event_text("WindowHints", window_hint_id(name, tag)));
        } else {
            self.unknown_tag(name, tag, out);
        }
        Some(0)
    }

    /// `<castTime>` / `<roundTime>` / `<timer>`: a value attribute is
    /// mandatory, and its absence is a `MalformedTag`.
    fn timed(&mut self, tag: &str, kind: &'static str, out: &mut Vec<Frame>) -> usize {
        if text::attribute_u32(tag, "value").is_some() {
            out.push(Frame::event(kind));
        } else {
            out.push(Frame::event("MalformedTag"));
        }
        0
    }

    /// A `Structural` frame for a known tag's occurrence, an unknown-tag frame
    /// (and a footer entry) for anything else.
    fn structural_or_unknown(&mut self, name: &str, tag: &str, out: &mut Vec<Frame>) {
        if crate::tags::is_known(name) {
            out.push(Frame::structural(name, tag));
        } else {
            self.unknown_tag(name, tag, out);
        }
    }

    fn unknown_tag(&mut self, name: &str, tag: &str, out: &mut Vec<Frame>) {
        *self.unknown.entry(name.to_string()).or_insert(0) += 1;
        out.push(Frame::Unknown {
            tag: name.to_string(),
            raw: tag.to_string(),
            text: String::new(),
        });
    }

    /// The frame a consumed element produces.
    fn element_frame(&self, kind: &'static str, body: &str) -> Frame {
        match kind {
            // `<prompt>` keeps its text; every other body stays inside the
            // payload struct the reconstruction does not model.
            "Prompt" => Frame::Prompt(entities::strip_control_chars(&entities::decode_entities(
                body,
            ))),
            _ => Frame::event(kind),
        }
    }

    /// Scan a consumed element's body with the child scanner `body` selects.
    fn scan_body(&mut self, body: &str, mode: Body, out: &mut Vec<Frame>) {
        let mut i = 0usize;
        while i < body.len() {
            let rest = &body[i..];
            let Some(at) = text::find_tag_start(rest) else {
                return;
            };
            let Some(gt) = rest[at..].find('>') else {
                return;
            };
            let tag = &rest[at..=at + gt];
            i += at + gt + 1;
            let name = text::tag_name(tag);
            let closing = tag.starts_with("</");
            let self_closing = tag.len() >= 2 && tag[..tag.len() - 1].trim_end().ends_with('/');

            match mode {
                Body::Swallowed => return,
                Body::Other => {
                    if markup::is_markup(name) {
                        continue;
                    }
                    if name == "crtrStatus" {
                        out.push(Frame::event("CreatureStatus"));
                        continue;
                    }
                    self.structural_or_unknown(name, tag, out);
                }
                Body::Struct => {
                    self.structural_or_unknown(name, tag, out);
                }
                Body::Objective | Body::MenuItem => {
                    let child = if mode == Body::Objective {
                        "objective"
                    } else {
                        "mi"
                    };
                    if name == child && !closing {
                        if self_closing {
                            continue;
                        }
                        let close = format!("</{child}>");
                        match body[i..].find(&close) {
                            Some(end) => i += end + close.len(),
                            None => return,
                        }
                        continue;
                    }
                    self.structural_or_unknown(name, tag, out);
                }
            }
        }
    }

    /// `<prompt>` (or anything else that ends the line) pops every open
    /// stream, innermost first.
    fn force_pop_streams(&mut self, out: &mut Vec<Frame>) {
        while let Some((id, _)) = self.streams.pop() {
            out.push(Frame::event_text("StreamPopForced", id));
        }
    }

    /// `popStream`/`</stream>`: after popping, the stream now on top resumes.
    fn resume_stream(&mut self, out: &mut Vec<Frame>) {
        if let Some((id, _)) = self.streams.last() {
            out.push(Frame::event_text("StreamResume", id.clone()));
        }
    }

    /// `</stream>` closes the innermost *element* stream, wherever it sits in
    /// the stack.  Returns `false` when no `<stream …>` element is open.
    fn close_element_stream(&mut self, out: &mut Vec<Frame>) -> bool {
        let Some(at) = self.streams.iter().rposition(|(_, element)| *element) else {
            return false;
        };
        let (id, _) = self.streams.remove(at);
        out.push(Frame::event_text("StreamPop", id));
        self.resume_stream(out);
        true
    }

    /// Track the style state the markup tags carry across a line.
    fn markup_state(&mut self, name: &str, tag: &str) {
        match name {
            "pushBold" => self.bold = true,
            "popBold" => self.bold = false,
            "preset" => {
                if tag.starts_with("</") {
                    self.preset = None;
                } else if let Some(id) = text::attribute(tag, "id") {
                    self.preset = Some(id);
                }
            }
            _ => {}
        }
    }

    fn attribute_text(&self, tag: &str, name: &str) -> String {
        text::attribute(tag, name)
            .map(|value| entities::decode_attribute_value(&value))
            .unwrap_or_default()
    }

    /// Emit the pending text as one `Text` frame, if there is any.
    ///
    /// Inside an open dialog a blank segment is dropped: `emit::flush`
    /// (`0x101c19dc0`) trims the line first and returns when it is empty.
    fn flush(&self, pending: &mut Runs, out: &mut Vec<Frame>) {
        if pending.is_empty() {
            return;
        }
        if !self.dialogs.is_empty() && self.blank(pending.plain().as_str()) {
            *pending = Runs::new();
            return;
        }
        out.push(Frame::Text(std::mem::take(pending)));
    }

    /// Push a text segment.  Entities are decoded and control characters
    /// stripped; `<…>` spans are **not** removed — what the scanner skipped as
    /// prose stays prose.
    fn push_text(&self, runs: &mut Runs, raw: &str, link: Option<&PayloadLink>) {
        let text = entities::strip_control_chars(&entities::decode_entities(raw));
        self.push_run(runs, text, link);
    }

    fn push_run(&self, runs: &mut Runs, text: String, link: Option<&PayloadLink>) {
        if text.is_empty() {
            return;
        }
        let mut run = Run {
            text,
            preset: self.preset.clone(),
            style: if self.bold { 1 } else { 0 },
            bold: self.bold,
            ..Run::default()
        };
        if let Some(link) = link {
            run.link = Some((*link).clone());
            if link.is_object() {
                run.object = Some((*link).clone());
            }
        }
        runs.push(run);
    }
}



/// Either a protocol tag or a `<!--` comment/client region starts next.
enum Either {
    Tag(usize),
    Comment(usize),
}

/// The consumed elements: `parser::is_paired`'s table plus `compDef`, and the
/// frame/child-scanner pair each one uses.
fn consumed_element(name: &str) -> Option<(&'static str, Body)> {
    Some(match name {
        "component" | "compDef" => ("Component", Body::Other),
        "inv" => ("ContainerItem", Body::Other),
        "egMerchant" => ("EbonGateMerchant", Body::Other),
        "inventoryManager" => ("InventoryManager", Body::Other),
        "cmdlist" => ("CmdListUpdate", Body::Struct),
        "objectives" => ("ObjectivesUpdate", Body::Objective),
        "menu" => ("MenuResponse", Body::MenuItem),
        "compass" => ("Compass", Body::Swallowed),
        "left" => ("LeftHand", Body::Swallowed),
        "right" => ("RightHand", Body::Swallowed),
        "spell" => ("Spell", Body::Swallowed),
        "prompt" => ("Prompt", Body::Swallowed),
        "worldEvent" => ("WorldEvent", Body::Swallowed),
        "inventoryViewItem" => ("InventoryViewItem", Body::Swallowed),
        _ => return None,
    })
}

/// The dialog widget tags, one `DialogWidgets` frame each.
fn is_widget(name: &str) -> bool {
    matches!(
        name,
        "checkBox"
            | "closeButton"
            | "cmdButton"
            | "dropDownBox"
            | "editBox"
            | "hScrollBar"
            | "link"
            | "menuLink"
            | "radio"
            | "sep"
            | "skin"
            | "upDownEditBox"
            | "vScrollBar"
    )
}

/// `clear` is truthy for `"true"` and `"t"` (the two spellings GemStone uses).
fn clears(tag: &str) -> bool {
    matches!(
        text::attribute(tag, "clear").as_deref(),
        Some("true") | Some("t")
    )
}

/// The `id` a `WindowHints` frame carries: the tag's own `id` attribute, with
/// `<mode>` naming the tag instead (the original's `mode` handler).
fn window_hint_id(name: &str, tag: &str) -> String {
    if name == "mode" {
        return name.to_string();
    }
    text::attribute(tag, "id").unwrap_or_else(|| name.to_string())
}

/// Decode a wire line as UTF-8.
///
/// The binary's `decode_wire_line` keeps valid UTF-8 unchanged and maps each
/// invalid byte to the corresponding `U+00xx` character.  That is *not*
/// `from_utf8_lossy` (which inserts `U+FFFD`).
pub(crate) fn decode_wire_line(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::payload::LinkTarget;

    /// The kinds one line produces, in order.  Every expectation below is a
    /// tiny hand-written line; the reference build prints exactly these.
    fn kinds(line: &str) -> Vec<&'static str> {
        Parser::new()
            .parse_line(line)
            .iter()
            .map(Frame::kind)
            .collect()
    }

    fn parsed(line: &str) -> Vec<Frame> {
        Parser::new().parse_line(line)
    }

    /// The text of the `Text` frames only, in order.
    fn plain(frames: &[Frame]) -> Vec<String> {
        frames
            .iter()
            .filter(|frame| matches!(frame, Frame::Text(_)))
            .map(Frame::plain)
            .collect()
    }

    #[test]
    fn a_tagless_line_is_exactly_one_text_frame_even_when_empty() {
        assert_eq!(kinds(""), vec!["Text"]);
        assert_eq!(plain(&parsed("")), vec![String::new()]);
        assert_eq!(plain(&parsed(" ")), vec![" ".to_string()]);
        assert_eq!(plain(&parsed("hello")), vec!["hello".to_string()]);
    }

    #[test]
    fn text_is_not_tag_stripped_and_entities_are_decoded() {
        // `a < b > c` is prose: the angle brackets survive, exactly as the
        // reference build prints them.
        assert_eq!(plain(&parsed("a &lt; b &gt; c")), vec!["a < b > c"]);
        assert_eq!(plain(&parsed("a < b > c")), vec!["a < b > c"]);
    }

    #[test]
    fn inline_markup_is_one_structural_per_occurrence() {
        assert_eq!(kinds("<b>"), vec!["Structural"]);
        assert_eq!(kinds("<b></b>"), vec!["Structural", "Structural"]);
        assert_eq!(
            kinds("AA<b>BB</b>CC"),
            vec!["Text", "Structural", "Text", "Structural", "Text"]
        );
        // `pushBold`/`popBold`/`style`/`preset`/`output`/`a`/`d` are markup too.
        for tag in ["a", "b", "d", "style", "preset", "output", "pushBold", "popBold"] {
            assert_eq!(kinds(&format!("<{tag}/>")), vec!["Structural"], "{tag}");
        }
    }

    #[test]
    fn bold_state_carries_through_the_text_runs() {
        let frames = parsed("<pushBold/>Hello<popBold/> world");
        assert_eq!(
            frames.iter().map(Frame::kind).collect::<Vec<_>>(),
            vec!["Structural", "Text", "Structural", "Text"]
        );
        assert_eq!(plain(&frames), vec!["Hello", " world"]);
        let bold = frames[1].runs().expect("a text frame");
        assert!(bold[0].is_bold(), "the span between the bold tags is bold");
        let plain_run = frames[3].runs().expect("a text frame");
        assert!(!plain_run[0].is_bold());
    }

    #[test]
    fn an_object_link_splits_the_line_into_structural_text_structural() {
        let frames = parsed("<a exist=\"123\" noun=\"gem\">a blue gem</a> glitters");
        assert_eq!(
            frames.iter().map(Frame::kind).collect::<Vec<_>>(),
            vec!["Structural", "Text", "Structural", "Text"]
        );
        let link = frames[1].runs().unwrap()[0].link.clone().expect("a link");
        assert_eq!(
            link.target,
            LinkTarget::Object {
                exist: "123".into(),
                noun: Some("gem".into())
            }
        );
        assert_eq!(frames[3].plain(), " glitters");
    }

    #[test]
    fn dedicated_tags_get_the_original_names() {
        for (line, kind) in [
            ("<label id=\"l\" value=\"v\"/>", "Label"),
            ("<progressBar id=\"p\"/>", "ProgressBar"),
            ("<indicator id=\"i\"/>", "StatusIndicator"),
            ("<link id=\"l\"/>", "DialogWidgets"),
            ("<sep/>", "DialogWidgets"),
            ("<skin id=\"s\"/>", "DialogWidgets"),
            ("<switchQuickBar id=\"s\"/>", "QuickbarSwitch"),
            ("<nav rm=\"1\"/>", "RoomId"),
            ("<streamWindow id=\"s\"/>", "StreamWindow"),
            ("<container id=\"c\"/>", "Container"),
            ("<clearContainer id=\"c\"/>", "ClearContainer"),
            ("<deleteContainer id=\"d\"/>", "DeleteContainer"),
            ("<closeDialog id=\"c\"/>", "CloseDialog"),
            ("<pulse/>", "Pulse"),
            ("<roommeta weather=\"0\"/>", "RoomMeta"),
            ("<resource picture=\"1\"/>", "RoomPicture"),
            ("<playerID id=\"p\"/>", "PlayerId"),
            ("<settingsInfo/>", "SettingsInfo"),
            ("<app id=\"a\"/>", "AppInfo"),
            ("<endSetup/>", "EndSetup"),
            ("<cmdtimestamp/>", "CmdTimestamp"),
            ("<image id=\"i\"/>", "InjuryImage"),
            ("<spell>fire</spell>", "Spell"),
            ("<left id=\"l\">sword</left>", "LeftHand"),
            ("<right id=\"r\">shield</right>", "RightHand"),
        ] {
            assert_eq!(kinds(line), vec![kind], "{line}");
        }
    }

    #[test]
    fn a_known_tag_without_a_handler_is_a_window_hint() {
        assert_eq!(kinds("<dir value=\"e\"/>"), vec!["WindowHints"]);
        assert_eq!(kinds("<task id=\"t\"/>"), vec!["WindowHints"]);
        assert_eq!(kinds("<room name=\"R\"/>"), vec!["WindowHints"]);
        // A `resource` with no `picture` attribute is plain structure.
        assert_eq!(kinds("<resource/>"), vec!["Structural"]);
    }

    #[test]
    fn a_consumed_element_owns_its_body_and_its_close() {
        // The body and the `</compDef>` produce nothing; the body's markup is
        // consumed, its `crtrStatus` is framed.
        assert_eq!(kinds("<compDef id=\"c\">hi <b>x</b></compDef>"), vec!["Component"]);
        assert_eq!(
            kinds("<component id=\"c\"><crtrStatus exist=\"1\"/></component>"),
            vec!["Component", "CreatureStatus"]
        );
        // Nested dedicated tags still frame; nested `component` does not.
        assert_eq!(
            kinds("<component id=\"c\"><foo/></component>"),
            vec!["Component", "UnknownTag"]
        );
        assert_eq!(
            kinds("<component id=\"a\"><component id=\"b\"/></component>"),
            vec!["Component", "Structural"]
        );
        // A body tag known to the vocabulary is structural inside a body.
        assert_eq!(
            kinds("<inv id=\"i\"><label id=\"l\"/></inv>"),
            vec!["ContainerItem", "Structural"]
        );
        assert_eq!(kinds("<inv id=\"i\">text</inv>"), vec!["ContainerItem"]);
    }

    #[test]
    fn a_consumed_element_without_its_close_is_a_malformed_tag() {
        assert_eq!(kinds("<component id=\"c\">hello"), vec!["MalformedTag"]);
        assert_eq!(kinds("<inv>text"), vec!["MalformedTag"]);
        assert_eq!(kinds("<prompt>text"), vec!["MalformedTag"]);
        // Text before it is flushed, then the malformed frame.
        assert_eq!(kinds("x<component id=\"c\">y"), vec!["Text", "MalformedTag"]);
    }

    #[test]
    fn a_missing_timers_value_is_a_malformed_tag() {
        assert_eq!(kinds("<castTime value=\"3\"/>"), vec!["CastTime"]);
        assert_eq!(kinds("<castTime text=\"x\"/>"), vec!["MalformedTag"]);
        assert_eq!(kinds("<castTime value=\"abc\"/>"), vec!["MalformedTag"]);
        assert_eq!(kinds("<roundTime value=\"5\"/>"), vec!["RoundTime"]);
        assert_eq!(kinds("<roundTime/>"), vec!["MalformedTag"]);
        assert_eq!(kinds("<timer value=\"5\"/>"), vec!["AimTime"]);
        assert_eq!(kinds("<timer id=\"t\"/>"), vec!["MalformedTag"]);
    }

    #[test]
    fn an_unknown_tag_is_counted_once_per_occurrence_and_tallied() {
        let mut parser = Parser::new();
        assert_eq!(
            parser
                .parse_line("<foo>hi</foo>")
                .iter()
                .map(Frame::kind)
                .collect::<Vec<_>>(),
            vec!["UnknownTag", "Text", "UnknownTag"]
        );
        parser.parse_line("<foo/>");
        assert_eq!(parser.unknown_tags().get("foo"), Some(&3));
        assert!(Parser::new().unknown_tags().is_empty());
    }

    #[test]
    fn a_comment_is_dropped_in_place_without_splitting_the_text() {
        assert_eq!(plain(&parsed("a <!-- b --> c")), vec!["a  c"]);
        assert_eq!(kinds("<!-- x -->"), Vec::<&str>::new());
        assert_eq!(kinds("<!-- unterminated"), vec!["MalformedTag"]);
        assert_eq!(kinds("< !y >").len(), 1, "not a tag: prose");
    }

    #[test]
    fn a_client_region_becomes_one_client_command() {
        let frames = parsed("<!-- CLIENT --><c>a &amp; b<!-- ENDCLIENT -->");
        assert_eq!(frames.iter().map(Frame::kind).collect::<Vec<_>>(), vec!["ClientCommand"]);
        assert_eq!(frames[0].plain(), "a & b");
        // A head without `<c>` is dropped and produces no frame at all.
        assert!(parsed("<!-- CLIENT -->hello<!-- ENDCLIENT -->").is_empty());
    }

    #[test]
    fn the_stream_stack_pops_in_order_and_resumes_the_new_top() {
        let mut parser = Parser::new();
        let mut all = Vec::new();
        for line in [
            "<pushStream id=\"a\"/>",
            "<pushStream id=\"b\"/>",
            "<popStream/>",
            "text",
        ] {
            all.extend(parser.parse_line(line));
        }
        let kinds: Vec<&str> = all.iter().map(Frame::kind).collect();
        assert_eq!(
            kinds,
            vec!["StreamPush", "StreamPush", "StreamPop", "StreamResume", "Text"]
        );
        assert_eq!(all[3].plain(), "a", "the stream now on top resumes");
    }

    #[test]
    fn a_prompt_forces_every_open_stream_shut() {
        let mut parser = Parser::new();
        parser.parse_line("<pushStream id=\"a\"/>");
        parser.parse_line("<stream id=\"b\">");
        let frames = parser.parse_line("<prompt>&gt;</prompt>");
        let kinds: Vec<&str> = frames.iter().map(Frame::kind).collect();
        assert_eq!(
            kinds,
            vec!["StreamPopForced", "StreamPopForced", "Prompt"]
        );
        assert_eq!(frames[0].plain(), "b", "innermost first");
        assert_eq!(frames[2].plain(), ">");
        // The stack is empty again: a second prompt forces nothing.
        assert_eq!(kinds_of(&mut parser, "<prompt>x</prompt>"), vec!["Prompt"]);
    }

    fn kinds_of(parser: &mut Parser, line: &str) -> Vec<&'static str> {
        parser.parse_line(line).iter().map(Frame::kind).collect()
    }

    #[test]
    fn closing_a_stream_element_pops_that_element_even_from_the_middle() {
        let mut parser = Parser::new();
        let mut all = Vec::new();
        all.extend(parser.parse_line("<stream id=\"a\">"));
        all.extend(parser.parse_line("<pushStream id=\"b\"/>"));
        all.extend(parser.parse_line("</stream>"));
        let kinds: Vec<&str> = all.iter().map(Frame::kind).collect();
        assert_eq!(
            kinds,
            vec!["StreamPush", "StreamPush", "StreamPop", "StreamResume"]
        );
        assert_eq!(all[2].plain(), "a");
        assert_eq!(all[3].plain(), "b");
        // With no element open, `</stream>` is just structure.
        assert_eq!(kinds_of(&mut parser, "</stream>"), vec!["Structural"]);
    }

    #[test]
    fn blank_text_is_dropped_while_a_dialog_is_open() {
        let mut parser = Parser::new();
        // A dialog opened on an earlier line suppresses whitespace-only text,
        // which is what an indented `<openDialog …> </openDialog>` relies on.
        assert_eq!(kinds_of(&mut parser, "<openDialog id=\"q\">"), vec!["DialogOpen"]);
        assert!(kinds_of(&mut parser, " ").is_empty());
        assert_eq!(kinds_of(&mut parser, "hi"), vec!["Text"]);
        assert_eq!(kinds_of(&mut parser, "</openDialog>"), vec!["Structural"]);
        assert_eq!(kinds_of(&mut parser, " "), vec!["Text"]);
        // On one line: the whitespace between the tags disappears.
        assert_eq!(
            kinds("<openDialog id=\"q\"> </openDialog>"),
            vec!["DialogOpen", "Structural"]
        );
        // …but a tag that is not a dialog keeps its whitespace text.
        assert_eq!(
            kinds("<label id=\"l\"> </label>"),
            vec!["Label", "Text", "Structural"]
        );
    }

    #[test]
    fn a_resident_open_dialog_is_a_panel() {
        assert_eq!(kinds("<openDialog id=\"q\"/>"), vec!["DialogOpen"]);
        assert_eq!(
            kinds("<openDialog id=\"q\" resident=\"true\"/>"),
            vec!["DialogPanelOpen"]
        );
        assert_eq!(
            kinds("<openDialog id=\"q\" resident=\"false\"/>"),
            vec!["DialogOpen"]
        );
    }

    #[test]
    fn dialog_data_clears_then_opens() {
        assert_eq!(kinds("<dialogData id=\"d\"/>"), vec!["ClearDialogData"]);
        assert_eq!(
            kinds("<dialogData id=\"d\"></dialogData>"),
            vec!["DialogOpen", "Structural"]
        );
        for truthy in ["true", "t"] {
            assert_eq!(
                kinds(&format!("<dialogData id=\"d\" clear=\"{truthy}\"></dialogData>")),
                vec!["ClearDialogData", "DialogOpen", "Structural"]
            );
        }
        for falsy in ["1", "false", "yes"] {
            assert_eq!(
                kinds(&format!("<dialogData id=\"d\" clear=\"{falsy}\"></dialogData>")),
                vec!["DialogOpen", "Structural"]
            );
        }
    }

    #[test]
    fn objective_and_menu_children_are_consumed() {
        assert_eq!(
            kinds("<objectives><objective id=\"1\"/><objective id=\"2\"/></objectives>"),
            vec!["ObjectivesUpdate"]
        );
        assert_eq!(kinds("<menu><mi id=\"1\"/></menu>"), vec!["MenuResponse"]);
        // A stray one is an update of its own.
        assert_eq!(kinds("<objective id=\"1\"/>"), vec!["ObjectivesUpdate"]);
        assert_eq!(kinds("<mi id=\"1\"/>"), vec!["MenuResponse"]);
        // Other known children of those two are structure.
        assert_eq!(
            kinds("<objectives><task id=\"t\"/></objectives>"),
            vec!["ObjectivesUpdate", "Structural"]
        );
    }

    #[test]
    fn a_compass_swallows_its_directions() {
        assert_eq!(
            kinds("<compass><dir value=\"e\"/><dir value=\"s\"/></compass>"),
            vec!["Compass"]
        );
        assert_eq!(kinds("<dir value=\"e\"/>"), vec!["WindowHints"]);
    }

    #[test]
    fn settings_blocks_are_suppressed_until_they_close() {
        let mut parser = Parser::new();
        assert!(parser.parse_line("<settings>").is_empty());
        assert!(parser.parse_line("{\"still\":\"settings\"}").is_empty());
        assert!(parser.parse_line("</settings>").is_empty());
        assert_eq!(
            parser
                .parse_line("visible")
                .iter()
                .map(Frame::kind)
                .collect::<Vec<_>>(),
            vec!["Text"]
        );
    }

    #[test]
    fn push_bytes_feeds_the_same_scanner_across_chunks() {
        let mut parser = Parser::new();
        assert!(parser.push_bytes(b"<pushBold/>Hel").is_empty());
        let frames = parser.push_bytes(b"lo<popBold/>\r\nsecond");
        assert_eq!(
            frames.iter().map(Frame::kind).collect::<Vec<_>>(),
            vec!["Structural", "Text", "Structural"]
        );
        assert_eq!(frames[1].plain(), "Hello");
        let frames = parser.push_bytes(b" line\r\n");
        assert_eq!(plain(&frames), vec!["second line"]);
        assert!(parser.finish().is_empty());
    }

    #[test]
    fn push_bytes_lines_groups_frames_by_input_line() {
        let mut parser = Parser::new();
        // Two input lines in one chunk, the second one still incomplete.
        let lines = parser.push_bytes_lines(b"one\r\n<b>two</b>\r\nthree");
        assert_eq!(lines.len(), 2, "only the completed lines are returned");
        assert_eq!(plain(&lines[0]), vec!["one"]);
        assert_eq!(
            lines[1].iter().map(Frame::kind).collect::<Vec<_>>(),
            vec!["Structural", "Text", "Structural"]
        );
        // The incomplete line is still buffered and comes out of the next call.
        let lines = parser.push_bytes_lines(b" and more\r\n");
        assert_eq!(plain(&lines[0]), vec!["three and more"]);
        // A blank line is still a line: it produces the one empty `Text` frame
        // the reference build produces for `"\n"`, so it gets its own group.
        let lines = parser.push_bytes_lines(b"\r\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(plain(&lines[0]), vec![""]);
        // A `<settings …>` line produces no frame, so it has no group at all.
        assert!(parser.push_bytes_lines(b"<settings></settings>\r\n").is_empty());
        assert!(parser.finish().is_empty());
    }

    #[test]
    fn the_grouped_and_flat_feeds_agree() {
        let wire = b"one\r\n<b>two</b>\r\n<prompt>&gt;</prompt>\r\nthree";
        let mut flat = Parser::new();
        let mut grouped = Parser::new();
        let mut a = flat.push_bytes(&wire[..7]);
        a.extend(flat.push_bytes(&wire[7..]));
        a.extend(flat.finish());
        let mut b: Vec<Frame> = grouped
            .push_bytes_lines(&wire[..7])
            .into_iter()
            .flatten()
            .collect();
        b.extend(grouped.push_bytes_lines(&wire[7..]).into_iter().flatten());
        b.extend(grouped.finish());
        assert_eq!(a, b);
    }
}
