//! A live session: the game's XML stream folded into what the window draws.
//!
//! Phase 9 wires `hydra gui <character>` as *login → game socket → parser → this
//! model → `hydra_ui`/`hydra_gui`*.  The model is deliberately socket-free: it
//! is fed one line at a time ([`LiveSession::feed_line`]) or one batch of parsed
//! frames ([`LiveSession::feed_frames`]), so every rule below is testable
//! without a game, a login or a window.
//!
//! # What it is built from
//!
//! Nothing here re-implements the UI's model, it only folds protocol frames into
//! the real ones:
//!
//! | part of the window | the real model |
//! | --- | --- |
//! | story/scrollback and the prompt line | [`hydra_ui::story::Story`], via [`Story::hear`] and the theme's palette ([`hydra_ui::theme::file::Theme::default`]) |
//! | room title/description/exits | [`hydra_ui::view::PlayView`] (`exits_in` parses the exit caption out of the description) |
//! | vitals | [`crate::bar::stack::Stack`] (`Sp`/`HP`/`MP`/`SP`, `HP 80%`) |
//! | hands | [`hydra_model::state::hands::Hand`] (`<left>`/`<right>`/`<held>`) |
//! | compass | the ten direction words, in the order the game sent them |
//!
//! # Why the raw line is inspected as well as the frames
//!
//! The recovered parser turns the game's structured elements into dedicated
//! `Frame::Event`s (`Component`, `RoomId`, `ProgressBar`, `Compass`, `LeftHand`,
//! `RightHand`, `Prompt`) that **carry no attributes**: `Parser::element_frame`
//! builds them with an empty body (`crates/hydra_protocol/src/parser.rs`).  So
//! the room name, the progress percentages, the compass directions and the hand
//! items are unreachable from the frame stream alone.  `feed_line` therefore
//! scans the raw line for the tags the game actually sends, *in addition* to
//! handing the line to the parser:
//!
//! | tag | effect |
//! | --- | --- |
//! | `<component id="roomName" value="…"/>` | the room title (starts a new room description) |
//! | `<component id="roomDesc" value="…"/>` | the room description, verbatim |
//! | `<nav rm="…"/>` | the room number |
//! | `<progressBar id="health\|mana\|spirit\|stamina" value="…"/>` | that vital's percentage |
//! | `<left>…</left>`, `<right>…</right>`, `<held>…</held>` | the hands |
//! | `<compass><dir value="n"/>…</compass>` | the compass words |
//! | `<prompt>…</prompt>` | the prompt |
//!
//! The **text/preset forms** are read by the same pass, not from the parser's
//! runs: `<preset id="roomName">…</preset>`, `<preset id="roomDesc">…</preset>`,
//! `<roomName>…</roomName>` and `<roomDesc>…</roomDesc>` all reach
//! [`LiveSession::read_tags`].  The story is fed from the frames
//! ([`Story::hear`]), so the same text is *also* what the scrollback shows —
//! which is what makes the raw/preset room text visible in the story as well as
//! in the room pane.  A `<component id="roomDesc" value="…"/>` description is an
//! attribute and has no text of its own, so it reaches the room pane only.
//!
//! # A parser state note
//!
//! `Parser::dispatch` records `<preset>`/bold markup state for **opening** tags
//! only: its closing branch returns through `structural_or_unknown` before
//! `markup_state` runs.  A `<preset id="roomDesc">…</preset>` line's style would
//! therefore leak into every later line's runs.
//! [`LiveSession::feed_line`] closes the span itself (a neutral
//! `<preset id=""/>`), and the room pane never reads a run's preset, so a stale
//! style can neither colour the story wrongly nor move the room.  The parser
//! itself is left alone: changing its frame stream would change what
//! `hydra replay` prints, which is a separate decision.
//!
//! # Inferred behaviour (marked at the point of use)
//!
//! * which tag spellings are accepted — the parser's own tag table plus the
//!   attribute names the game is known to send; anything else is ignored rather
//!   than guessed at;
//! * a room name arriving **starts** a new room description (so a second `look`
//!   replaces the first description instead of appending to it);
//! * the `> ` prefix on an echoed command, and the `[hydra] …` shape of the
//!   local notices;
//! * blank protocol lines are dropped instead of taking a row in the scrollback.

