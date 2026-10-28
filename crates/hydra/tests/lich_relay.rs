//! `hydra play` and `hydra serve` with Lich on: the loopback integration test.
//!
//! Everything here goes through `env!("CARGO_BIN_EXE_hydra")`, so the paths a
//! user takes are the paths covered: `$HYDRA_DATA_DIR/lich.json` decides, the
//! recovered start lines are printed, Lich is spawned as the reference build
//! spawns it, the keyed handshake really runs over `127.0.0.1`, and Lich's
//! stdout is relayed with the 0x1F chunk byte stripped.
//!
//! Nothing real is involved: `tests/fixtures/lich_path/ruby` is a synthetic
//! shell script that plays Lich's half of the handshake (`docs/LICH_WIRE.md`
//! §1-2) with bash's `/dev/tcp` built-in, `lich.rbw` is an empty placeholder,
//! and every name, id and line of output is invented.  Since Phase 6 `hydra
//! play` also logs in for real, so its runs point `HYDRA_EACCESS_HOST`/`PORT` at
//! `hydra_mocks::MockEaccess` and the launch at `hydra_mocks::MockGame` — both
//! on `127.0.0.1`, with a certificate generated at test time and a synthetic
//! password in `HYDRA_PASSWORD_MAIN`.  Since Phase 7 `hydra serve` runs the
//! same session, so its test points at the same mocks (`tests/serve.rs` covers
//! the `--in` spool, the `--web` page and — since Phase 9 — the resting
//! reconnects on top of that).  Since Phase 9 a *game* EOF no longer ends
//! `serve`, so `serve_starts_lich_too` keeps its game up and lets the
//! short-lived fixture Lich end the run instead.
//!
//! Every wait is bounded: [`WAIT`] caps a whole run, and a run that overruns
//! fails the test instead of parking it.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hydra_mocks::{EaccessScript, MockEaccess, MockGame};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_hydra")
}

/// Upper bound on one `hydra` run.
const WAIT: Duration = Duration::from_secs(30);

/// The folder that holds the synthetic `ruby` put on the child's `PATH`.
fn fake_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lich_path")
}

