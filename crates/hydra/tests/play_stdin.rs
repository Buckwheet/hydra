//! `hydra play` with a keyboard: what a line typed on stdin does.
//!
//! Everything here goes through `env!("CARGO_BIN_EXE_hydra")` with its stdin
//! **piped**, against the two loopback mocks from `hydra_mocks`
//! (`MockEaccess` for the login, `MockGame` for the game socket) and the
//! synthetic Lich under `tests/fixtures/lich_typing/`.  That fixture ruby plays
//! the half of `docs/LICH_WIRE.md` §3 this file needs: it completes the keyed
//! handshake and then forwards every line typed on its stdin to the game socket,
//! which is what a real Lich does when it takes typing.
//!
//! The rules being checked are `crate::typing`'s (unit-tested there as well):
//!
//! * a bare line reaches the game (`look`);
//! * a `.`-prefixed line is Hydra's and is answered locally (`.lich status`);
//! * `;status` is a local status line;
//! * neither of the latter two is ever sent to the game.
//!
//! Nothing real is involved: both servers bind `127.0.0.1`, the certificate is
//! generated in-process, the account, password and every line of output are
//! invented.  Every run is bounded ([`WAIT`]) and killed if it overruns.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hydra_mocks::{EaccessScript, MockEaccess, MockGame};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_hydra")
}

/// Upper bound on one `hydra` run: every wait inside a run is bounded by the
/// run's own deadline, set when the child starts.
const WAIT: Duration = Duration::from_secs(30);

/// The folder that holds the synthetic `ruby` put on the child's `PATH`.
fn fake_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lich_typing")
}

/// A temp folder, removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn with_lich(tag: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is before the epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("hydra-typed-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).expect("cannot create the temp folder");
        let data = TempDir { path };
        let folder = data.path.join("lich");
        fs::create_dir_all(&folder).expect("cannot create the Lich folder");
        fs::write(folder.join("lich.rbw"), "").expect("cannot write lich.rbw");
        // `.lich on`, with a folder that holds a `lich.rbw`: the settings file
        // the run is started from, and the one `.lich` commands answer for.
        let settings = format!(
            "{{\n  \"folder\": {},\n  \"on\": true,\n  \"view\": \"game\"\n}}\n",
            json_string(&folder.display().to_string())
        );
        fs::write(data.path.join("lich.json"), settings).expect("cannot write lich.json");
        data
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

/// A `hydra play` run that is still alive: its stdin is the test's keyboard and
/// its stdout/stderr are read on threads, so the test can watch a live session.
struct Run {
    child: Child,
    stdin: Option<ChildStdin>,
    out: Arc<Mutex<String>>,
    err: Arc<Mutex<String>>,
    /// When this run must be over: [`WAIT`] after the child started.
    deadline: Instant,
}

