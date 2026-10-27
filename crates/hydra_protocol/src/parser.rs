//! Streaming parser for the GemStone IV wire format.
//!
//! The original `parse_line` is a 45 KB inlined tag-dispatch function.  This
//! implementation recovers the architecture and the common tags:
//!
//!   * `<pushBold/>` / `<popBold/>`
//!   * `<a exist=".." noun="..">text</a>` (object links)
//!   * `<a href="..">text</a>` (hyperlinks)
//!   * `<d cmd="..">text</d>` (command links)
//!   * `<prompt>..</prompt>`
//!   * `<merchantStatus>..</merchantStatus>`
//!   * `<preset id="..">..</preset>` (and `</preset>`)
//!   * entity decoding and control-character stripping
//!
//! Unknown tags are preserved as raw text rather than dropped.  Rare
//! structural tags (dialogs, containers, menus, streams) are ignored for
//! now; see the `UNKNOWN` notes on [`Parser::parse_line`].

pub mod read;

use crate::frame::payload::{Link, MerchantStatus};
use crate::frame::Frame;
use crate::runs::{Run, Runs};
use crate::text::{self, entities};

/// Maximum number of bytes buffered for a single line before it is emitted
/// as a [`Frame::Raw`].  The binary used `0x40000`.
pub const MAX_LINE_BYTES: usize = 0x40000;

/// A streaming parser.
#[derive(Clone, Debug, Default)]
pub struct Parser {
    buffer: Vec<u8>,
    /// Dropping the remainder of an over-long line until the next newline.
    skip_until_newline: bool,
    /// Inside a multi-line `<settings>...</settings>` block.
    in_settings: bool,
    /// Bold state carried across lines.
    bold: bool,
    /// Current `<preset>` style id.
    preset: Option<String>,
}

impl Parser {
    pub fn new() -> Self {
        Parser::default()
    }

    /// Parse one complete line (without its trailing newline).
    ///
    /// The returned vector is usually empty or contains a single frame, but
    /// the original could emit several frames for one line; callers should
    /// always iterate it.
    ///
    /// `// UNKNOWN:` the original dispatches many more structural tags
    /// (`<dialogData>`, `<inv>`, `<streamWindow>`, `<compass>`, menus, ...).
    /// Those are ignored here; text-bearing known tags are rendered as text
    /// and unknown tags are preserved verbatim.
    pub fn parse_line(&mut self, line: &str) -> Vec<Frame> {
        let line = line.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start();

        if self.in_settings {
            if trimmed.contains("</settings") {
                self.in_settings = false;
            }
            return Vec::new();
        }
        if trimmed.starts_with("<settings") {
            if !trimmed.contains("</settings") {
                self.in_settings = true;
            }
            return Vec::new();
        }

        let mut runs = Runs::new();
        let mut prompt: Option<String> = None;
        let mut status: Option<MerchantStatus> = None;
        let mut i = 0usize;

        while i < line.len() {
            let Some(rel) = text::find_tag_start(&line[i..]) else {
                self.push_text(&mut runs, &line[i..], None);
                break;
            };
            let start = i + rel;
            if start > i {
                self.push_text(&mut runs, &line[i..start], None);
            }
            let Some(gt_rel) = line[start..].find('>') else {
                // Unterminated tag: keep the remainder as text.
                self.push_text(&mut runs, &line[start..], None);
                break;
            };
            let end = start + gt_rel; // index of '>'
            let tag = &line[start..=end];
            let name = text::tag_name(tag);
            let closing = tag.starts_with("</");
            let self_closing = tag[..tag.len() - 1].trim_end().ends_with('/');
            i = end + 1;

            match name {
                "pushBold" => {
                    self.bold = true;
                }
                "popBold" => {
                    self.bold = false;
                }
                "preset" => {
                    if closing {
                        self.preset = None;
                    } else if let Some(id) = text::attribute(tag, "id") {
                        self.preset = Some(id);
                    }
                }
                "a" | "d" if !closing => {
                    let link = text::link_from_tag(tag);
                    if let Some((inner, next)) = inner_until(line, i, name) {
                        self.push_text(&mut runs, inner, link.as_ref());
                        i = next;
                    } else if let Some(link) = link {
                        // Self-closing / unterminated link: keep the tag text
                        // so information is not lost.
                        self.push_text(&mut runs, tag, Some(&link));
                    }
                }
                "prompt" if !closing => {
                    if let Some((inner, next)) = inner_until(line, i, name) {
                        prompt = Some(self.decoded(inner));
                        i = next;
                    }
                }
                "merchantStatus" if !closing => {
                    if let Some((inner, next)) = inner_until(line, i, name) {
                        status = Some(MerchantStatus::parse(inner));
                        i = next;
                    }
                }
                _ if closing => {
                    // A stray known closing tag is ignored; an unknown one is
                    // preserved verbatim.
                    if !crate::tags::is_known(name) {
                        self.push_literal(&mut runs, tag, None);
                    }
                }
                _ if self_closing => {
                    // Formatting/stream tags with no body.  Known tags are
                    // structural and are dropped; unknown ones are preserved
                    // as raw text so nothing disappears silently.
                    if !crate::tags::is_known(name) {
                        self.push_literal(&mut runs, tag, None);
                    }
                }
                _ => {
                    // A body-bearing tag.  Known structural tags (roomDesc,
                    // streamWindow, b, i, ...) contribute no text of their
                    // own: ignoring them lets the scanner descend into any
                    // nested links/text instead of flattening them.
                    // Unknown markup is preserved verbatim.
                    if !crate::tags::is_known(name) {
                        self.push_literal(&mut runs, tag, None);
                    }
                }
            }
        }

        let mut frames = Vec::new();
        if let Some(status) = status {
            frames.push(Frame::MerchantStatus(status));
        }
        if let Some(prompt) = prompt {
            frames.push(Frame::Prompt(prompt));
        }
        if !runs.is_empty() {
            frames.push(Frame::Text(runs));
        }
        frames
    }

