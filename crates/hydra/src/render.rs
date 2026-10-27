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
        assert_eq!(text_line(&Frame::event("Component")), None);
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
