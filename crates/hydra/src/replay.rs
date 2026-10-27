//! `hydra replay <file>` — the headless gate.
//!
//! Recovered from two crate-local functions:
//!
//! * `hydra::replay::Show::from_args` (`0x100205f3c`, 184 bytes): scans the
//!   whole token list for a **9-byte** `--summary` (`mov x10, #0x2d2d` …
//!   `movk #0x7261, lsl #48` + `mov w11, #0x79`, `0x100205f4c`) and returns
//!   `2`; failing that, for a **6-byte** `--text` (`mov w10, #0x2d2d` /
//!   `movk #0x6574` + `mov w11, #0x7478`, `0x100205fa4`) and returns `1`;
//!   otherwise `0`.  The scan never skips the subcommand token, so a file
//!   literally called `--text` would be read as the flag as well.
//! * `hydra::replay::replay` (`0x100205ff4`, 5288 bytes):
//!
//!   1. `std::fs::read(file)` (`0x10020602c`).  On error it returns
//!      `Some(format!("{}: {}", file, err))` — the `{:?}`-free `"{}: {}"` piece
//!      table is at `0x103e21615` — which `hydra::main` prints to stderr before
//!      exiting 1 (`0x10021dffc`).
//!   2. Builds a `Parser` on the stack (`0x1002060c4` … `0x10020610c`: the
//!      line buffer, the `Vec::new()` dangling pointers `(0, 8, 0)` and the
//!      `<settings>` window) and locks stdout (`0x100206110`/`0x10020611c`).
//!   3. Per 4 KiB piece: `push_bytes` (`0x100206174`), then per frame
//!      (`Frame` stride `0x118`, `madd x10, x10, #0x118, x1` at `0x100206208`)
//!      it counts the frame kind and, unless `--summary` was given, writes one
//!      line (`0x100206758` `{}`-of-`Frame` for the default mode,
//!      `0x1002068b4` `hydra::render::Renderer::frame` for `--text`).
//!   4. `Parser::finish` (`0x1002061ac`) for the tail.
//!   5. The footer `"\n== {} frames from {} bytes\n"` (`0x103e24ce9`, printed
//!      at `0x100206b1c` with the total frame count and `data.len()`) and, when
//!      the tally is non-empty, the per-kind counts (`0x100206bfc` …).
//!
//! Replay never touches Lich, the session or the network: it is
//! `std::fs::read` + [`hydra_protocol::Parser`] + stdout, which is why it can
//! serve as the phase gate.
//!
//! # Report format
//!
//! See [`crate::render`] for the documented line format; the footer text is the
//! recovered one.

use std::fs;
use std::io::{self, Write};

use hydra_protocol::Parser;

use crate::render::{self, Counts};
use crate::source::{pump, Replay};

/// What `hydra replay` was asked to show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Show {
    /// One line per frame (the reference build's default).
    #[default]
    Default,
    /// Readable text only (`--text`).
    Text,
    /// Counts per frame kind, no per-frame lines (`--summary`).
    Summary,
}

impl Show {
    /// The recovered value of `Show::from_args` (`0x100205f3c`).
    pub fn as_u8(self) -> u8 {
        match self {
            Show::Default => 0,
            Show::Text => 1,
            Show::Summary => 2,
        }
    }

    /// `Show::from_args(&tokens)` — `--summary` wins over `--text`, exactly like
    /// the recovered scan, which looks for `--summary` first.
    pub fn from_args(tokens: &[String]) -> Show {
        if tokens.iter().any(|t| t == "--summary") {
            Show::Summary
        } else if tokens.iter().any(|t| t == "--text") {
            Show::Text
        } else {
            Show::Default
        }
    }
}

/// What one replay reported.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Bytes read from the file.
    pub bytes: usize,
    /// Frames the parser produced (including the `finish` flush).
    pub frames: usize,
    /// Per-kind counts.
    pub counts: Counts,
    /// Every tag name the parser did not recognise, with its occurrence count
    /// (open *and* close counted separately), as the footer prints it.
    pub unknown: std::collections::BTreeMap<String, usize>,
}