    fn decoded(&self, raw: &str) -> String {
        let stripped = strip_tags(raw);
        entities::strip_control_chars(&entities::decode_entities(&stripped))
    }

    /// Push text, stripping any nested markup first (normal text content).
    fn push_text(&self, runs: &mut Runs, raw: &str, link: Option<&Link>) {
        let text = self.decoded(raw);
        self.push_run(runs, text, link);
    }

    /// Push a literal fragment such as a preserved unknown tag.  Entities are
    /// still decoded, but `<...>` spans are *not* stripped.
    fn push_literal(&self, runs: &mut Runs, raw: &str, link: Option<&Link>) {
        let text = entities::strip_control_chars(&entities::decode_entities(raw));
        self.push_run(runs, text, link);
    }

    fn push_run(&self, runs: &mut Runs, text: String, link: Option<&Link>) {
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
            run.link = Some(link.clone());
            if link.is_object() {
                run.object = Some(link.clone());
            }
        }
        runs.push(run);
    }
}

/// Return `(inner, index_after_closing_tag)` for the next `</name>`.
fn inner_until<'a>(line: &'a str, from: usize, name: &str) -> Option<(&'a str, usize)> {
    let close = format!("</{}>", name);
    let rel = line[from..].find(&close)?;
    let inner_end = from + rel;
    let next = inner_end + close.len();
    Some((&line[from..inner_end], next))
}

