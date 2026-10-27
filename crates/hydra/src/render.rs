//! The textual report for parsed frames.
//!
//! # What the reference build prints
//!
//! `hydra::replay::replay` (`0x100205ff4`) tallies the frames it decodes and,
//! after `Parser::finish`, writes the recovered footer — the literals `"\n== "`,
//! `" frames from "`, `" bytes\n"` at `__cstring 0x103e24ce9`, printed at
//! `0x100206b1c` — followed by the per-kind counts and the unknown-tag block:
//!
//! ```text
//! ␊== {frames} frames from {bytes} bytes␊
//! {count:>8}  {Variant}␊            # only the non-zero kinds, sorted by name
//! == no unknown tags␊               # or:
//! == unknown tags␊
//! {count:>8}  <{tag}>␊              # one line per name, sorted, open+close counted
//! ```
//!
//! The histogram is printed in **every** mode, not just `--summary`:
//! `--summary` merely suppresses the per-frame lines the default and `--text`
//! modes write before it.
//!
//! Our default mode still prints one line per frame in this crate's own
//! documented format (`{index}\t{kind}\t{tag}\t{text}`, see
//! [`crate::render::frame_line`]) because the reference build prints
//! `Display for Frame`, which the reconstruction does not implement; the
//! histogram after it is the recovered one.
//!
//! `--text` is [`TextRender`]: the readable text of the recording, line
//! buffered the way the reference build's `hydra::render::Renderer::frame`
//! (`0x1002593ec`) buffers it, and byte-identical to it on every fixture.  See
//! `NOTES.md` §7.2.

use hydra_protocol::Frame;

/// Every kind word the original's histogram can print, alphabetically.
///
/// These are the original `Frame` variant names (`Debug for Frame`'s switch);
/// [`kind`] names the frames this crate emits, and
/// [`hydra_protocol::Frame::Event`] carries the rest verbatim.
pub const KINDS: [&str; 54] = [
    "ActiveEffect",
    "AimTime",
    "AppInfo",
    "CastTime",
    "ClearContainer",
    "ClearDialogData",
    "ClearStream",
    "ClientCommand",
    "ClientRegion",
    "ClientSettings",
    "CloseDialog",
    "CmdListUpdate",
    "CmdTimestamp",
    "Compass",
    "Component",
    "Container",
    "ContainerItem",
    "CreatureStatus",
    "DeleteContainer",
    "DialogOpen",
    "DialogPanelOpen",
    "DialogWidgets",
    "EbonGateMerchant",
    "Echo",
    "EndSetup",
    "Expose",
    "InjuryImage",
    "InlineImage",
    "InventoryManager",
    "InventoryViewItem",
    "Label",
    "LaunchUrl",
    "LeftHand",
    "MalformedTag",
    "MenuResponse",
    "MerchantStatus",
    "ObjectivesUpdate",
    "PantheonStatus",
    "PlayerId",
    "ProgressBar",
    "Prompt",
    "Pulse",
    "QuickbarSwitch",
    "Raw",
    "RightHand",
    "RoomId",
    "RoomMeta",
    "RoomPicture",
    "RoundTime",
    "SettingsInfo",
    "StatusIndicator",
    "Structural",
    "Text",
    "UnknownTag",
];

/// The kind word for a frame: the original variant name.
pub fn kind(frame: &Frame) -> &'static str {
    frame.kind()
}

/// Escape a payload so it never spans more than one line.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// The report line for one frame (without the trailing newline).
pub fn frame_line(index: usize, frame: &Frame) -> String {
    let tag = frame.tag().unwrap_or("-");
    format!(
        "{}\t{}\t{}\t{}",
        index,
        kind(frame),
        escape(tag),
        escape(&frame.plain())
    )
}

/// The `--text` renderer: the readable text of a recording, one line buffer at
/// a time.
///
/// Only two frame kinds print anything: `Text` contributes its plain text and
/// `Prompt` its prompt string.  Everything else — `Structural`, `Component`,
/// `DialogWidgets`, `MalformedTag`, `ClientCommand`, `UnknownTag`, … —
/// contributes the empty string, and `UnknownTag` does *not* print its raw tag.
/// `Text` frames are **not** one line each: within one input line they coalesce
/// into the buffer, and the buffer is broken only when
///
/// * the input line ends after visible text ([`Self::end_line`]),
/// * a `Prompt` frame arrives ([`Self::frame`] flushes first, even a previous
///   prompt), or
/// * the recording ends ([`Self::finish`]).
///
/// A prompt's buffer therefore survives across input lines until the next
/// `Text`/`Prompt`, and a prompt pending at the very end is left
/// **unterminated**: the report footer's leading `'\n'` terminates it.
///
/// One more rule, derived from the reference build: a `Text` frame produced
/// while a stream is open (`<stream …>`/`<pushStream …>` not yet popped) is not
/// printed at all and does not touch the buffer — that is why the `inv`/stream
/// dumps in a recording never show up in `--text`.  Prompts are always printed
/// (the parser emits its `StreamPopForced` frames before one).  The stream
/// depth is tracked from the frame sequence itself.
///
/// The return value of each method is the exact bytes to append to stdout (a
/// complete line, terminator included), or `None` when nothing is written.
#[derive(Clone, Debug, Default)]
pub struct TextRender {
    buf: String,
    pending: Pending,
    /// Open streams, tracked from the frames (`StreamPush` opens one,
    /// `StreamPop` closes one, `StreamPopForced` empties the stack).
    streams: usize,
}

