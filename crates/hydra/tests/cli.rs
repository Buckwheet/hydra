//! The `hydra` binary, exercised the way a user runs it.
//!
//! Everything here goes through `env!("CARGO_BIN_EXE_hydra")`, i.e. the binary
//! `cargo` built for this package, so the argument handling, the exit statuses
//! and the report format are covered end to end.
//!
//! The fixture `tests/fixtures/small_stream.xml` is hand-written from the wire
//! syntax documented in `crates/hydra_protocol` (`parser.rs`'s tag table,
//! `tags.rs`'s 128-name vocabulary, `text/entities.rs`'s entity list): one line
//! each of a `<preset>` name, a structural `<room/>`, plain text with `&lt;`
//! entities, an object link, a bold span with UTF-8, an unknown tag and a
//! prompt, CRLF-terminated like a Lich relay's stdout.  No game data is invented.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_hydra")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("the hydra binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

/// The exact default report for `small_stream.xml` (316 bytes, 16 frames).
///
/// The per-frame line is this crate's own documented format; the histogram
/// after it is the reference build's byte-for-byte.
const REPORT: &str = concat!(
    "1\tStructural\t-\t\n",
    "2\tText\t-\tTown Square\n",
    "3\tStructural\t-\t\n",
    "4\tWindowHints\t-\t42\n",
    "5\tText\t-\tYou see a fountain here, its water <cold> and clear.\n",
    "6\tStructural\t-\t\n",
    "7\tText\t-\ta blue gem\n",
    "8\tStructural\t-\t\n",
    "9\tText\t-\t glitters on the ground.\n",
    "10\tStructural\t-\t\n",
    "11\tText\t-\tVaalor’s gate creaks.\n",
    "12\tStructural\t-\t\n",
    "13\tUnknownTag\tquestState\t\n",
    "14\tText\t-\tready\n",
    "15\tUnknownTag\tquestState\t\n",
    "16\tPrompt\t-\t>\n",
    "\n== 16 frames from 316 bytes\n",
    "       1  Prompt\n",
    "       6  Structural\n",
    "       6  Text\n",
    "       2  UnknownTag\n",
    "       1  WindowHints\n",
    "== unknown tags\n",
    "       2  <questState>\n",
);

#[test]
fn replay_prints_one_documented_line_per_frame() {
    let path = fixture("small_stream.xml");
    let output = run(&["--replay", path.to_str().unwrap()]);
    assert!(output.status.success(), "status: {:?}", output.status);
    assert_eq!(stdout(&output), REPORT);
    assert_eq!(stderr(&output), "");
}

#[test]
fn the_recovered_spelling_of_replay_works_too() {
    let path = fixture("small_stream.xml");
    let output = run(&["replay", path.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), REPORT);
}

#[test]
fn the_room_tag_is_dropped_and_the_unknown_tag_is_kept_verbatim() {
    let path = fixture("small_stream.xml");
    let output = run(&["--replay", path.to_str().unwrap()]);
    let text = stdout(&output);
    // `<room name=".." id=".."/>` is a known tag the parser does not model:
    // the original frames it as `WindowHints` (named by its `id`), it never
    // reaches the text.
    assert!(!text.contains("room name"), "{text}");
    assert!(text.contains("4\tWindowHints\t-\t42\n"), "{text}");
    // An unknown tag is framed as `UnknownTag` (once per occurrence, open and
    // close) and listed in the footer.
    assert!(
        text.contains("13\tUnknownTag\tquestState\t\n"),
        "{text}"
    );
    assert!(text.contains("       2  <questState>\n"), "{text}");
    // The entity was decoded and the UTF-8 passed through untouched.
    assert!(text.contains("water <cold> and clear"), "{text}");
    assert!(text.contains("Vaalor’s gate creaks."), "{text}");
}

#[test]
fn replay_text_shows_readable_text_only() {
    let path = fixture("small_stream.xml");
    let output = run(&["--replay", path.to_str().unwrap(), "--text"]);
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        concat!(
            // One line per input line, not one per frame: the two text
            // segments around the object link coalesce, and only `Text` and
            // `Prompt` frames print anything (the `WindowHints` frame of
            // `<room/>` adds no line).
            "Town Square\n",
            "You see a fountain here, its water <cold> and clear.\n",
            "a blue gem glitters on the ground.\n",
            "Vaalor’s gate creaks.\n",
            "ready\n",
            // The prompt is left unterminated; the footer's leading newline
            // closes it.
            ">\n== 16 frames from 316 bytes\n",
            "       1  Prompt\n",
            "       6  Structural\n",
            "       6  Text\n",
            "       2  UnknownTag\n",
            "       1  WindowHints\n",
            "== unknown tags\n",
            "       2  <questState>\n",
        )
    );
}