use hydra_model::state::hands::Hand;
use hydra_protocol::frame::Frame;
use hydra_protocol::parser::Parser;
use hydra_protocol::runs::Runs;
use hydra_protocol::text::{self, entities};
use hydra_ui::input::{self, InputError};
use hydra_ui::story::Story;
use hydra_ui::theme::file::Theme;
use hydra_ui::view::{self, PlayView};

use crate::bar::stack::{Stack, Vital};

/// The status word while the game socket is open.
pub const CONNECTED: &str = "connected";

/// The status word once the game socket has ended.
pub const DISCONNECTED: &str = "disconnected";

/// The story line added when the game closes the connection.
///
/// **RECOVERED** wording, from `crates/hydra/src/typing.rs`'s `GAME_CLOSED`
/// literal (`\n[hydra] the game closed the connection\n`), without the leading
/// and trailing newlines that literal carries for `stdout`.  `hydra` (the
/// binary) has a test that compares this constant with that literal, so the two
/// cannot drift apart.
pub const GAME_CLOSED: &str = "[hydra] the game closed the connection";

/// The prefix an echoed command is shown with in the scrollback.
// UNKNOWN: the reference build's echo shape is not in the recovered literal
// pool; `> ` is the conventional front-end echo and is only ever drawn, never
// sent.
pub const ECHO_PREFIX: &str = "> ";

/// How often a live window asks for a repaint.
///
/// The reader is a background thread: without a timer the window would only
/// repaint on input, and a game line could sit unpainted indefinitely.
// UNKNOWN: 50 ms is a drawing decision (the binary's pacing constants are not
// recovered); it is a repaint hint, not a network poll.
pub const REPAINT: std::time::Duration = std::time::Duration::from_millis(50);

/// A live game session, folded into the models the window draws.
pub struct LiveSession {
    /// The character the window is for (the `hydra gui` argument).
    pub character: String,
    /// The story scrollback, fed from `Frame::Text` runs.
    pub story: Story,
    /// The room pane: title, description, exits and prompt.
    pub view: PlayView,
    /// What the character is holding.
    pub hands: Hand,
    /// The compass directions the game sent, in order, without duplicates.
    pub compass: Vec<String>,
    /// The room number from `<nav rm="…"/>`, as written.
    pub room_number: String,
    /// The vitals stack (`Sp`, `HP`, `MP`, `SP`).
    pub vitals: Stack,
    /// The theme whose palette the story is painted with.
    pub theme: Theme,
    /// Whether the game socket is still open.
    pub connected: bool,
    /// The input line's text (the `TextEdit`'s buffer lives here between frames).
    pub input: String,
    /// The room description, one entry per line.
    description: Vec<String>,
    /// The streaming parser, kept across lines (its bold/preset/stream state
    /// spans them).
    parser: Parser,
}

impl LiveSession {
    /// A session for `character` with an empty model and an open connection.
    pub fn new(character: impl Into<String>) -> LiveSession {
        LiveSession {
            character: character.into(),
            story: Story::new(),
            view: PlayView::default(),
            hands: Hand::default(),
            compass: Vec::new(),
            room_number: String::new(),
            vitals: Stack::default(),
            theme: Theme::default(),
            connected: true,
            input: String::new(),
            description: Vec::new(),
            parser: Parser::new(),
        }
    }

