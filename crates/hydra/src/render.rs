//! The textual report for parsed frames.
//!
//! # What the reference build prints
//!
//! `hydra::replay::replay` (`0x100205ff4`) does **not** use this module's
//! line format.  It writes, per frame, either
//!
//! * `writeln!(stdout, "{}", frame)` — the default mode.  The `Display`
//!   formatting function pointer loaded at `0x100206760`/`0x100206768` is the
//!   standard in-crate `Frame` formatter, so the reference build prints whatever
//!   `impl Display for Frame` renders; or
//! * `writeln!(stdout, "{}", renderer.frame(frame))` — the `--text` mode, which
//!   goes through `hydra::render::Renderer::frame` (`0x1002593ec`; the renderer
//!   keeps a paragraph flag and brackets `death`/`group` frames, see
//!   `NOTES.md`).
//!
//! `--summary` (`Show::Summary`) suppresses the per-frame lines and keeps only
//! the tally.  Every mode then ends with the recovered footer format
//! `"\n== "`, `" frames from "`, `" bytes\n"` (`__cstring` `0x103e24ce9`,
//! printed at `0x100206b1c`), i.e. `== {frames} frames from {bytes} bytes`.
//!
//! Our `hydra_protocol::Frame` has no `Display` impl (the reconstruction models
//! six of the original's variants and keeps the rest as data), so this crate
//! documents its own line format instead of reproducing the original's opaque
//! one:
//!
//! ```text
//! {index}\t{kind}\t{tag}\t{text}
//! ```
//!
//! * `index` is the 1-based frame number in stream order;
//! * `kind` is one of `text`, `prompt`, `echo`, `raw`, `merchant`, `tag`,
//!   `structural`, `client-text`, `client-region`;
//! * `tag` is the tag name for `tag` frames and `-` for every other kind;
//! * `text` is the frame's plain text, or the raw line for `raw` frames.
//!
//! Fields are tab separated, so the payload is escaped: `\` → `\\`, TAB →
//! `\t`, LF → `\n`, CR → `\r`, and any other C0 control or DEL → `\xNN`.
//! One frame is always exactly one line, which keeps the output
//! line-comparable (and therefore diffable and testable).

use hydra_protocol::Frame;

/// The frame kinds, in report order.
///
/// The last three are the protocol-audit variants added after this crate was
/// first written (`Structural`, `ClientText`, `ClientRegion`); they are kept
/// after the original six so the existing report remains stable.
pub const KINDS: [&str; 9] = [
    "text",
    "prompt",
    "echo",
    "raw",
    "merchant",
    "tag",
    "structural",
    "client-text",
    "client-region",
];

/// The kind word for a frame.
pub fn kind(frame: &Frame) -> &'static str {
    match frame {
        Frame::Text(_) => "text",
        Frame::Prompt(_) => "prompt",
        Frame::Echo(_) => "echo",
        Frame::Raw(_) => "raw",
        Frame::MerchantStatus(_) => "merchant",
        Frame::Unknown { .. } => "tag",
        Frame::Structural { .. } => "structural",
        Frame::ClientText(_) => "client-text",
        Frame::ClientRegion => "client-region",
    }
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

/// The `--text` line for one frame: the readable text, or `None` when the frame
/// carries none.
///
/// This is **not** `hydra::render::Renderer::frame` (`0x1002593ec`): that
/// renderer also breaks paragraphs (the `'\n'` it pushes at `0x100259474`/
/// `0x100259558`) and brackets `death`/`group` frames, and it was not ported.
/// See `NOTES.md`.
pub fn text_line(frame: &Frame) -> Option<String> {
    let text = frame.plain();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Per-kind frame counts, in [`KINDS`] order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    counts: [usize; KINDS.len()],
}

impl Counts {
    /// Count one frame.
    pub fn add(&mut self, frame: &Frame) {
        let word = kind(frame);
        if let Some(slot) = KINDS.iter().position(|k| *k == word) {
            self.counts[slot] += 1;
        }
    }