/// Remove `<...>` spans, keeping their text content.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find('>') {
            Some(end) => rest = &after[end + 1..],
            None => {
                out.push_str(after);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
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

    fn first_text(frames: &[Frame]) -> &Runs {
        frames
            .iter()
            .find_map(Frame::runs)
            .expect("a text frame should be produced")
    }

    #[test]
    fn bold_tags_toggle_run_style() {
        let mut parser = Parser::new();
        let frames = parser.parse_line("<pushBold/>Hello<popBold/> world");
        assert_eq!(frames.len(), 1);
        let runs = first_text(&frames);
        assert_eq!(runs.plain(), "Hello world");
        assert_eq!(runs.len(), 2);
        assert!(runs[0].is_bold());
        assert!(!runs[1].is_bold());
    }

    #[test]
    fn object_link_is_extracted() {
        let mut parser = Parser::new();
        let frames =
            parser.parse_line("<a exist=\"123\" noun=\"gem\">a blue gem</a>");
        let runs = first_text(&frames);
        assert_eq!(runs.plain(), "a blue gem");
        let link = runs[0].link.as_ref().expect("link");
        assert_eq!(
            link.target,
            LinkTarget::Object {
                exist: "123".into(),
                noun: Some("gem".into())
            }
        );
        assert_eq!(runs[0].object_name(), Some("123"));
    }

    #[test]
    fn command_link_is_extracted() {
        let mut parser = Parser::new();
        let frames = parser.parse_line("<d cmd=\"look at gem\">look</d>");
        let runs = first_text(&frames);
        assert_eq!(runs.plain(), "look");
        assert_eq!(
            runs[0].link.as_ref().unwrap().target,
            LinkTarget::Command {
                cmd: "look at gem".into()
            }
        );
    }

    #[test]
    fn links_nested_inside_roomdesc_are_found() {
        let mut parser = Parser::new();
        let frames = parser.parse_line(
            "<roomDesc>A blue gem lies here.</roomDesc>",
        );
        assert_eq!(
            first_text(&frames).plain(),
            "A blue gem lies here."
        );

        let frames = parser.parse_line(
            "<roomDesc>You see <a exist=\"7\" noun=\"gem\">a blue gem</a> here.</roomDesc>",
        );
        let runs = first_text(&frames);
        assert_eq!(runs.plain(), "You see a blue gem here.");
        assert_eq!(runs[1].object_name(), Some("7"));
    }

    #[test]
    fn entities_are_decoded_in_text() {
        let mut parser = Parser::new();
        let frames = parser.parse_line("a &lt; b &amp; c");
        assert_eq!(first_text(&frames).plain(), "a < b & c");
    }

    #[test]
    fn prompt_and_status_frames() {
        let mut parser = Parser::new();
        let frames = parser.parse_line("<prompt>&gt;</prompt>");
        assert_eq!(frames, vec![Frame::Prompt(">".into())]);

        let frames = parser.parse_line("<merchantStatus> STOP </merchantStatus>");
        assert_eq!(
            frames,
            vec![Frame::MerchantStatus(MerchantStatus::Stop)]
        );
    }

    #[test]
    fn unknown_tags_are_preserved() {
        let mut parser = Parser::new();
        let frames = parser.parse_line("<notATag>hi</notATag>");
        assert_eq!(first_text(&frames).plain(), "<notATag>hi</notATag>");
    }

    #[test]
    fn settings_lines_are_suppressed() {
        let mut parser = Parser::new();
        assert!(parser.parse_line("<settingsInfo/>").is_empty());
        assert!(parser.parse_line("<settings>").is_empty());
        assert!(parser.parse_line("{\"still\":\"settings\"}").is_empty());
        assert!(parser.parse_line("</settings>").is_empty());
        assert!(parser
            .parse_line("<settings>{\"a\":1}</settings>")
            .is_empty());
        assert_eq!(
            first_text(&parser.parse_line("visible")).plain(),
            "visible"
        );
    }

    #[test]
    fn push_bytes_handles_chunk_boundaries() {
        let mut parser = Parser::new();
        assert!(parser.push_bytes(b"<pushBold/>Hel").is_empty());
        let frames = parser.push_bytes(b"lo<popBold/>\nsecond");
        assert_eq!(frames.len(), 1);
        assert_eq!(first_text(&frames).plain(), "Hello");

        let frames = parser.push_bytes(b" line\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(first_text(&frames).plain(), "second line");

        assert!(parser.finish().is_empty());
    }

    #[test]
    fn finish_flushes_partial_line_and_crlf_is_stripped() {
        let mut parser = Parser::new();
        let frames = parser.push_bytes(b"a line\r\n");
        assert_eq!(first_text(&frames).plain(), "a line");

        assert!(parser.push_bytes(b"tail").is_empty());
        let frames = parser.finish();
        assert_eq!(first_text(&frames).plain(), "tail");
        assert!(parser.finish().is_empty());
    }
}
