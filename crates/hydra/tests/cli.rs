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

/// The exact report for `small_stream.xml` (316 bytes, 6 frames).
const REPORT: &str = concat!(
    "1\ttext\t-\tTown Square\n",
    "2\ttext\t-\tYou see a fountain here, its water <cold> and clear.\n",
    "3\ttext\t-\ta blue gem glitters on the ground.\n",
    "4\ttext\t-\tVaalor’s gate creaks.\n",
    "5\ttext\t-\t<questState id=\"7\">ready</questState>\n",
    "6\tprompt\t-\t>\n",
    "\n== 6 frames from 316 bytes\n",
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
    // `<room name=".." id=".."/>` is a *known* structural tag: the recovered
    // parser consumes it and emits nothing for it, so it is not a frame.
    assert!(!text.contains("room name"), "{text}");
    // An unknown tag is preserved as literal text inside the frame it sits on
    // (`hydra_protocol::Frame` has no `Unknown` emission path for this line).
    assert!(
        text.contains("5\ttext\t-\t<questState id=\"7\">ready</questState>\n"),
        "{text}"
    );
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
            "Town Square\n",
            "You see a fountain here, its water <cold> and clear.\n",
            "a blue gem glitters on the ground.\n",
            "Vaalor’s gate creaks.\n",
            "<questState id=\"7\">ready</questState>\n",
            ">\n",
            "\n== 6 frames from 316 bytes\n",
        )
    );
}

#[test]
fn replay_summary_counts_the_frame_kinds() {
    let path = fixture("small_stream.xml");
    let output = run(&["--replay", path.to_str().unwrap(), "--summary"]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "text\t5\nprompt\t1\n\n== 6 frames from 316 bytes\n");
}

#[test]
fn replaying_an_empty_recording_succeeds_with_no_frames() {
    let path = fixture("empty.xml");
    let output = run(&["--replay", path.to_str().unwrap()]);
    assert!(output.status.success(), "status: {:?}", output.status);
    assert_eq!(stdout(&output), "\n== 0 frames from 0 bytes\n");
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
    for args in [
        &[][..],
        &["gui"][..],
        &["play", "Aelith"][..],
        &["login", "Aelith"][..],
        &["serve", "--detach"][..],
        &["import", "x.bytes"][..],
        &["lookout", "seed"][..],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        let err = stderr(&output);
        assert!(err.contains("not wired up in this headless phase"), "{args:?}: {err}");
    }
}