    /// The connection indicator's word.
    pub fn status(&self) -> &'static str {
        if self.connected {
            CONNECTED
        } else {
            DISCONNECTED
        }
    }

    /// Feed one wire line (with or without its terminator) through the raw-tag
    /// pass and the parser, and return the frames the parser produced.
    pub fn feed_line(&mut self, line: &str) -> Vec<Frame> {
        let line = line.trim_end_matches(['\r', '\n']);
        // The tag pass first: it is the only source of the attributes the
        // parser's dedicated frames drop, and it is where the room pane's
        // `component`/`preset`/element forms are read from.
        self.read_tags(line);
        let frames = self.parser.parse_line(line);
        self.feed_frames(&frames);
        // `</preset>` ends the game's style span, but the parser records markup
        // state for *opening* tags only (its closing branch returns through
        // `structural_or_unknown` before `markup_state`), so the style would
        // otherwise leak into every later line's runs.
        if line.contains("</preset>") {
            self.close_preset();
        }
        frames
    }

    /// Feed one wire line's **bytes** (with or without its terminator).
    ///
    /// A game line is bytes, not text: `Parser::decode_wire_line` keeps valid
    /// UTF-8 unchanged and maps every invalid byte to the corresponding
    /// `U+00xx` character (not `from_utf8_lossy`, which would insert `U+FFFD`),
    /// and this does the same so the reader thread never has to end a session
    /// over one bad byte.
    pub fn feed_bytes(&mut self, bytes: &[u8]) -> Vec<Frame> {
        match std::str::from_utf8(bytes) {
            Ok(text) => self.feed_line(text),
            Err(_) => {
                let text: String = bytes.iter().map(|&byte| byte as char).collect();
                self.feed_line(&text)
            }
        }
    }

    /// Clear the parser's `<preset>` style after a line that closed one.
    ///
    /// `<preset id=""/>` is the neutral opener: `run::preset_token("")` is
    /// `None`, so the following runs paint in the plain text colour, which is
    /// what "no preset" means.  Nothing from this line reaches the story — the
    /// frames are dropped on the floor.
    fn close_preset(&mut self) {
        let _ = self.parser.parse_line("<preset id=\"\"/>");
    }

    /// Fold one batch of parsed frames into the model.
    pub fn feed_frames(&mut self, frames: &[Frame]) {
        for frame in frames {
            match frame {
                Frame::Text(runs) => self.hear(runs),
                Frame::Prompt(prompt) => self.set_prompt(prompt.clone()),
                // A line the game echoed back: shown, but not re-sent.
                Frame::Echo(text) => {
                    if !text.trim().is_empty() {
                        self.story.see(text.clone());
                    }
                }
                // `ClientText` is the text of a client-side `<c>` region; it is
                // the player's own command coming back, so it is shown like an
                // echo and never fed to the room model.
                Frame::ClientText(text) => {
                    if !text.trim().is_empty() {
                        self.story.see(text.clone());
                    }
                }
                // `Structural`, the dedicated element frames and the unknown-tag
                // frames carry no text this model uses.
                _ => {}
            }
        }
    }

    /// A parsed text line: the story hears it, with the theme's palette.
    ///
    /// The room pane is filled from the **raw line** instead
    /// ([`LiveSession::read_tags`]), not from a run's `preset`: the parser's
    /// preset state is a whole-line span that can outlive the tag that set it
    /// (see [`LiveSession::close_preset`]), and a stale preset must never move
    /// the room.
    fn hear(&mut self, runs: &Runs) {
        if runs.is_empty() || runs.plain().trim().is_empty() {
            // A blank protocol line is dropped rather than taking a row.
            return;
        }
        self.story.hear(runs, &self.theme.palette);
    }

    /// Take the input line, and return the bare line to send — or `None` when
    /// there is nothing to send.
    ///
    /// The line is validated with [`hydra_ui::input::validate_line`]; a valid
    /// line is echoed into the story (`> look`) and returned trimmed, and an
    /// empty or whitespace-only line is dropped without a trace.  The caller
    /// owns the socket, so this never blocks and never sends.
    pub fn submit(&mut self) -> Option<String> {
        let typed = std::mem::take(&mut self.input);
        match input::validate_line(&typed) {
            Ok(()) => {
                let bare = typed.trim().to_string();
                self.story.see(format!("{ECHO_PREFIX}{bare}"));
                Some(bare)
            }
            Err(error) => {
                // Nothing is echoed for an empty line: pressing Enter on an empty
                // input is not an event.  Anything else is the validation's own
                // sentence, prefixed like every other local notice.
                if !matches!(error, InputError::Empty) {
                    self.story.see(format!("[hydra] {error}"));
                }
                None
            }
        }
    }

    /// The game closed the connection (or the socket failed): say so.
    ///
    /// The reader thread calls this once when its read ends.  The recovered
    /// sentence is [`GAME_CLOSED`]; no line is added twice.
    pub fn closed(&mut self) {
        self.connected = false;
        self.story.see(GAME_CLOSED);
    }

    /// Replace the prompt (the last line of the story, and the room pane's
    /// prompt).
    pub fn set_prompt(&mut self, prompt: impl Into<String>) {
        self.view.prompt = prompt.into();
    }

    /// A new room: its name, with the previous description dropped.
    ///
    /// The game sends the whole room block on every `look`, so a room name is
    /// the start of a description, not an addition to one.  (INFERRED: the
    /// reference build's `Room::component` replaces per component id; the
    /// description is kept as lines here only to support the preset form, which
    /// arrives one line at a time.)
    pub fn set_room_title(&mut self, title: impl Into<String>) {
        self.description.clear();
        self.view.room = title.into();
        self.rebuild();
    }

    /// Append one description line (the `<preset id="roomDesc">` form).
    pub fn push_description(&mut self, line: impl Into<String>) {
        let line = clean_line(&line.into());
        let line = line.trim_end();
        if line.trim().is_empty() {
            return;
        }
        self.description.push(line.to_string());
        self.rebuild();
    }

    /// Replace the whole description (the `<component id="roomDesc">` form).
    ///
    /// A component value can carry `&#10;` line breaks, so it is split into
    /// lines first and each line is cleaned.
    pub fn set_description(&mut self, description: &str) {
        self.description = decode(description)
            .lines()
            .map(|line| entities::strip_control_chars(line).trim_end().to_string())
            .filter(|line| !line.trim().is_empty())
            .collect();
        self.rebuild();
    }

    /// The room number from `<nav rm="…"/>`.
    pub fn set_room_number(&mut self, number: impl Into<String>) {
        self.room_number = number.into();
    }

    /// The description lines as the room pane has them.
    pub fn description(&self) -> &[String] {
        &self.description
    }

    /// Re-derive the projected view: the description text and its exit words.
    fn rebuild(&mut self) {
        self.view.description = self.description.join("\n");
        self.view.exits = view::exits_in(&self.view.description);
    }

    /// The raw-tag pass: the attributes the parser's dedicated frames drop.
    fn read_tags(&mut self, line: &str) {
        let mut in_compass = false;
        for (start, end) in tag_spans(line) {
            let tag = &line[start..end];
            let name = text::tag_name(tag);
            let closing = tag.starts_with("</");
            let self_closing = tag[..tag.len().saturating_sub(1)]
                .trim_end()
                .ends_with('/');
            let body = |close: &str| -> &str {
                if closing || self_closing {
                    return "";
                }
                let close = format!("</{close}>");
                match line[end..].find(&close) {
                    Some(at) => &line[end..end + at],
                    None => &line[end..],
                }
            };
            match name {
                "compass" => {
                    in_compass = !closing;
                    if !closing {
                        self.compass.clear();
                    }
                }
                "dir" if in_compass && !closing => {
                    if let Some(value) = attribute(tag, "value") {
                        if !value.is_empty() && !self.compass.contains(&value) {
                            self.compass.push(value);
                        }
                    }
                }
                "component" => {
                    if let (Some(id), Some(value)) =
                        (attribute(tag, "id"), attribute(tag, "value"))
                    {
                        match id.as_str() {
                            "roomName" => {
                                self.set_room_title(clean_line(&value).trim().to_string())
                            }
                            "roomDesc" => self.set_description(&value),
                            _ => {}
                        }
                    }
                }
                "nav" => {
                    if let Some(room) = attribute(tag, "rm") {
                        self.set_room_number(room);
                    }
                }
                "progressBar" => {
                    if let (Some(id), Some(value)) =
                        (attribute(tag, "id"), attribute(tag, "value"))
                    {
                        if let Some(percent) = percent(&value) {
                            self.set_vital(&id, percent);
                        }
                    }
                }
                "left" | "right" | "held" if !closing => {
                    // `hydra_model`'s own reader: it knows the recovered
                    // `nothing`/`empty` spellings, so the tag text is handed to
                    // it as the `<left>…</left>` line it expects.
                    let close = format!("</{name}>");
                    let slice = match line[end..].find(&close) {
                        Some(at) => &line[start..end + at + close.len()],
                        None => &line[start..],
                    };
                    self.hands.read(slice);
                }
                "preset" if !closing => {
                    // The preset form: `<preset id="roomDesc">text</preset>`.
                    let text = body("preset");
                    if !text.trim().is_empty() {
                        match attribute(tag, "id").as_deref() {
                            Some("roomName") => {
                                self.set_room_title(clean_line(text).trim().to_string())
                            }
                            Some("roomDesc") => self.push_description(text),
                            _ => {}
                        }
                    }
                }
                "roomName" => {
                    let title = body("roomName");
                    if !title.trim().is_empty() {
                        self.set_room_title(decode(title).trim().to_string());
                    }
                }
                "roomDesc" => {
                    let desc = body("roomDesc");
                    if !desc.trim().is_empty() {
                        self.push_description(decode(desc));
                    }
                }
                "prompt" => {
                    let prompt = body("prompt");
                    if !prompt.is_empty() {
                        self.set_prompt(decode(prompt));
                    }
                }
                _ => {}
            }
        }
    }

    /// Apply one `progressBar` percentage to the matching vital.
    fn set_vital(&mut self, id: &str, percent: u32) {
        let vital = Vital::new(percent);
        match id {
            "health" => self.vitals.health = vital,
            "mana" => self.vitals.mana = vital,
            "spirit" => self.vitals.spirit = vital,
            "stamina" => self.vitals.stamina = vital,
            // Any other bar (encumbrance, …) is not one of the four the stack
            // draws.
            _ => {}
        }
    }
}