/// Read `file` and replay it, writing the report to stdout.
///
/// The error string is the recovered `"{file}: {io error}"` shape; `main`
/// prints it to stderr and exits 1.  (The recovered `hydra::replay::replay`
/// takes the [`Show`] mode as its third argument, hence the name.)
pub fn replay_show(file: &str, show: Show) -> Result<Summary, String> {
    let data = fs::read(file).map_err(|err| format!("{file}: {err}"))?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let summary = replay_bytes(&data, show, &mut out).map_err(|err| err.to_string())?;
    // The reference build drops a failing stdout on the floor (the `io::Error`
    // at 0x1002067fc / 0x1002068fc is discarded), so a closed pipe is not an
    // error there either; match it, or the exit status would differ.
    let _ = out.flush();
    Ok(summary)
}

/// The replay core: feed `data` through a fresh [`hydra_protocol::Parser`] and
/// write the report to `out`.
///
/// Split out from [`replay_show`] so tests can drive it without a file or a
/// terminal.  Per-frame lines are written as they are decoded (the default and
/// `--text` modes); the footer — the recovered `\n== {frames} frames from
/// {bytes} bytes`, the per-kind histogram and the unknown-tag block — is
/// written in every mode, exactly as the reference build does.
pub fn replay_bytes<W: Write>(data: &[u8], show: Show, out: &mut W) -> io::Result<Summary> {
    let mut parser = Parser::new();
    let mut source = Replay::new(data.to_vec());
    let mut counts = Counts::default();
    let mut index = 0usize;
    let mut broken = false;

    let frames = pump(&mut source, &mut parser, |frame| {
        index += 1;
        counts.add(&frame);
        if broken {
            return;
        }
        let written = match show {
            Show::Default => writeln!(out, "{}", render::frame_line(index, &frame)),
            Show::Text => match render::text_line(&frame) {
                Some(text) => writeln!(out, "{text}"),
                None => Ok(()),
            },
            Show::Summary => Ok(()),
        };
        // A failing stdout stops the per-frame lines but is not an error: the
        // reference build drops the `io::Error` (0x1002067fc / 0x1002068fc).
        if written.is_err() {
            broken = true;
        }
    });

    let unknown = parser.unknown_tags().clone();
    // The footer is best-effort for the same reason.
    let _ = write_footer(out, frames, data.len(), &counts, &unknown);

    Ok(Summary {
        bytes: data.len(),
        frames,
        counts,
        unknown,
    })
}