    /// The count for `kind`.
    pub fn of(&self, kind: &str) -> usize {
        KINDS
            .iter()
            .position(|k| *k == kind)
            .map(|i| self.counts[i])
            .unwrap_or(0)
    }

    /// `(kind, count)` pairs, including the zero counts, in [`KINDS`] order.
    pub fn pairs(&self) -> impl Iterator<Item = (&'static str, usize)> + '_ {
        KINDS.iter().copied().zip(self.counts.iter().copied())
    }

    /// The total number of frames counted.
    pub fn total(&self) -> usize {
        self.counts.iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_protocol::frame::payload::{Link, MerchantStatus};
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
    fn every_variant_has_a_kind_word() {
        assert_eq!(kind(&text_frame("hi")), "text");
        assert_eq!(kind(&Frame::Prompt(">".into())), "prompt");
        assert_eq!(kind(&Frame::Echo("hi".into())), "echo");
        assert_eq!(kind(&Frame::Raw("hi".into())), "raw");
        assert_eq!(
            kind(&Frame::MerchantStatus(MerchantStatus::Start)),
            "merchant"
        );
        assert_eq!(
            kind(&Frame::Unknown {
                tag: "room".into(),
                raw: "<room/>".into(),
                text: String::new()
            }),
            "tag"
        );
        assert_eq!(
            kind(&Frame::Structural {
                tag: "room".into(),
                text: "a room".into()
            }),
            "structural"
        );
        assert_eq!(kind(&Frame::ClientText("look".into())), "client-text");
        assert_eq!(kind(&Frame::ClientRegion), "client-region");
        // KINDS is the closed set `kind` produces.
        for frame in [
            text_frame("hi"),
            Frame::Prompt(">".into()),
            Frame::Echo("hi".into()),
            Frame::Raw("hi".into()),
            Frame::MerchantStatus(MerchantStatus::Stop),
            Frame::Structural {
                tag: "room".into(),
                text: String::new(),
            },
            Frame::ClientText("hi".into()),
            Frame::ClientRegion,
        ] {
            assert!(KINDS.contains(&kind(&frame)));
        }
    }

    #[test]
    fn text_frames_report_their_plain_text() {
        assert_eq!(
            frame_line(1, &text_frame("a blue gem glitters")),
            "1\ttext\t-\ta blue gem glitters"
        );
    }

    #[test]
    fn object_links_do_not_leak_markup_into_the_text_column() {
        let run = Run::with_link("a blue gem", Link::object("-1001", Some("gem")));
        let mut runs = Runs::new();
        runs.push(run);
        assert_eq!(frame_line(7, &Frame::Text(runs)), "7\ttext\t-\ta blue gem");
    }

    #[test]
    fn unknown_frames_show_their_tag_and_text() {
        let frame = Frame::Unknown {
            tag: "questState".into(),
            raw: "<questState id=\"7\">".into(),
            text: "ready".into(),
        };
        assert_eq!(frame_line(2, &frame), "2\ttag\tquestState\tready");
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
            "1\traw\t-\ttwo\\nlines"
        );
    }

    #[test]
    fn text_lines_skip_empty_frames() {
        assert_eq!(text_line(&text_frame("hi")).as_deref(), Some("hi"));
        assert_eq!(
            text_line(&Frame::Unknown {
                tag: "room".into(),
                raw: "<room/>".into(),
                text: String::new()
            }),
            None
        );
        assert_eq!(text_line(&Frame::Raw(String::new())), None);
    }

    #[test]
    fn counts_add_up_per_kind() {
        let mut counts = Counts::default();
        counts.add(&text_frame("one"));
        counts.add(&text_frame("two"));
        counts.add(&Frame::Prompt(">".into()));
        assert_eq!(counts.of("text"), 2);
        assert_eq!(counts.of("prompt"), 1);
        assert_eq!(counts.of("raw"), 0);
        assert_eq!(counts.total(), 3);
        let pairs: Vec<_> = counts.pairs().collect();
        assert_eq!(pairs[0], ("text", 2));
        assert_eq!(pairs[1], ("prompt", 1));
        assert_eq!(pairs.len(), KINDS.len());
    }
}