/// What [`TextRender::buf`] currently holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Pending {
    /// Nothing buffered.
    #[default]
    Empty,
    /// Coalesced text of the current input line.
    Text,
    /// Prompt text, which survives across input lines.
    Prompt,
}

impl TextRender {
    /// A renderer with an empty buffer.
    pub fn new() -> Self {
        TextRender::default()
    }

    /// Feed one frame; returns the line it completed, if any.
    pub fn frame(&mut self, frame: &Frame) -> Option<String> {
        match frame {
            Frame::Text(runs) => {
                if self.streams > 0 {
                    // A stream dump: displayed nowhere, and it does not even
                    // break the pending line.
                    return None;
                }
                // Text ending a prompt breaks the prompt line first.
                let out = if self.pending == Pending::Prompt {
                    self.break_line()
                } else {
                    None
                };
                self.buf.push_str(&runs.plain());
                self.pending = Pending::Text;
                out
            }
            Frame::Prompt(text) => {
                // A prompt always starts a new line — even after a prompt.
                let out = self.break_line();
                self.buf.push_str(text);
                self.pending = Pending::Prompt;
                out
            }
            _ => {
                self.track_streams(frame);
                None
            }
        }
    }

    /// Break the line at the end of an input line that produced visible text.
    ///
    /// A pending prompt is *not* broken here: it survives until the next
    /// `Text`/`Prompt`, or until [`Self::finish`].
    pub fn end_line(&mut self) -> Option<String> {
        if self.pending == Pending::Text {
            self.break_line()
        } else {
            None
        }
    }

    /// End of recording: a pending text line is broken, and a pending prompt is
    /// written **without** a terminator — the report footer's leading `'\n'`
    /// terminates it.
    pub fn finish(&mut self) -> Option<String> {
        match self.pending {
            Pending::Empty => None,
            Pending::Text => self.break_line(),
            Pending::Prompt => {
                self.pending = Pending::Empty;
                Some(std::mem::take(&mut self.buf))
            }
        }
    }

    /// `buf` plus the line terminator, or `None` when nothing is buffered.
    fn break_line(&mut self) -> Option<String> {
        if self.pending == Pending::Empty {
            return None;
        }
        let mut line = std::mem::take(&mut self.buf);
        line.push('\n');
        self.pending = Pending::Empty;
        Some(line)
    }

    /// Follow the stream stack through the frame sequence.
    fn track_streams(&mut self, frame: &Frame) {
        match frame.kind() {
            "StreamPush" => self.streams += 1,
            "StreamPop" => self.streams = self.streams.saturating_sub(1),
            // `<prompt>` empties the stack (one frame per open stream).
            "StreamPopForced" => self.streams = 0,
            _ => {}
        }
    }
}

/// Per-kind frame counts.
///
/// Sorted by kind name, which is also the order the histogram prints in; only
/// the kinds that occurred are kept, because a zero row is never printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    counts: std::collections::BTreeMap<&'static str, usize>,
}

impl Counts {
    /// Count one frame.
    pub fn add(&mut self, frame: &Frame) {
        *self.counts.entry(kind(frame)).or_insert(0) += 1;
    }

    /// The count for `kind`.
    pub fn of(&self, kind: &str) -> usize {
        self.counts.get(kind).copied().unwrap_or(0)
    }