impl Run {
    fn start(args: &[&str], data: &TempDir, path_dir: &Path, envs: &[(&str, String)]) -> Self {
        let mut command = Command::new(binary());
        command
            .args(args)
            .env("HYDRA_DATA_DIR", &data.path)
            .env("PATH", path_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in envs {
            command.env(name, value);
        }
        let mut child: Child = command.spawn().expect("the hydra binary starts");
        let out = Arc::new(Mutex::new(String::new()));
        let err = Arc::new(Mutex::new(String::new()));
        read_into(child.stdout.take().expect("stdout is piped"), out.clone());
        read_into(child.stderr.take().expect("stderr is piped"), err.clone());
        Run {
            stdin: child.stdin.take(),
            child,
            out,
            err,
            deadline: Instant::now() + WAIT,
        }
    }

    /// Type one line, as a player would.
    fn say(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin is piped");
        stdin
            .write_all(format!("{line}\n").as_bytes())
            .expect("the child reads its stdin");
        stdin.flush().expect("the child's stdin flushes");
    }

    fn out(&self) -> String {
        self.out.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn err(&self) -> String {
        self.err.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Wait until `ready` holds, or fail once this run's [`WAIT`] is over.
    fn wait_until(&self, what: &str, mut ready: impl FnMut() -> bool) {
        while !ready() {
            assert!(
                Instant::now() < self.deadline,
                "hydra did not {what} within {WAIT:?}\nstdout: {}\nstderr: {}",
                self.out(),
                self.err()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Wait until the child has printed `needle`.
    fn wait_for_output(&self, needle: &str) {
        self.wait_until(&format!("print {needle:?}"), || self.out().contains(needle));
    }

    /// Wait until the mock game has received `line` from the client.
    fn wait_for_game(&self, game: &MockGame, line: &str) {
        self.wait_until(&format!("send {line:?} to the game"), || {
            game.sent().iter().any(|sent| sent == line)
        });
    }

    /// Wait for the child to end by itself, or fail once this run's [`WAIT`]
    /// is over.
    fn wait_for_exit(&mut self) -> std::process::ExitStatus {
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return status,
                None => {
                    assert!(
                        Instant::now() < self.deadline,
                        "hydra did not end within {WAIT:?}\nstdout: {}\nstderr: {}",
                        self.out(),
                        self.err()
                    );
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    /// Give the run a moment to do anything it is going to do.
    fn settle(&self) {
        thread::sleep(Duration::from_millis(200));
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        // Every run ends here: closing the keyboard stops nothing by itself, so
        // the child is killed and reaped rather than left to run on.
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Append everything `reader` produces to `sink`, on a thread of its own.
fn read_into<R: Read + Send + 'static>(mut reader: R, sink: Arc<Mutex<String>>) {
    thread::Builder::new()
        .name("hydra-test-reader".to_string())
        .spawn(move || {
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buffer[..n]).into_owned();
                        sink.lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push_str(&text);
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("the reader thread starts");
}

/// The synthetic game stream: two lines of markup, nothing captured.
const STREAM: &str = "<settings/>\r\n<room>synthetic</room>\r\n";

/// Start a lingering `MockGame` and a `MockEaccess` whose launch names it, and
/// return the environment `play` needs (all of it on `127.0.0.1`).
fn start_mocks() -> (MockGame, MockEaccess, Vec<(&'static str, String)>) {
    let game = MockGame::start_lingering(STREAM).expect("the mock game server starts");
    let mut script = EaccessScript::synthetic();
    script.launch.host = "127.0.0.1".to_string();
    script.launch.port = game.addr().port();
    let eaccess = MockEaccess::start(script.clone()).expect("the mock eaccess server starts");
    let envs = vec![
        ("HYDRA_EACCESS_HOST", "127.0.0.1".to_string()),
        ("HYDRA_EACCESS_PORT", eaccess.addr().port().to_string()),
        ("HYDRA_PASSWORD_MAIN", script.password.clone()),
    ];
    (game, eaccess, envs)
}

#[test]
fn a_typed_line_reaches_the_game_and_hydras_own_lines_do_not() {
    let data = TempDir::with_lich("typing");
    let (game, eaccess, envs) = start_mocks();
    let mut run = Run::start(
        &["play", "Aelith", "--account", "main", "--game", "GS3"],
        &data,
        &fake_path(),
        &envs,
    );

    // The session is up: login done, Lich started, game socket open, and the
    // recovered connected notice printed.
    run.wait_for_output("[hydra] connected; type commands, Ctrl-C to quit");
    run.wait_for_output("--- synthetic Lich is taking typing.");

    // A bare line: Lich takes typing, so Hydra hands it to Lich and Lich
    // forwards it to the game (docs/LICH_WIRE.md §3).
    run.say("look");
    run.wait_for_game(&game, "look");

    // `.lich` is Hydra's: answered here, never forwarded.  The answer is the
    // recovered pair of sentences, read from the same `lich.json` this run
    // started from.
    run.say(".lich");
    run.wait_for_output("Lich runs for this character, and is off, and starts with it");
    run.wait_for_output("Lich is at ");

    // `.lich status` is the *same* command for the reference build: its
    // recovered usage table has no `status` word (the status is the bare
    // `.lich`), so the answer is the recovered usage line — still local, still
    // never forwarded.
    run.say(".lich status");
    run.wait_for_output("say lich on, lich off, lich folder <folder>");

    // `;status` is also local, and says what the session is doing.
    run.say(";status");
    run.wait_for_output("[status] Aelith: Lich is running and takes typing");
    run.wait_for_output("typed line(s) wait");

    // `quit` is a plain line: it goes the same way `look` did.
    run.say("quit");
    run.wait_for_game(&game, "quit");

    // Nothing Hydra handled locally reached the game: exactly the two typed
    // lines did, in order.
    run.settle();
    assert_eq!(
        game.sent(),
        vec!["look".to_string(), "quit".to_string()],
        "stdout: {}\nstderr: {}",
        run.out(),
        run.err()
    );
    // The login handshake is still the only thing the game recorded as a
    // handshake, and the login never named a host.
    assert_eq!(game.received().len(), 2, "{:?}", game.received());
    assert!(!eaccess.sni_seen(), "the login sent SNI");
    assert!(eaccess.errors().is_empty(), "{:?}", eaccess.errors());
    assert!(
        !eaccess.requests().iter().any(|line| line.contains("look")),
        "{:?}",
        eaccess.requests()
    );

    // The recovered literals are where they belong: the start lines, the
    // connected notice and Lich's own line.
    let printed = run.out();
    assert!(printed.contains("Lich is starting, from "), "{printed}");
    assert!(
        !printed.contains('\u{1f}'),
        "a chunk byte reached stdout: {printed:?}"
    );
    drop(run);
    eaccess
        .join()
        .expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn an_empty_line_is_ignored_and_does_not_end_the_run() {
    let data = TempDir::with_lich("empty");
    let (game, eaccess, envs) = start_mocks();
    let mut run = Run::start(
        &["play", "Aelith", "--account", "main", "--game", "GS3"],
        &data,
        &fake_path(),
        &envs,
    );
    run.wait_for_output("[hydra] connected; type commands, Ctrl-C to quit");
    run.wait_for_output("--- synthetic Lich is taking typing.");

    // A blank line is not a command and not a game command.
    run.say("");
    run.say("   ");
    run.settle();
    assert!(
        game.sent().is_empty(),
        "a blank line reached the game: {:?}",
        game.sent()
    );

    // The session is still alive: the next real line still gets through.
    run.say("look");
    run.wait_for_game(&game, "look");
    assert_eq!(game.sent(), vec!["look".to_string()]);

    drop(run);
    eaccess
        .join()
        .expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

/// Send `signal` to `child` with `/bin/kill`, the way a terminal's Ctrl-C
/// would; `false` when the helper is not there (the test then says so instead
/// of failing).
#[cfg(unix)]
fn signal(child: &Child, signal: &str) -> bool {
    let kill = Path::new("/bin/kill");
    if !kill.is_file() {
        return false;
    }
    Command::new(kill)
        .arg(format!("-{signal}"))
        .arg(child.id().to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[test]
#[cfg(unix)]
fn ctrl_c_sends_quit_and_ends_the_run() {
    let data = TempDir::with_lich("ctrl-c");
    let (game, eaccess, envs) = start_mocks();
    let mut run = Run::start(
        &["play", "Aelith", "--account", "main", "--game", "GS3"],
        &data,
        &fake_path(),
        &envs,
    );
    run.wait_for_output("[hydra] connected; type commands, Ctrl-C to quit");
    run.wait_for_output("--- synthetic Lich is taking typing.");

    if !signal(&run.child, "INT") {
        eprintln!("no /bin/kill on this host: the Ctrl-C case was not run");
        return;
    }

    // Ctrl-C sends `quit` and then cancels the session, so the run ends by
    // itself: the recovered teardown, not a kill.
    let status = run.wait_for_exit();
    assert_eq!(status.code(), Some(0), "stderr: {}", run.err());
    // `quit` went where every typed line goes, and the session said it ended.
    run.wait_for_game(&game, "quit");
    run.wait_for_output("[status] the session has ended");

    drop(run);
    eaccess
        .join()
        .expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn a_lich_off_typed_mid_session_stops_lich_and_ends_the_run() {
    let data = TempDir::with_lich("lich-off");
    let (game, eaccess, envs) = start_mocks();
    let mut run = Run::start(
        &["play", "Aelith", "--account", "main", "--game", "GS3"],
        &data,
        &fake_path(),
        &envs,
    );
    run.wait_for_output("[hydra] connected; type commands, Ctrl-C to quit");
    run.wait_for_output("--- synthetic Lich is taking typing.");

    // `.lich off` is Hydra's, so it is answered locally with the recovered
    // sentence and then stops the child: with Lich gone the relay loop is done,
    // and the run ends as a clean exit rather than a hanging one.
    run.say(".lich off");
    run.wait_for_output("Lich is stopping, and stays off for this character.");
    let status = run.wait_for_exit();
    assert_eq!(status.code(), Some(0), "stderr: {}", run.err());
    run.wait_for_output("[status] the session has ended");

    // Nothing was typed at the game, and the command did not reach it either.
    assert!(
        game.sent().is_empty(),
        "a Hydra command reached the game: {:?}",
        game.sent()
    );

    drop(run);
    eaccess
        .join()
        .expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}