#[test]
fn replay_summary_counts_the_frame_kinds() {
    let path = fixture("small_stream.xml");
    let output = run(&["--replay", path.to_str().unwrap(), "--summary"]);
    assert!(output.status.success());
    // Byte-identical to the reference build's `--summary`.
    assert_eq!(
        stdout(&output),
        concat!(
            "\n== 16 frames from 316 bytes\n",
            "       1  Prompt\n",
            "       6  Structural\n",
            "       6  Text\n",
            "       2  UnknownTag\n",
            "       1  WindowHints\n",
            "== unknown tags\n",
            "       2  <questState>\n",
        )
    );
}

#[test]
fn replaying_an_empty_recording_succeeds_with_no_frames() {
    let path = fixture("empty.xml");
    let output = run(&["--replay", path.to_str().unwrap()]);
    assert!(output.status.success(), "status: {:?}", output.status);
    assert_eq!(
        stdout(&output),
        "\n== 0 frames from 0 bytes\n== no unknown tags\n"
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn a_missing_file_is_a_clear_error_on_stderr() {
    let missing = fixture("does-not-exist.xml");
    let output = run(&["--replay", missing.to_str().unwrap()]);
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert!(err.starts_with(missing.to_str().unwrap()), "{err}");
    assert!(err.contains("No such file"), "{err}");
    assert_eq!(stdout(&output), "", "nothing is reported as frames");
}

#[test]
fn replay_without_a_file_prints_the_usage_to_stderr() {
    for args in [&["--replay"][..], &["replay"][..]] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "");
        assert!(stderr(&output).starts_with("usage:\n"), "{args:?}");
    }
}

#[test]
fn help_prints_the_recovered_usage_text() {
    for flag in ["--help", "-h", "help"] {
        let output = run(&[flag]);
        assert!(output.status.success(), "{flag}: {:?}", output.status);
        let text = stdout(&output);
        assert!(text.starts_with("usage:\n  hydra   "), "{text}");
        assert!(
            text.contains("hydra replay <file.bytes|file.xml> [--summary | --text]"),
            "{text}"
        );
        assert!(text.trim_end().ends_with("Hydra stopped"), "{text}");
        assert_eq!(stderr(&output), "");
    }
}

#[test]
fn version_is_useful() {
    for flag in ["--version", "-V", "version"] {
        let output = run(&[flag]);
        assert!(output.status.success());
        assert_eq!(stdout(&output), "hydra 0.1.0-beta.7\n");
    }
}

#[test]
fn an_unknown_word_prints_the_usage_to_stderr_and_fails() {
    let output = run(&["bogus"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("usage:\n"));
    assert_eq!(stdout(&output), "");
}

#[test]
fn the_unwired_subcommands_say_what_they_still_need() {
    // `gui` is wired since Phase 4: it opens the eframe window, so it is not
    // exercised here — `main`'s unit test
    // `the_gui_subcommand_builds_the_recovered_hub_window` asserts the built
    // plan (`"Hydra"`, 660 × 520, wgpu) without launching it.  `play` and
    // `serve` are wired since Phase 3 (the Lich side); they are covered by
    // `play_and_serve_say_that_lich_is_off` and `tests/lich_relay.rs`.  `login`
    // is wired since Phase 6 (the real game login) and is covered end to end by
    // `tests/login.rs`, so it is no longer in this list.  `import` and
    // `lookout seed` are wired since Phase 7 and are covered end to end by
    // `tests/import_lookout.rs`; the bare hub is the only report left.
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert!(err.contains("not wired up in this headless phase"), "{err}");
}

#[test]
fn play_and_serve_say_that_lich_is_off() {
    // `HYDRA_LICH_SETTINGS` points at a file that is not there, so this reads
    // the defaults (`on: false`) whatever the working directory holds.
    for args in [&["play", "Aelith"][..], &["serve", "Aelith", "--detach"][..]] {
        let output = Command::new(binary())
            .args(args)
            .env("HYDRA_LICH_SETTINGS", "/nonexistent/hydra/lich.json")
            .output()
            .expect("the hydra binary runs");
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_eq!(stdout(&output), "", "{args:?}");
        let err = stderr(&output);
        assert!(
            err.starts_with(&format!("hydra {}: Lich is off for this character", args[0])),
            "{args:?}: {err}"
        );
        assert!(err.contains("no game connection"), "{args:?}: {err}");
    }
}
