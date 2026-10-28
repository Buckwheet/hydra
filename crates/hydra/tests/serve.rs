//! `hydra serve`: the resting session, with `--in <dir>` and `--web <port>`.
//!
//! Everything here runs `env!("CARGO_BIN_EXE_hydra")` against the two loopback
//! mocks from `hydra_mocks` (`MockEaccess`, `MockGame`) and the synthetic Lich
//! under `tests/fixtures/lich_typing/` — the same pieces `tests/play_stdin.rs`
//! uses.  Nothing real is involved and nothing leaves `127.0.0.1`.
//!
//! What is checked:
//!
//! * `serve --in <dir> --web <port>`: a line dropped into `<dir>` reaches the
//!   game (through Lich, exactly as a typed line does), the recovered start
//!   lines are printed, `GET /` on `127.0.0.1:<port>` answers `200` with a page
//!   saying `Hydra`, and a spooled `quit` ends the run deliberately with status
//!   0 (Phase 9: the game *not* closing ends it, because a resting serve no
//!   longer treats a game EOF as the end of the session);
//! * `serve` with a game that drops and comes back: the recovered
//!   `[hydra] the connection dropped; reconnecting` notice, the recovered
//!   backoff and a **second** game handshake, with the same Lich still running
//!   (one boot, one start pair) and the same session typing at the new
//!   connection;
//! * `serve --again` on the same drop: the fresh session (`--again: started a
//!   new session for …`) and still one Lich;
//! * `serve --resume <file>`: the resume file names the character when the
//!   command line names none;
//! * `serve --detach`: the parent exits 0, prints the recovered resting line and
//!   leaves `<data>/hydra.log` behind, holding the child's own stage lines, its
//!   reconnect and (after a spooled `quit`) the session's end.
//!
//! Every wait is bounded ([`WAIT`]) and every child is killed and reaped on the
//! way out.  Nothing here dials a real server: the game is a multi-connection
//! `hydra_mocks::MockGame` (`start_series`) and the login is `MockEaccess`.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hydra_mocks::{EaccessScript, MockEaccess, MockGame};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_hydra")
}

/// Upper bound on one wait.
const WAIT: Duration = Duration::from_secs(30);

/// The folder holding the synthetic `ruby` put on the child's `PATH`.
///
/// `tests/fixtures/lich_typing`: the Lich stand-in that takes the player's
/// typing and stays up until its read times out.
fn fake_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lich_typing")
}