/// An attribute value, entity-decoded.
///
/// This is [`entities::decode_entities`] only: the recovered
/// `decode_attribute_value` also strips control characters, which would remove
/// the `&#10;` line breaks a room description can carry, so the line splitting
/// ([`LiveSession::set_description`]) happens first and the control-character
/// cleanup is applied per line ([`clean_line`]).
fn attribute(tag: &str, name: &str) -> Option<String> {
    text::attribute(tag, name).map(|value| decode(&value))
}

/// Decode the entities of element body text (line breaks are kept).
fn decode(text: &str) -> String {
    entities::decode_entities(text)
}

/// One line of wire text: entities decoded and the recovered control-character
/// cleanup applied (`strip_control_chars` keeps tab and everything at or above
/// `U+00A0`).
fn clean_line(text: &str) -> String {
    entities::strip_control_chars(&decode(text))
}

/// A `progressBar` value as a percentage: `80`, `80%` and ` 80 ` all work.
fn percent(value: &str) -> Option<u32> {
    let trimmed = value.trim().trim_end_matches('%').trim();
    if let Ok(percent) = trimmed.parse::<u32>() {
        return Some(percent);
    }
    // A bar may carry `80/100` (or `80 of 100`) instead of a bare number.
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<u32>().ok()
}

/// The `(start, end)` byte ranges of every markup tag in `line`.
///
/// Quote-aware, so an attribute value containing `>` does not end the tag
/// early.  A `<` that is not followed by a `>` is left alone (it is prose).
fn tag_spans(line: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(offset) = line[at..].find('<') {
        let start = at + offset;
        let mut quote: Option<char> = None;
        let mut end = None;
        for (index, character) in line[start..].char_indices() {
            match character {
                '"' | '\'' if quote.is_none() => quote = Some(character),
                character if Some(character) == quote => quote = None,
                '>' if quote.is_none() => {
                    end = Some(start + index + 1);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            // A `<` with no closing `>` is prose; skip it and keep scanning, so
            // one stray bracket does not hide the rest of the line's tags.
            at = start + 1;
            continue;
        };
        out.push((start, end));
        at = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> LiveSession {
        LiveSession::new("Aelith")
    }

    /// The session's story as plain text, oldest first.
    fn story(session: &LiveSession) -> Vec<String> {
        session.story.text(session.story.lines().len())
    }

    #[test]
    fn a_text_frame_is_heard_with_the_themes_palette() {
        let mut session = session();
        let frames = session.feed_line("A synthetic breeze crosses the road.");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind(), "Text");
        assert_eq!(
            story(&session),
            vec!["A synthetic breeze crosses the road."]
        );
        // The palette is the theme's (`Theme::default()` is the Light theme).
        assert_eq!(session.theme, Theme::default());
        // The line is a `Token::Text` run, so it paints in the text colour.
        let line = session.story.lines()[0];
        assert_eq!(line.runs.len(), 1);
        assert_eq!(line.runs[0].token, Some(hydra_ui::theme::Token::Text));
        assert_eq!(line.runs[0].color, hydra_ui::theme::Token::Text.bare());
    }

    #[test]
    fn a_blank_line_is_dropped() {
        let mut session = session();
        session.feed_line("");
        session.feed_line("   ");
        assert!(session.story.is_empty());
    }

    #[test]
    fn the_room_name_component_sets_the_title() {
        let mut session = session();
        session.feed_line("<component id=\"roomName\" value=\"A Synthetic Crossing\"/>");
        assert_eq!(session.view.room, "A Synthetic Crossing");
        // `<nav rm>` carries the room number.
        session.feed_line("<nav rm=\"4242\"/>");
        assert_eq!(session.room_number, "4242");
    }

    #[test]
    fn the_room_desc_component_fills_the_description_and_its_exits() {
        let mut session = session();
        session.feed_line("<component id=\"roomName\" value=\"A Synthetic Crossing\"/>");
        session.feed_line(
            "<component id=\"roomDesc\" value=\"A wide synthetic road.&#10;Obvious exits: north, southeast.\"/>",
        );
        assert_eq!(
            session.description(),
            &[
                "A wide synthetic road.".to_string(),
                "Obvious exits: north, southeast.".to_string()
            ]
        );
        assert_eq!(session.view.exits, vec!["n".to_string(), "se".to_string()]);
        // The exit caption is not part of the drawn description lines.
        assert_eq!(session.view.described(), vec!["A wide synthetic road."]);
    }

    #[test]
    fn a_second_room_name_starts_a_new_description() {
        let mut session = session();
        session.feed_line("<component id=\"roomName\" value=\"One\"/>");
        session.feed_line("<component id=\"roomDesc\" value=\"First room.\"/>");
        session.feed_line("<component id=\"roomName\" value=\"Two\"/>");
        assert_eq!(session.description(), &[] as &[String]);
        session.feed_line("<component id=\"roomDesc\" value=\"Second room.\"/>");
        assert_eq!(session.description(), &["Second room.".to_string()]);
        assert_eq!(session.view.room, "Two");
        assert!(session.view.exits.is_empty());
    }

    #[test]
    fn the_preset_room_forms_fill_the_same_pane_and_the_story() {
        let mut session = session();
        session.feed_line("<preset id=\"roomName\">A Synthetic Crossing</preset>");
        session.feed_line("<preset id=\"roomDesc\">A wide synthetic road.</preset>");
        session.feed_line("<preset id=\"roomDesc\">Obvious exits: north, southeast.</preset>");
        assert_eq!(session.view.room, "A Synthetic Crossing");
        assert_eq!(
            session.description(),
            &[
                "A wide synthetic road.".to_string(),
                "Obvious exits: north, southeast.".to_string()
            ]
        );
        assert_eq!(session.view.exits, vec!["n".to_string(), "se".to_string()]);
        // The raw/preset room text is in the scrollback too.
        let story = story(&session);
        assert_eq!(
            story,
            vec![
                "A Synthetic Crossing",
                "A wide synthetic road.",
                "Obvious exits: north, southeast."
            ]
        );
    }

    #[test]
    fn the_unpreset_room_tags_are_read_as_well() {
        let mut session = session();
        session.feed_line("<roomName>A Synthetic Crossing</roomName>");
        session.feed_line("<roomDesc>A wide synthetic road.</roomDesc>");
        assert_eq!(session.view.room, "A Synthetic Crossing");
        assert_eq!(session.description(), &["A wide synthetic road.".to_string()]);
        // The body is still story text (the parser does not consume it).
        assert_eq!(story(&session).len(), 2);
    }

    #[test]
    fn progress_bars_fill_the_vitals_stack() {
        let mut session = session();
        // The stack starts out unknown (`?`), not at zero.
        assert_eq!(session.vitals.words(), "Sp ?, HP ?, MP ?, SP ?");
        for (id, value) in [
            ("health", "80"),
            ("mana", "90%"),
            ("spirit", "70"),
            ("stamina", "60/100"),
        ] {
            session.feed_line(&format!("<progressBar id=\"{id}\" value=\"{value}\"/>"));
        }
        // The recovered label order is `Sp`, `HP`, `MP`, `SP`.
        assert_eq!(session.vitals.words(), "Sp 70%, HP 80%, MP 90%, SP 60%");
        // An unknown bar changes nothing.
        session.feed_line("<progressBar id=\"encum\" value=\"12\"/>");
        assert_eq!(session.vitals.words(), "Sp 70%, HP 80%, MP 90%, SP 60%");
        // Percentages are clamped like the stack does.
        session.feed_line("<progressBar id=\"health\" value=\"250\"/>");
        assert_eq!(session.vitals.health.word("HP"), "HP 100%");
    }

    #[test]
    fn the_hands_are_read_from_the_left_right_and_held_tags() {
        let mut session = session();
        session.feed_line("<left>a synthetic shield</left>");
        assert_eq!(session.hands.left(), Some("a synthetic shield"));
        assert_eq!(session.hands.right(), None);
        session.feed_line("<right>a synthetic sword</right>");
        assert_eq!(session.hands.right(), Some("a synthetic sword"));
        // The recovered empty spellings clear the slot.
        session.feed_line("<right>nothing</right>");
        assert_eq!(session.hands.right(), None);
        session.feed_line("<held>a synthetic lantern</held>");
        assert_eq!(session.hands.right(), Some("a synthetic lantern"));
        // Both on one line works too.
        session.feed_line("<left>a shield</left><right>a sword</right>");
        assert_eq!(session.hands.left(), Some("a shield"));
        assert_eq!(session.hands.right(), Some("a sword"));
    }

    #[test]
    fn the_compass_directions_are_collected_in_order() {
        let mut session = session();
        let frames = session.feed_line("<compass><dir value=\"n\"/><dir value=\"se\"/></compass>");
        // The parser swallows the directions into one `Compass` frame.
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind(), "Compass");
        assert_eq!(session.compass, vec!["n".to_string(), "se".to_string()]);
        // A `<dir>` outside a compass is not a compass word.
        session.feed_line("<dir value=\"w\"/>");
        assert_eq!(session.compass, vec!["n".to_string(), "se".to_string()]);
    }

    #[test]
    fn the_prompt_arrives_as_a_frame_and_as_the_raw_tag() {
        // The frame form (the parser decodes the entities itself).
        let mut session = session();
        let frames = session.feed_line("<prompt>synthetic&gt;</prompt>");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind(), "Prompt");
        assert_eq!(session.view.prompt, "synthetic>");

        // The raw tag alone, without the parser's help.
        let mut raw = self::session();
        raw.read_tags("<prompt>synthetic&gt;</prompt>");
        assert_eq!(raw.view.prompt, "synthetic>");
    }

    #[test]
    fn a_closed_preset_does_not_colour_the_next_line() {
        use hydra_ui::theme::Token;
        let mut session = session();
        session.feed_line("<preset id=\"roomDesc\">A wide synthetic road.</preset>");
        session.feed_line("A synthetic breeze crosses the road.");
        let lines = session.story.lines();
        assert_eq!(lines.len(), 2);
        // The preset line paints as room text…
        assert_eq!(lines[0].runs[0].token, Some(Token::RoomDesc));
        // …and the next line is plain text.  The parser only records markup
        // state for opening tags, so without `close_preset` the room text
        // style would leak into every line after it.
        assert_eq!(lines[1].runs[0].token, Some(Token::Text));
        // The room pane is unchanged by the plain line.
        assert_eq!(session.description(), &["A wide synthetic road.".to_string()]);
    }

    #[test]
    fn the_parser_state_survives_between_lines() {
        let mut session = session();
        // `<pushBold/>`/`<popBold/>` state spans lines; the model keeps one
        // parser, so the second line is still bold.
        session.feed_line("<pushBold/>a loud line");
        session.feed_line("still loud");
        session.feed_line("<popBold/>quiet");
        let lines = session.story.lines();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].runs.iter().all(|run| run.bold));
        assert!(lines[1].runs.iter().all(|run| run.bold));
        assert!(lines[2].runs.iter().all(|run| !run.bold));
    }

    #[test]
    fn submit_sends_a_valid_line_and_echoes_it() {
        let mut session = session();
        session.input = "  look  ".to_string();
        assert_eq!(session.submit(), Some("look".to_string()));
        assert_eq!(session.input, "");
        assert_eq!(story(&session), vec!["> look".to_string()]);
    }

    #[test]
    fn submit_drops_an_empty_line() {
        let mut session = session();
        session.input = "   ".to_string();
        assert_eq!(session.submit(), None);
        assert!(session.story.is_empty());
        assert_eq!(session.input, "");
    }

    #[test]
    fn submit_reports_a_line_that_is_too_long() {
        let mut session = session();
        session.input = "x".repeat(input::MAX_LEN + 1);
        assert_eq!(session.submit(), None);
        let story = story(&session);
        assert_eq!(story.len(), 1);
        assert!(
            story[0].starts_with("[hydra] that line is"),
            "{story:?}"
        );
    }

    #[test]
    fn closing_the_game_says_so_once() {
        let mut session = session();
        assert_eq!(session.status(), CONNECTED);
        session.closed();
        assert!(!session.connected);
        assert_eq!(session.status(), DISCONNECTED);
        assert_eq!(story(&session), vec![GAME_CLOSED.to_string()]);
        assert_eq!(GAME_CLOSED, "[hydra] the game closed the connection");
    }

    #[test]
    fn tag_spans_are_quote_aware() {
        let line = "<component id=\"roomDesc\" value=\"a > b\"/>text";
        assert_eq!(tag_spans(line).len(), 1);
        let (start, end) = tag_spans(line)[0];
        assert_eq!(&line[start..end], "<component id=\"roomDesc\" value=\"a > b\"/>");
        // Prose with a stray `<` is left alone.
        assert!(tag_spans("a < b").is_empty());
        assert!(tag_spans("no tags here").is_empty());
    }

    #[test]
    fn the_character_and_status_are_the_windows_indicators() {
        let session = session();
        assert_eq!(session.character, "Aelith");
        assert_eq!(session.status(), "connected");
    }
}
