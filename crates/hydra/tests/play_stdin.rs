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
//! Since Phase 9 the same loop honours `.lich off` and `.lich on` while the
//! session is running (`crate::lich::relay_game`): the last three tests use
//! `tests/fixtures/lich_restart/ruby`, a second synthetic Lich that counts its
//! own boots in `$HYDRA_DATA_DIR/lich-boots`, so "a *second* Lich was really
//! spawned" is something a test can see.  The game socket is never replaced —
//! the same `MockGame` connection carries every line, and its handshake is
//! recorded once.
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

/// The same, for the Lich that counts its boots (`.lich off` / `.lich on`).
fn restart_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lich_restart")
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
fn a_lich_off_typed_mid_session_stops_lich_and_sends_later_typing_to_the_game() {
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
    // sentence, then the child is stopped and the door closed.  The session is
    // *not* over: the game socket is still open (Phase 9).
    run.say(".lich off");
    run.wait_for_output("Lich is stopping, and stays off for this character.");

    // The first line after it may still have been taken by the child (the
    // request is carried out on the relay's thread), so the *second* one is the
    // proof: by then the door is certainly shut and the line goes to the game
    // directly, with no Lich in the way.
    run.say("after-off");
    run.wait_for_game(&game, "after-off");
    run.say("direct");
    run.wait_for_game(&game, "direct");
    assert!(
        !run.out().contains("--- synthetic Lich forwarded direct"),
        "a stopped Lich received a typed line: {}",
        run.out()
    );

    // The session is still here, and says so.
    run.say(";status");
    run.wait_for_output("[status] Aelith: Lich is not running");

    // Nothing Hydra answered locally reached the game.
    run.settle();
    assert_eq!(
        game.sent(),
        vec!["after-off".to_string(), "direct".to_string()],
        "stdout: {}\nstderr: {}",
        run.out(),
        run.err()
    );

    drop(run);
    eaccess
        .join()
        .expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn a_lich_off_then_a_lich_on_starts_a_second_lich_on_the_same_game_socket() {
    let data = TempDir::with_lich("lich-restart");
    let (game, eaccess, envs) = start_mocks();
    let mut run = Run::start(
        &["play", "Aelith", "--account", "main", "--game", "GS3"],
        &data,
        &restart_path(),
        &envs,
    );

    // The initial Lich: started with the session, boot number one.
    run.wait_for_output("[hydra] connected; type commands, Ctrl-C to quit");
    run.wait_for_output("--- synthetic Lich boot 1.");
    // ... and it takes the typing: a bare line goes to it and on to the game.
    run.say("look");
    run.wait_for_game(&game, "look");
    run.wait_for_output("--- synthetic Lich forwarded look");

    // `.lich off`: the child stops and the door is closed.
    run.say(".lich off");
    run.wait_for_output("Lich is stopping, and stays off for this character.");
    // Carried out on the relay's thread, so the door may still have been open
    // when the next line arrived; once *any* line has reached the game the stop
    // has certainly been taken (the loop drains the door before it acts on the
    // request), and the line after that goes to the game directly.
    run.say("after-off");
    run.wait_for_game(&game, "after-off");
    run.say("direct");
    run.wait_for_game(&game, "direct");

    // `.lich on`: a **second** Lich, spawned while this game socket stays open.
    run.say(".lich on");
    run.wait_for_output("--- synthetic Lich boot 2.");
    // The fresh door claim: a typed line goes to the new child's stdin and on
    // to the game, which the second boot echoes.
    run.say("through-second");
    run.wait_for_game(&game, "through-second");
    run.wait_for_output("--- synthetic Lich forwarded through-second");

    // `.lich on` again, while that second Lich runs: the recovered
    // `Lich already runs for this character.` and **no** third child.
    run.say(".lich on");
    run.wait_for_output("Lich already runs for this character.");

    run.settle();
    assert!(
        !run.out().contains("--- synthetic Lich boot 3."),
        "a second `.lich on` spawned a duplicate: {}",
        run.out()
    );
    // One game connection carried all four lines, in order — the socket was
    // never dropped or replaced, and no second login happened.
    assert_eq!(
        game.sent(),
        vec![
            "look".to_string(),
            "after-off".to_string(),
            "direct".to_string(),
            "through-second".to_string()
        ],
        "stdout: {}\nstderr: {}",
        run.out(),
        run.err()
    );
    assert_eq!(game.received().len(), 2, "{:?}", game.received());

    // The recovered start lines were said twice — once per Lich — and both
    // boots' forwarded chunks arrived without their 0x1F byte.
    let printed = run.out();
    assert_eq!(printed.matches("Lich is at ").count(), 2, "{printed}");
    // Three times: the initial start, `.lich on`'s own recovered notice, and
    // the second start's own line.
    assert!(
        printed.matches("Lich is starting, from ").count() >= 3,
        "{printed}"
    );
    assert!(
        printed.contains("<synthetic-forwarded-boot-1/>"),
        "{printed}"
    );
    assert!(
        printed.contains("<synthetic-forwarded-boot-2/>"),
        "{printed}"
    );
    assert!(
        !printed.contains('\u{1f}'),
        "a chunk byte reached stdout: {printed:?}"
    );
    // `direct` was typed while no Lich ran, so no Lich can have echoed it.
    assert!(!printed.contains("forwarded direct"), "{printed}");

    // The game closing still ends the session cleanly — with the second Lich
    // running — so the teardown reaps it.
    drop(game);
    let status = run.wait_for_exit();
    assert_eq!(status.code(), Some(0), "stderr: {}", run.err());
    run.wait_for_output("[status] the session has ended");

    drop(run);
    eaccess
        .join()
        .expect("the mock eaccess server ends cleanly");
}

#[test]
fn a_lich_on_that_cannot_be_started_says_so_and_leaves_the_session_usable() {
    let data = TempDir::with_lich("lich-refused");
    let folder = data.path.join("lich");
    let (game, eaccess, envs) = start_mocks();
    let mut run = Run::start(
        &["play", "Aelith", "--account", "main", "--game", "GS3"],
        &data,
        &restart_path(),
        &envs,
    );
    run.wait_for_output("--- synthetic Lich boot 1.");

    // The folder the settings name is no longer a Lich: its `lich.rbw` is gone
    // by the time `.lich on` is typed.
    fs::remove_file(folder.join("lich.rbw")).expect("cannot remove lich.rbw");

    run.say(".lich off");
    run.wait_for_output("Lich is stopping, and stays off for this character.");
    run.say("after-off");
    run.wait_for_game(&game, "after-off");

    // `.lich on` is refused with the recovered sentence, and nothing is spawned.
    run.say(".lich on");
    run.wait_for_output(&format!("{} has no lich.rbw in it", folder.display()));
    run.settle();
    assert!(
        !run.out().contains("--- synthetic Lich boot 2."),
        "a Lich was spawned after a refusal: {}",
        run.out()
    );

    // The session is still here, and typing still reaches the game directly.
    run.say("still-here");
    run.wait_for_game(&game, "still-here");
    run.say(";status");
    run.wait_for_output("[status] Aelith: Lich is not running");
    run.settle();
    assert_eq!(
        game.sent(),
        vec!["after-off".to_string(), "still-here".to_string()],
        "stdout: {}\nstderr: {}",
        run.out(),
        run.err()
    );
    assert!(!run.out().contains('\u{1f}'), "{}", run.out());

    drop(run);
    eaccess
        .join()
        .expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}