/// The recovered footer: `"\n== "`, `" frames from "`, `" bytes\n"`, the
/// per-kind histogram `"{count:>8}  {Variant}"` for the non-zero kinds in name
/// order, and then either `"== no unknown tags"` or the `"== unknown tags"`
/// block with one `"{count:>8}  <{tag}>"` line per name.
fn write_footer<W: Write>(
    out: &mut W,
    frames: usize,
    bytes: usize,
    counts: &Counts,
    unknown: &std::collections::BTreeMap<String, usize>,
) -> io::Result<()> {
    writeln!(out, "\n== {frames} frames from {bytes} bytes")?;
    for (kind, count) in counts.pairs() {
        writeln!(out, "{count:>8}  {kind}")?;
    }
    if unknown.is_empty() {
        writeln!(out, "== no unknown tags")?;
    } else {
        writeln!(out, "== unknown tags")?;
        for (name, count) in unknown {
            writeln!(out, "{count:>8}  <{name}>")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::REPLAY_CHUNK;

    fn toks(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn show_from_args_follows_the_recovered_scan() {
        assert_eq!(Show::from_args(&toks(&["replay", "f.xml"])), Show::Default);
        assert_eq!(
            Show::from_args(&toks(&["replay", "f.xml", "--text"])),
            Show::Text
        );
        assert_eq!(
            Show::from_args(&toks(&["replay", "f.xml", "--summary"])),
            Show::Summary
        );
        // `--summary` is looked for first, so it wins even when it comes last.
        assert_eq!(
            Show::from_args(&toks(&["replay", "--text", "f.xml", "--summary"])),
            Show::Summary
        );
        // Anything else in the list is ignored, wherever it sits.
        assert_eq!(
            Show::from_args(&toks(&["replay", "f.xml", "--nope"])),
            Show::Default
        );
    }

    #[test]
    fn show_matches_the_recovered_discriminants() {
        assert_eq!(Show::Default.as_u8(), 0);
        assert_eq!(Show::Text.as_u8(), 1);
        assert_eq!(Show::Summary.as_u8(), 2);
    }

    #[test]
    fn an_empty_recording_succeeds_with_no_frames() {
        let mut out = Vec::new();
        let summary = replay_bytes(b"", Show::Default, &mut out).unwrap();
        assert_eq!(summary.frames, 0);
        assert_eq!(summary.bytes, 0);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "\n== 0 frames from 0 bytes\n== no unknown tags\n");
        assert!(summary.unknown.is_empty());
    }

    #[test]
    fn the_default_report_is_one_line_per_frame_plus_the_footer() {
        let data = b"you feel better\r\n<prompt>&gt;</prompt>\r\n";
        let mut out = Vec::new();
        let summary = replay_bytes(data, Show::Default, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            concat!(
                "1\tText\t-\tyou feel better\n",
                "2\tPrompt\t-\t>\n",
                "\n== 2 frames from 40 bytes\n",
                "       1  Prompt\n",
                "       1  Text\n",
                "== no unknown tags\n"
            )
        );
        assert_eq!(summary.frames, 2);
        assert_eq!(summary.bytes, 40);
        assert_eq!(summary.counts.of("Text"), 1);
        assert_eq!(summary.counts.of("Prompt"), 1);
    }

    #[test]
    fn the_summary_mode_counts_instead_of_listing() {
        let data = b"one\ntwo\n<prompt>&gt;</prompt>\n";
        let mut out = Vec::new();
        let summary = replay_bytes(data, Show::Summary, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            concat!(
                "\n== 3 frames from 30 bytes\n",
                "       1  Prompt\n",
                "       2  Text\n",
                "== no unknown tags\n"
            )
        );
        assert_eq!(summary.counts.total(), 3);
    }

    #[test]
    fn the_text_mode_prints_readable_text_only() {
        let data = b"<pushBold/>bold words<popBold/>\n<prompt>&gt;</prompt>\n";
        let mut out = Vec::new();
        replay_bytes(data, Show::Text, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        // `--text` writes the readable text, then the same recovered footer.
        assert_eq!(
            text,
            concat!(
                "bold words\n",
                ">\n",
                "\n== 4 frames from 54 bytes\n",
                "       1  Prompt\n",
                "       2  Structural\n",
                "       1  Text\n",
                "== no unknown tags\n"
            )
        );
    }

    #[test]
    fn a_missing_file_is_the_recovered_error_shape() {
        let err = replay_show("/nonexistent/hydra/replay/fixture", Show::Default).unwrap_err();
        assert!(
            err.starts_with("/nonexistent/hydra/replay/fixture: "),
            "{err:?}"
        );
        assert!(err.contains("No such file"), "{err:?}");
    }

    #[test]
    fn the_recording_is_read_in_recovered_chunk_sizes() {
        // 2 × 0x1000 + 1 bytes of plain text still parse as one logical stream.
        let mut data = vec![b'a'; REPLAY_CHUNK * 2];
        data.push(b'\n');
        let mut out = Vec::new();
        let summary = replay_bytes(&data, Show::Summary, &mut out).unwrap();
        // A single over-long-but-legal line: 8193 bytes < the 0x40000 limit, so
        // it is one text frame.
        assert_eq!(summary.frames, 1);
        assert_eq!(summary.bytes, REPLAY_CHUNK * 2 + 1);
    }
}