    /// `(kind, count)` pairs in name order, zero counts omitted.
    pub fn pairs(&self) -> impl Iterator<Item = (&'static str, usize)> + '_ {
        self.counts.iter().map(|(kind, count)| (*kind, *count))
    }

    /// The total number of frames counted.
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_protocol::frame::payload::Link;
    use hydra_protocol::{Run, Runs};

    fn text_frame(text: &str) -> Frame {
        let mut runs = Runs::new();
        runs.push(Run {
            text: text.to_string(),
            ..Run::default()
        });
        Frame::Text(runs)
    }

    #[test]
    fn every_variant_has_the_original_kind_word() {
        assert_eq!(kind(&text_frame("hi")), "Text");
        assert_eq!(kind(&Frame::Prompt(">".into())), "Prompt");
        assert_eq!(kind(&Frame::Raw("hi".into())), "Raw");
        assert_eq!(
            kind(&Frame::Unknown {
                tag: "foo".into(),
                raw: "<foo/>".into(),
                text: String::new()
            }),
            "UnknownTag"
        );
        assert_eq!(
            kind(&Frame::Structural {
                tag: "b".into(),
                text: "<b>".into()
            }),
            "Structural"
        );
        assert_eq!(kind(&Frame::ClientText("look".into())), "ClientCommand");
        assert_eq!(kind(&Frame::ClientRegion), "ClientRegion");
        assert_eq!(kind(&Frame::event("Component")), "Component");
        assert_eq!(kind(&Frame::event("StreamPopForced")), "StreamPopForced");
    }

    #[test]
    fn the_kind_list_is_sorted_and_covers_every_variant() {
        assert!(KINDS.windows(2).all(|w| w[0] < w[1]), "KINDS is sorted");
        assert_eq!(KINDS.len(), 54);
        // Every kind the reconstructed variants can produce is listed.
        for frame in [
            text_frame("hi"),
            Frame::Prompt(">".into()),
            Frame::Echo("hi".into()),
            Frame::Raw("hi".into()),
            Frame::MerchantStatus(hydra_protocol::MerchantStatus::Stop),
            Frame::Unknown {
                tag: "foo".into(),
                raw: "<foo/>".into(),
                text: String::new(),
            },
            Frame::structural("b", "<b>"),
            Frame::ClientText("hi".into()),
            Frame::ClientRegion,
        ] {
            assert!(KINDS.contains(&kind(&frame)), "{}", kind(&frame));
        }
    }

    #[test]
    fn text_frames_report_their_plain_text() {
        assert_eq!(
            frame_line(1, &text_frame("a blue gem glitters")),
            "1\tText\t-\ta blue gem glitters"
        );
    }

    #[test]
    fn object_links_do_not_leak_markup_into_the_text_column() {
        let run = Run::with_link("a blue gem", Link::object("-1001", Some("gem")));
        let mut runs = Runs::new();
        runs.push(run);
        assert_eq!(frame_line(7, &Frame::Text(runs)), "7\tText\t-\ta blue gem");
    }

    #[test]
    fn unknown_frames_show_their_tag_and_text() {
        let frame = Frame::Unknown {
            tag: "questState".into(),
            raw: "<questState id=\"7\">".into(),
            text: "ready".into(),
        };
        assert_eq!(frame_line(2, &frame), "2\tUnknownTag\tquestState\tready");
    }

    #[test]
    fn the_payload_is_escaped_to_one_line() {
        assert_eq!(escape("a\tb\nc\rd\\e"), "a\\tb\\nc\\rd\\\\e");
        assert_eq!(escape("bell\x07"), "bell\\x07");
        assert_eq!(escape("del\x7f"), "del\\x7f");
        // UTF-8 is kept as it is: only ASCII control characters are escaped.
        assert_eq!(escape("Vaalor’s gate — ok"), "Vaalor’s gate — ok");
        assert_eq!(
            frame_line(1, &Frame::Raw("two\nlines".into())),
            "1\tRaw\t-\ttwo\\nlines"
        );
    }

    /// Feed one input line's frames, then break the line like a recording line
    /// end does; returns everything written.
    fn render_line(render: &mut TextRender, frames: &[Frame]) -> String {
        let mut out = String::new();
        for frame in frames {
            if let Some(line) = render.frame(frame) {
                out.push_str(&line);
            }
        }
        if let Some(line) = render.end_line() {
            out.push_str(&line);
        }
        out
    }

    #[test]
    fn text_frames_coalesce_within_one_input_line() {
        let mut render = TextRender::new();
        // `A<b>B</b>C` -> Text, Structural, Text, Structural, Text: one line.
        let line = render_line(
            &mut render,
            &[
                text_frame("A"),
                Frame::structural("b", "<b>"),
                text_frame("B"),
                Frame::structural("b", "</b>"),
                text_frame("C"),
            ],
        );
        assert_eq!(line, "ABC\n");
        // The next input line starts a fresh buffer.
        assert_eq!(render_line(&mut render, &[text_frame("next")]), "next\n");
        assert_eq!(render.finish(), None);
    }

    #[test]
    fn a_prompt_breaks_the_pending_line_and_persists_until_text() {
        let mut render = TextRender::new();
        // `A<prompt>&gt;</prompt>`: the text line breaks, the prompt stays open.
        assert_eq!(
            render_line(&mut render, &[text_frame("A"), Frame::Prompt(">".into())]),
            "A\n"
        );
        // A prompt does not survive its own line end...
        assert_eq!(render_line(&mut render, &[]), "");
        // ... but the *next* prompt breaks it, just like text does.
        assert_eq!(
            render_line(&mut render, &[Frame::Prompt("#".into())]),
            ">\n"
        );
        // Text after a prompt breaks the prompt line first, and its own line
        // ends with the input line.
        assert_eq!(render_line(&mut render, &[text_frame("B")]), "#\nB\n");
        assert_eq!(render.finish(), None);
    }

    #[test]
    fn a_prompt_pending_at_the_end_is_left_unterminated() {
        let mut render = TextRender::new();
        assert_eq!(render_line(&mut render, &[Frame::Prompt(">".into())]), "");
        // No `"\n"`: the report footer's leading newline terminates the prompt.
        assert_eq!(render.finish().as_deref(), Some(">"));
        assert_eq!(render.finish(), None);
    }

    #[test]
    fn a_blank_input_line_writes_a_blank_line() {
        let mut render = TextRender::new();
        // `"\n"` is one empty `Text` frame, which still breaks the line.
        assert_eq!(render_line(&mut render, &[text_frame("")]), "\n");
        assert_eq!(render_line(&mut render, &[text_frame("")]), "\n");
        // An empty line after a prompt breaks the prompt, then writes a blank.
        let mut render = TextRender::new();
        assert_eq!(render_line(&mut render, &[Frame::Prompt(">".into())]), "");
        assert_eq!(render_line(&mut render, &[text_frame("")]), ">\n\n");
    }

    #[test]
    fn only_text_and_prompt_frames_print_anything() {
        let mut render = TextRender::new();
        let frames = [
            Frame::Unknown {
                tag: "room".into(),
                raw: "<room/>".into(),
                text: "leaked".into(),
            },
            Frame::structural("b", "<b>"),
            Frame::event("Component"),
            Frame::ClientText("look".into()),
            Frame::ClientRegion,
            Frame::Raw("raw".into()),
        ];
        assert_eq!(render_line(&mut render, &frames), "");
        assert_eq!(render.finish(), None, "nothing was buffered");
    }

    #[test]
    fn text_produced_while_a_stream_is_open_is_hidden() {
        let mut render = TextRender::new();
        // `<stream id="s">hid</stream>shown`: the dump is dropped, and it does
        // not even break the line, so `A` and `shown` coalesce.
        let line = render_line(
            &mut render,
            &[
                text_frame("A"),
                Frame::event("StreamPush"),
                text_frame("hid"),
                Frame::event("StreamPop"),
                text_frame("shown"),
            ],
        );
        assert_eq!(line, "Ashown\n", "the stream text is not displayed");
        // A stream left open across input lines hides everything after it.
        let line = render_line(
            &mut render,
            &[
                Frame::event("StreamPush"),
                text_frame("one"),
                Frame::event("StreamPush"),
                text_frame("two"),
            ],
        );
        assert_eq!(line, "");
        // `<prompt>` empties the stream stack (`StreamPopForced`), so the
        // prompt is always displayed and text after it is visible again.
        assert_eq!(
            render_line(
                &mut render,
                &[
                    Frame::event("StreamPopForced"),
                    Frame::event("StreamPopForced"),
                    Frame::Prompt(">".into()),
                    text_frame("visible"),
                ]
            ),
            ">\nvisible\n"
        );
    }

    #[test]
    fn a_stream_pop_without_a_push_does_not_underflow() {
        let mut render = TextRender::new();
        assert_eq!(
            render_line(&mut render, &[Frame::event("StreamPop"), text_frame("shown")]),
            "shown\n"
        );
    }

    #[test]
    fn counts_are_kept_per_kind_in_name_order_and_skip_zeros() {
        let mut counts = Counts::default();
        counts.add(&text_frame("one"));
        counts.add(&text_frame("two"));
        counts.add(&Frame::Prompt(">".into()));
        counts.add(&Frame::event("Component"));
        assert_eq!(counts.of("Text"), 2);
        assert_eq!(counts.of("Prompt"), 1);
        assert_eq!(counts.of("Raw"), 0);
        assert_eq!(counts.total(), 4);
        let pairs: Vec<_> = counts.pairs().collect();
        assert_eq!(
            pairs,
            vec![("Component", 1), ("Prompt", 1), ("Text", 2)],
            "the histogram sorts by kind name and prints no zero rows"
        );
    }
}