/// A temp folder, removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is before the epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hydra-play-{tag}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("cannot create the temp folder");
        TempDir { path }
    }

    /// A data directory holding a `lich.json` that points at a Lich folder with
    /// a synthetic `lich.rbw` in it.
    fn with_lich(tag: &str) -> (Self, PathBuf) {
        let data = TempDir::new(tag);
        let folder = data.path.join("lich");
        fs::create_dir_all(&folder).expect("cannot create the Lich folder");
        fs::write(folder.join("lich.rbw"), "").expect("cannot write lich.rbw");
        data.write_settings(&folder, true);
        (data, folder)
    }

    fn write_settings(&self, folder: &Path, on: bool) {
        let settings = format!(
            "{{\n  \"folder\": {},\n  \"on\": {},\n  \"view\": \"game\"\n}}\n",
            json_string(&folder.display().to_string()),
            on
        );
        fs::write(self.path.join("lich.json"), settings).expect("cannot write lich.json");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Quote a string as a JSON member value (paths only: no control characters).
fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What a finished run produced.
struct Finished {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run the binary with `HYDRA_DATA_DIR` and a `PATH` that holds only
/// `path_dir`, waiting at most [`WAIT`].
fn run(args: &[&str], data: &TempDir, path_dir: &Path) -> Finished {
    run_with(args, data, path_dir, &[])
}

/// [`run`] with extra environment values (the Phase 6 login seam).
fn run_with(args: &[&str], data: &TempDir, path_dir: &Path, envs: &[(&str, String)]) -> Finished {
    let mut command = Command::new(binary());
    command
        .args(args)
        .env("HYDRA_DATA_DIR", &data.path)
        .env("PATH", path_dir)
        .stdin(Stdio::null());
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child: Child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the hydra binary starts");

    let deadline = Instant::now() + WAIT;
    let status = loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => break status,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("hydra {args:?} did not finish within {WAIT:?}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    child
        .stdout
        .take()
        .expect("stdout is piped")
        .read_to_string(&mut stdout)
        .expect("stdout is UTF-8");
    child
        .stderr
        .take()
        .expect("stderr is piped")
        .read_to_string(&mut stderr)
        .expect("stderr is UTF-8");
    Finished {
        code: status.code(),
        stdout,
        stderr,
    }
}

/// The two recovered start lines, for a folder.
fn start_lines(folder: &Path) -> (String, String) {
    (
        format!("Lich is at {}, for every character.", folder.display()),
        format!("Lich is starting, from {}\\lich.rbw.", folder.display()),
    )
}

/// The synthetic game stream `MockGame` writes for the `play` tests.
const STREAM: &str = "<settings/>\r\n<room>synthetic</room>\r\n";

/// The environment that points `hydra play` at the two mocks: the loopback
/// eaccess endpoint and the synthetic account password.  The game socket needs
/// no variable — the launch the mock answers with already names it.
fn mocks_for_play(eaccess_port: u16, password: &str) -> Vec<(&'static str, String)> {
    vec![
        ("HYDRA_EACCESS_HOST", "127.0.0.1".to_string()),
        ("HYDRA_EACCESS_PORT", eaccess_port.to_string()),
        ("HYDRA_PASSWORD_MAIN", password.to_string()),
    ]
}

/// Start a `MockGame` and a `MockEaccess` whose launch points at it, and return
/// the environment `play` needs.  Both serve on `127.0.0.1`.
fn start_mocks() -> (MockGame, MockEaccess, Vec<(&'static str, String)>) {
    start_mocks_with(MockGame::start(STREAM).expect("the mock game server starts"))
}

/// [`start_mocks`] around a game server the caller chose.
fn start_mocks_with(game: MockGame) -> (MockGame, MockEaccess, Vec<(&'static str, String)>) {
    let mut script = EaccessScript::synthetic();
    script.launch.host = "127.0.0.1".to_string();
    script.launch.port = game.addr().port();
    let eaccess = MockEaccess::start(script.clone()).expect("the mock eaccess server starts");
    let envs = mocks_for_play(eaccess.addr().port(), &script.password);
    (game, eaccess, envs)
}

#[test]
fn play_starts_lich_relays_its_stdout_and_exits_cleanly() {
    let (data, folder) = TempDir::with_lich("play");
    let (game, eaccess, envs) = start_mocks();
    let finished = run_with(
        &["play", "Aelith", "--account", "main", "--game", "GS3"],
        &data,
        &fake_path(),
        &envs,
    );

    assert_eq!(finished.code, Some(0), "stderr: {}", finished.stderr);

    // The login said its stages, in order, on stderr — and nothing else.
    let mut at = 0;
    for stage in [
        "[login] [stage: cert_pin]",
        "[login] [stage: k_response]",
        "[login] [stage: a_response]",
        "[login] [stage: m_response]",
        "[login] [stage: f_response]",
        "[login] [stage: g_response]",
        "[login] [stage: c_response]",
        "[login] [stage: l_response]",
        "[login] [stage: game_connect]",
    ] {
        let found = finished.stderr[at..]
            .find(stage)
            .unwrap_or_else(|| panic!("{stage} missing: {}", finished.stderr));
        at += found + stage.len();
    }

    let (at, starting) = start_lines(&folder);
    assert!(finished.stdout.contains(&at), "{}", finished.stdout);
    assert!(finished.stdout.contains(&starting), "{}", finished.stdout);
    // Lich's own line is shown as it wrote it.
    assert!(
        finished.stdout.contains("--- synthetic Lich is active."),
        "{}",
        finished.stdout
    );
    // ... and the forwarded chunk arrives without its 0x1F byte.
    assert!(
        finished.stdout.contains("<synthetic-forwarded-line/>"),
        "{}",
        finished.stdout
    );
    assert!(
        !finished.stdout.contains('\u{1f}'),
        "the 0x1F chunk byte reached the output: {:?}",
        finished.stdout
    );
    // The game's first line reached Lich, which read it back: the game -> Lich
    // direction of the relay really carried something.
    assert!(
        finished.stdout.contains("--- synthetic Lich read <settings/> from the game."),
        "{}",
        finished.stdout
    );

    // The login itself carried the two handshake lines to the game.
    assert_eq!(game.received().len(), 2, "{:?}", game.received());
    assert!(!eaccess.sni_seen());
    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn serve_starts_lich_too() {
    // `serve` is the whole session since Phase 7 (login + Lich + game socket),
    // so it needs the same loopback mocks `play` does.  Since Phase 9 its run
    // does *not* end with the game (`serve` reconnects instead), so this test
    // keeps the game up (`MockGame::start_lingering`) and ends the run the way
    // a resting serve really ends: the short-lived fixture Lich exits by
    // itself, which stops the session (`Ended::Lich`) exactly as it does for
    // `play`.  Nothing here is left waiting for a reconnect.
    let (data, folder) = TempDir::with_lich("serve");
    let (game, eaccess, envs) = start_mocks_with(
        MockGame::start_lingering(STREAM).expect("the mock game server starts"),
    );
    let finished = run_with(
        &["serve", "Aelith", "--account", "main"],
        &data,
        &fake_path(),
        &envs,
    );

    assert_eq!(finished.code, Some(0), "stderr: {}", finished.stderr);
    let (at, starting) = start_lines(&folder);
    assert!(finished.stdout.contains(&at), "{}", finished.stdout);
    assert!(finished.stdout.contains(&starting), "{}", finished.stdout);
    assert!(
        finished.stdout.contains("--- synthetic Lich is active."),
        "{}",
        finished.stdout
    );
    assert!(!finished.stdout.contains('\u{1f}'), "{}", finished.stdout);
    // The login really ran, on stderr, as `hydra login` prints it.
    assert!(
        finished.stderr.contains("[login] [stage: cert_pin]"),
        "{}",
        finished.stderr
    );

    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn lich_off_starts_nothing() {
    let data = TempDir::new("off");
    // A folder is known, but `on` is false: this must not spawn anything.
    let folder = data.path.join("lich");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("lich.rbw"), "").unwrap();
    data.write_settings(&folder, false);

    let finished = run(&["play", "Aelith"], &data, &fake_path());
    assert_eq!(finished.code, Some(1));
    assert_eq!(finished.stdout, "");
    assert!(
        finished.stderr.starts_with("hydra play: Lich is off for this character"),
        "{}",
        finished.stderr
    );
    assert!(!finished.stderr.contains("Lich is starting"), "{}", finished.stderr);
}

#[test]
fn a_missing_ruby_is_the_recovered_message() {
    let (data, _folder) = TempDir::with_lich("no-ruby");
    // An empty PATH entry: there is no `ruby` to find anywhere.
    let empty = TempDir::new("empty-path");
    let finished = run(&["play", "Aelith"], &data, &empty.path);
    assert_eq!(finished.code, Some(1));
    assert_eq!(finished.stdout, "");
    assert_eq!(
        finished.stderr.trim_end(),
        r"Lich needs Ruby, which Lich's installer puts in C:\Ruby4Lich5; none was found there or on the PATH"
    );
}

#[test]
fn a_folder_without_lich_rbw_is_the_recovered_message() {
    let (data, folder) = TempDir::with_lich("no-script");
    let empty = TempDir::new("no-script-folder");
    fs::remove_file(folder.join("lich.rbw")).unwrap();
    // The data folder's `lich` folder no longer holds the script.
    let finished = run(&["play", "Aelith"], &data, &fake_path());
    assert_eq!(finished.code, Some(1));
    assert_eq!(finished.stdout, "");
    assert_eq!(
        finished.stderr.trim_end(),
        format!("{} has no lich.rbw in it", folder.display())
    );
    let _ = empty;
}

#[test]
fn a_settings_file_without_a_folder_says_where_lich_was_not_said() {
    let data = TempDir::new("no-folder");
    fs::write(
        data.path.join("lich.json"),
        "{\n  \"on\": true,\n  \"view\": \"lich\"\n}\n",
    )
    .unwrap();
    let finished = run(&["serve", "Aelith"], &data, &fake_path());
    assert_eq!(finished.code, Some(1));
    assert_eq!(finished.stdout, "");
    assert_eq!(
        finished.stderr.trim_end(),
        "Where Lich is has not been said: lich folder <folder>."
    );
}