/// The folder holding the synthetic `ruby` that plays a Lich which reads its
/// handshake, says one line and **exits** (`tests/fixtures/lich_path`): the run
/// it belongs to therefore ends by itself.
fn fake_path_short_lived() -> PathBuf {
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
    /// A data folder whose `lich.json` says on and points at a folder with an
    /// (empty) `lich.rbw` in it — the settings `serve` starts from.
    fn with_lich(tag: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is before the epoch")
            .as_nanos();
        let path = std::env::temp_dir()
            .join(format!("hydra-serve-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).expect("cannot create the temp folder");
        let data = TempDir { path };
        let folder = data.path.join("lich");
        fs::create_dir_all(&folder).expect("cannot create the Lich folder");
        fs::write(folder.join("lich.rbw"), "").expect("cannot write lich.rbw");
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

/// The two mocks, on `127.0.0.1`, and the environment that points `serve` at
/// them.
///
/// The password variable is `HYDRA_PASSWORD_MAIN` because the runs below pass
/// `--account main`, which is the account `EaccessScript::synthetic` is scripted
/// for — `HYDRA_PASSWORD_<ACCOUNT>` is named after the *account*, not the
/// character.
fn start_mocks(stream: &str) -> (MockGame, MockEaccess, Vec<(&'static str, String)>) {
    start_mocks_for(vec![(stream.to_string(), true)])
}

/// [`start_mocks`] with a series of game connections: `(stream, linger)` per
/// connection, in the order the run is expected to make them.
fn start_mocks_for(
    streams: Vec<(String, bool)>,
) -> (MockGame, MockEaccess, Vec<(&'static str, String)>) {
    let game = MockGame::start_series(streams).expect("the mock game server starts");
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

/// The synthetic game stream: two lines of markup, nothing captured.
const STREAM: &str = "<settings/>\r\n<room>synthetic</room>\r\n";

/// Everything a run's output readers share.
type Sink = Arc<Mutex<String>>;

fn sink() -> Sink {
    Arc::new(Mutex::new(String::new()))
}

fn text(sink: &Sink) -> String {
    sink.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Append everything `reader` produces to `sink`, on a thread of its own.
fn read_into<R: Read + Send + 'static>(mut reader: R, sink: Sink) {
    thread::Builder::new()
        .name("hydra-serve-test-reader".to_string())
        .spawn(move || {
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => sink
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push_str(&String::from_utf8_lossy(&buffer[..n])),
                    Err(_) => break,
                }
            }
        })
        .expect("the reader thread starts");
}

/// Wait until `ready` holds, or fail once [`WAIT`] is over.
fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    wait_until_with(what, &mut ready, &String::new())
}

/// [`wait_until`] with the run's output quoted when it fails.
fn wait_until_with(what: &str, ready: &mut dyn FnMut() -> bool, detail: &str) {
    let deadline = Instant::now() + WAIT;
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "hydra did not {what} within {WAIT:?}\n{detail}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// A bound loopback port nobody is using, for `--web`.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a free port");
    let port = listener.local_addr().expect("the local address").port();
    drop(listener);
    port
}

/// `GET /` on `127.0.0.1:<port>`, retried until the page answers.
fn get_page(port: u16) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(mut socket) = TcpStream::connect(("127.0.0.1", port)) {
            let _ = socket.set_read_timeout(Some(Duration::from_secs(10)));
            if socket
                .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
                .is_ok()
                && socket.flush().is_ok()
            {
                let mut got = Vec::new();
                if socket.read_to_end(&mut got).is_ok() && !got.is_empty() {
                    return String::from_utf8_lossy(&got).into_owned();
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "the page on 127.0.0.1:{port} never answered"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn serve_reads_its_input_folder_relays_and_serves_the_page() {
    let data = TempDir::with_lich("relay");
    let input = data.path.join("in");
    fs::create_dir_all(&input).expect("cannot create the input folder");
    let (game, eaccess, envs) = start_mocks(STREAM);
    let port = free_port();

    let mut command = serve_command(
        &data,
        &fake_path(),
        &[
            "serve",
            "Aelith",
            "--account",
            "main",
            "--in",
            &input.display().to_string(),
            "--web",
            &port.to_string(),
        ],
    );
    for (name, value) in &envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the hydra binary starts");
    let out = sink();
    let err = sink();
    read_into(child.stdout.take().expect("stdout is piped"), out.clone());
    read_into(child.stderr.take().expect("stderr is piped"), err.clone());

    // The session is up: the recovered start lines, and Lich taking typing.
    let detail = || format!("stdout: {}\nstderr: {}", text(&out), text(&err));
    wait_until_with(
        "start Lich",
        &mut || text(&out).contains("Lich is starting, from "),
        &detail(),
    );
    wait_until_with(
        "relay Lich",
        &mut || text(&out).contains("--- synthetic Lich is taking typing."),
        &detail(),
    );

    // The page answers, and it is Hydra's.
    let page = get_page(port);
    assert!(page.starts_with("HTTP/1.1 200\r\n"), "{page}");
    assert!(
        page.contains("Content-Type: text/html; charset=utf-8\r\n"),
        "{page}"
    );
    assert!(page.contains("Hydra"), "{page}");

    // A spooled line goes where a typed one goes: to Lich, and on to the game.
    fs::write(input.join("one"), "look\n").expect("cannot write the spool file");
    wait_until("send look to the game", || {
        game.sent().iter().any(|line| line == "look")
    });
    assert!(
        !input.join("one").exists(),
        "the spool file was not consumed"
    );

    // `.lich off` stops Lich; the `quit` that follows goes straight to the game
    // (`docs/LICH_WIRE.md` §3's "Lich not taking typing" path).
    fs::write(input.join("two"), ".lich off\n").expect("cannot write the spool file");
    fs::write(input.join("three"), "quit\n").expect("cannot write the spool file");
    wait_until("send quit to the game", || {
        game.sent().iter().any(|line| line == "quit")
    });

    // Both player lines were read, in order, and nothing else reached the game.
    assert_eq!(
        game.sent(),
        vec!["look".to_string(), "quit".to_string()],
        "stdout: {}\nstderr: {}",
        text(&out),
        text(&err)
    );

    // `quit` is a deliberate end (`hydra_session::actor::is_exit_intent`), so
    // the run ends with status 0 — since Phase 9 the *game* closing is not what
    // ends a resting `serve`, and this game is still there.
    let status = wait_for_exit(&mut child, &out, &err);
    assert_eq!(status.code(), Some(0), "stderr: {}", text(&err));

    let printed = text(&out);
    // The recovered lines are where they belong.
    assert!(printed.contains("Lich is at "), "{printed}");
    assert!(
        printed.contains("Lich is stopping, and stays off for this character."),
        "{printed}"
    );
    assert!(printed.contains("\nsession stopped: quit"), "{printed}");
    assert!(printed.contains("[status] the session has ended"), "{printed}");
    // And it did not reconnect: this end was the player's, not a dropped line.
    assert!(!printed.contains("[hydra] the connection dropped"), "{printed}");
    assert_eq!(game.received().len(), 2, "{:?}", game.received());
    assert_eq!(game.connections(), 1, "the run reconnected after a quit");
    assert!(!eaccess.sni_seen(), "the login sent SNI");
    assert!(eaccess.errors().is_empty(), "{:?}", eaccess.errors());
    // The stage lines went to stderr exactly as `hydra login` prints them.
    assert!(
        text(&err).contains("[login] [stage: cert_pin]"),
        "{}",
        text(&err)
    );

    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

/// Wait for the child to end by itself, or fail once [`WAIT`] is over.
fn wait_for_exit(
    child: &mut std::process::Child,
    out: &Sink,
    err: &Sink,
) -> std::process::ExitStatus {
    let deadline = Instant::now() + WAIT;
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => return status,
            None => {
                assert!(
                    Instant::now() < deadline,
                    "hydra did not end within {WAIT:?}\nstdout: {}\nstderr: {}",
                    text(out),
                    text(err)
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

/// The folder holding the synthetic `ruby` that plays a Lich which **counts its
/// boots** (`tests/fixtures/lich_restart`): one line per invocation in
/// `$HYDRA_DATA_DIR/lich-boots`, so a test can tell one Lich from two.
fn fake_path_counting() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("lich_restart")
}

/// How many times the counting fixture Lich was started.
fn lich_boots(data: &Path) -> usize {
    fs::read_to_string(data.join("lich-boots"))
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

/// A `hydra` command with the environment a serve test needs: the data folder,
/// the settings file (found by its own path, because `--in` is a chdir), the
/// loopback login host and the Lich search path.  The port and the synthetic
/// password come from `start_mocks`/`start_mocks_for`.
fn serve_command(data: &TempDir, path_dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(binary());
    command
        .args(args)
        .env("HYDRA_DATA_DIR", &data.path)
        .env("PATH", path_dir)
        // `--in` is a chdir, so `lich.json` must be found by its own path.
        .env("HYDRA_LICH_SETTINGS", data.path.join("lich.json"))
        .env("HYDRA_EACCESS_HOST", "127.0.0.1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[test]
fn detach_rests_in_the_background_and_logs_the_child() {
    let data = TempDir::with_lich("detach");
    let input = data.path.join("in");
    fs::create_dir_all(&input).expect("cannot create the input folder");
    // The game drops after its first stream and stays for the second: the child
    // must reconnect (that is what "resting" means now) and then end when the
    // spool asks it to.
    let (game, eaccess, envs) = start_mocks_for(vec![
        (STREAM.to_string(), false),
        (STREAM.to_string(), true),
    ]);

    let mut command = serve_command(
        &data,
        &fake_path(),
        &[
            "serve",
            "Aelith",
            "--account",
            "main",
            "--detach",
            "--in",
            &input.display().to_string(),
        ],
    );
    for (name, value) in &envs {
        command.env(name, value);
    }
    let child = command.spawn().expect("the hydra binary starts");
    let output = child.wait_with_output().expect("the parent ends");

    // The parent says the recovered line and exits 0 without waiting.
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(0), "stderr: {err}");
    assert!(
        err.contains("resting in the background; its log is"),
        "{err}"
    );
    let log = data.path.join("hydra.log");
    assert!(err.contains(&log.display().to_string()), "{err}");

    // The child really ran: it logged its own stage lines (and its output, both
    // streams, went to that one file).
    assert!(log.is_file(), "no log at {}", log.display());
    let logged = || fs::read_to_string(&log).unwrap_or_default();
    wait_until("log its first stage", || {
        logged().contains("[login] [stage: cert_pin]")
    });

    // Then the game dropped and the child — still detached, nobody watching —
    // said so, waited the recovered backoff and dialled again.
    wait_until("report the dropped connection", || {
        logged().contains("[hydra] the connection dropped; reconnecting")
    });
    wait_until("say when it retries", || logged().contains("reconnecting in "));
    wait_until("dial the game again", || game.connections() >= 2);
    // `startup::Serve::args` gives every detached child `--again` (recovered),
    // so the child starts a new session when the game comes back.
    wait_until("start a new session", || {
        logged().contains("[hydra] serve: --again: started a new session for Aelith")
    });
    assert!(logged().contains("Lich is starting, from "), "{}", logged());

    // The spool ends it deliberately: `.lich off`, then `quit`.
    fs::write(input.join("one"), ".lich off\n").expect("cannot write the spool file");
    fs::write(input.join("two"), "quit\n").expect("cannot write the spool file");
    wait_until("finish the detached session", || {
        logged().contains("[status] the session has ended")
    });
    let end = logged();
    assert!(end.contains("\nsession stopped: quit"), "{end}");
    assert!(end.contains("[status] the session has ended"), "{end}");

    // Nothing is left behind: the child closed its game socket (so the mock's
    // lingering connection ended) and made no third connection.
    assert_eq!(game.connections(), 2, "the child reconnected after a quit");
    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn serve_rests_when_the_game_drops_and_comes_back() {
    let data = TempDir::with_lich("reconnect");
    let input = data.path.join("in");
    fs::create_dir_all(&input).expect("cannot create the input folder");
    // Two connections: the first drops after its stream, the second lingers.
    let (game, eaccess, envs) = start_mocks_for(vec![
        (STREAM.to_string(), false),
        (STREAM.to_string(), true),
    ]);

    let mut command = serve_command(
        &data,
        &fake_path_counting(),
        &[
            "serve",
            "Aelith",
            "--account",
            "main",
            "--in",
            &input.display().to_string(),
        ],
    );
    for (name, value) in &envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the hydra binary starts");
    let out = sink();
    let err = sink();
    read_into(child.stdout.take().expect("stdout is piped"), out.clone());
    read_into(child.stderr.take().expect("stderr is piped"), err.clone());
    let detail = || format!("stdout: {}\nstderr: {}", text(&out), text(&err));

    // The session is up.
    wait_until_with(
        "start Lich",
        &mut || text(&out).contains("Lich is starting, from "),
        &detail(),
    );
    wait_until_with(
        "relay Lich",
        &mut || text(&out).contains("--- synthetic Lich boot 1."),
        &detail(),
    );

    // The game drops: the recovered notice, the recovered lifecycle state and
    // the recovered backoff line, in that order.
    wait_until_with(
        "report the dropped connection",
        &mut || {
            text(&out).contains("\n\n[hydra] the connection dropped; reconnecting\n\n")
        },
        &detail(),
    );
    wait_until_with(
        "say it is reconnecting",
        &mut || text(&out).contains("\nlifecycle Reconnecting"),
        &detail(),
    );
    wait_until_with(
        "say when it retries",
        &mut || text(&out).contains("reconnecting in "),
        &detail(),
    );

    // A second game handshake really happened: two KEY/identification pairs.
    wait_until_with(
        "dial the game again",
        &mut || game.received().len() == 4,
        &detail(),
    );
    let received = game.received();
    assert_eq!(received[0], received[2], "a different launch key was used");
    assert_eq!(
        received[1], received[3],
        "a different front-end line was sent"
    );
    assert_eq!(game.connections(), 2, "{}", detail());

    // The same Lich served both connections: one boot, one start pair.
    assert_eq!(lich_boots(&data.path), 1, "Lich was started {} times", lich_boots(&data.path));
    assert_eq!(
        text(&out).matches("Lich is starting, from ").count(),
        1,
        "{}",
        detail()
    );

    // The same session and the same Lich serve the new connection: a spooled
    // line goes through Lich and reaches the second game socket.
    fs::write(input.join("one"), "look\n").expect("cannot write the spool file");
    wait_until_with(
        "forward look over the new connection",
        &mut || game.sent().iter().any(|line| line == "look"),
        &detail(),
    );
    assert_eq!(lich_boots(&data.path), 1, "a reconnect started a second Lich");

    // A typed `quit` ends the run deliberately, with status 0.
    fs::write(input.join("two"), "quit\n").expect("cannot write the spool file");
    let status = wait_for_exit(&mut child, &out, &err);
    assert_eq!(status.code(), Some(0), "stderr: {}", text(&err));
    let printed = text(&out);
    assert!(printed.contains("\nsession stopped: quit"), "{printed}");
    assert!(printed.contains("[status] the session has ended"), "{printed}");
    assert_eq!(game.sent(), vec!["look".to_string(), "quit".to_string()], "{printed}");
    assert_eq!(game.connections(), 2, "the run reconnected after a quit");

    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn serve_again_starts_a_new_session_when_the_game_comes_back() {
    let data = TempDir::with_lich("again");
    let input = data.path.join("in");
    fs::create_dir_all(&input).expect("cannot create the input folder");
    let (game, eaccess, envs) = start_mocks_for(vec![
        (STREAM.to_string(), false),
        (STREAM.to_string(), true),
    ]);

    let mut command = serve_command(
        &data,
        &fake_path_counting(),
        &[
            "serve",
            "Aelith",
            "--account",
            "main",
            "--again",
            "--in",
            &input.display().to_string(),
        ],
    );
    for (name, value) in &envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the hydra binary starts");
    let out = sink();
    let err = sink();
    read_into(child.stdout.take().expect("stdout is piped"), out.clone());
    read_into(child.stderr.take().expect("stderr is piped"), err.clone());
    let detail = || format!("stdout: {}\nstderr: {}", text(&out), text(&err));

    // The drop and the second connection happen as they do without `--again`…
    wait_until_with(
        "dial the game again",
        &mut || game.received().len() == 4,
        &detail(),
    );
    // …and this time the session is started over (Phase 9's reading of
    // `--again`: "start over rather than continue").
    wait_until_with(
        "start a new session",
        &mut || {
            text(&out).contains("[hydra] serve: --again: started a new session for Aelith")
        },
        &detail(),
    );
    // The Lich child is the run's, not the session's: it is still the one boot.
    assert_eq!(lich_boots(&data.path), 1, "`--again` restarted Lich");
    assert_eq!(
        text(&out).matches("Lich is starting, from ").count(),
        1,
        "{}",
        detail()
    );

    // It ends deliberately, like any other resting run.
    fs::write(input.join("one"), "quit\n").expect("cannot write the spool file");
    let status = wait_for_exit(&mut child, &out, &err);
    assert_eq!(status.code(), Some(0), "stderr: {}", text(&err));
    assert!(
        text(&out).contains("[status] the session has ended"),
        "{}",
        detail()
    );

    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn resume_names_the_character_when_the_command_line_names_none() {
    let data = TempDir::with_lich("resume");
    // The resume file (`hydra_table::table::Resume::read`) names the character;
    // the command line names none.
    let resume = data.path.join("resume");
    fs::write(&resume, "Aelith\n").expect("cannot write the resume file");
    let (game, eaccess, envs) = start_mocks_for(vec![(STREAM.to_string(), false)]);

    // The short-lived fixture Lich ends the run by itself, so this needs no
    // spool.
    let mut command = serve_command(
        &data,
        &fake_path_short_lived(),
        &["serve", "--resume", &resume.display().to_string(), "--account", "main"],
    );
    for (name, value) in &envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the hydra binary starts");
    let out = sink();
    let err = sink();
    read_into(child.stdout.take().expect("stdout is piped"), out.clone());
    read_into(child.stderr.take().expect("stderr is piped"), err.clone());

    let status = wait_for_exit(&mut child, &out, &err);
    assert_eq!(status.code(), Some(0), "stderr: {}", text(&err));
    // The login matched the character the resume file named: the `L` line
    // carries that character's code, and no character at all would have been
    // refused by the mock's character list.
    assert!(
        eaccess
            .requests()
            .iter()
            .any(|line| line == "L\tW_TEST_001\tSTORM"),
        "{:?}",
        eaccess.requests()
    );
    assert_eq!(game.received().len(), 2, "{:?}", game.received());
    assert!(
        !text(&err).contains("names no character"),
        "{}",
        text(&err)
    );
    assert!(
        text(&out).contains("Lich is starting, from "),
        "{}",
        text(&out)
    );

    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

/// Send `signal` to `child`, or answer `false` when this host has no `/bin/kill`.
fn signal(child: &std::process::Child, signal: &str) -> bool {
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
fn ctrl_c_ends_the_resting_serve_and_says_why() {
    let data = TempDir::with_lich("ctrl-c");
    let input = data.path.join("in");
    fs::create_dir_all(&input).expect("cannot create the input folder");
    // The game lingers: nothing but Ctrl-C ends this run.
    let (game, eaccess, envs) = start_mocks_for(vec![(STREAM.to_string(), true)]);
    let mut command = serve_command(
        &data,
        &fake_path_counting(),
        &[
            "serve",
            "Aelith",
            "--account",
            "main",
            "--in",
            &input.display().to_string(),
        ],
    );
    for (name, value) in &envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the hydra binary starts");
    let out = sink();
    let err = sink();
    read_into(child.stdout.take().expect("stdout is piped"), out.clone());
    read_into(child.stderr.take().expect("stderr is piped"), err.clone());
    let detail = || format!("stdout: {}\nstderr: {}", text(&out), text(&err));
    wait_until_with(
        "start the session",
        &mut || text(&out).contains("--- synthetic Lich boot 1."),
        &detail(),
    );

    if !signal(&child, "INT") {
        eprintln!("no /bin/kill on this host: the Ctrl-C case was not run");
        let _ = child.kill();
        let _ = child.wait();
        return;
    }

    // Ctrl-C sends `quit` and cancels the session: the run ends by itself, with
    // the recovered `finish` lines and status 0.
    let status = wait_for_exit(&mut child, &out, &err);
    assert_eq!(status.code(), Some(0), "stderr: {}", text(&err));
    let printed = text(&out);
    assert!(printed.contains("\nsession stopped: Ctrl-C"), "{printed}");
    assert!(printed.contains("[status] the session has ended"), "{printed}");
    assert!(!printed.contains("the connection dropped"), "{printed}");
    // `quit` went where every typed line goes (Lich takes typing), and no
    // reconnect was attempted afterwards.
    wait_until("send quit to the game", || {
        game.sent().iter().any(|line| line == "quit")
    });
    assert_eq!(game.connections(), 1, "the run reconnected after Ctrl-C");

    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}
