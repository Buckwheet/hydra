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
//!   game (through Lich, exactly as a typed line does), a second file's `quit`
//!   ends the run with status 0, the recovered start lines are printed, and
//!   `GET /` on `127.0.0.1:<port>` answers `200` with a page saying `Hydra`;
//! * `serve --detach`: the parent exits 0, prints the recovered resting line and
//!   leaves `<data>/hydra.log` behind, holding the child's own stage lines (so
//!   the child really ran) — and the child ends with the run.
//!
//! Every wait is bounded ([`WAIT`]) and every child is killed and reaped on the
//! way out.

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
    let game = MockGame::start_lingering(stream).expect("the mock game server starts");
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

    let mut command = Command::new(binary());
    command
        .args([
            "serve",
            "Aelith",
            "--account",
            "main",
            "--in",
            &input.display().to_string(),
            "--web",
            &port.to_string(),
        ])
        .env("HYDRA_DATA_DIR", &data.path)
        .env("PATH", fake_path())
        // `--in` is a chdir, so `lich.json` must be found by its own path.
        .env("HYDRA_LICH_SETTINGS", data.path.join("lich.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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

    // `quit` is a plain line too, and it goes the same way `look` did.
    fs::write(input.join("two"), "quit\n").expect("cannot write the spool file");
    wait_until("send quit to the game", || {
        game.sent().iter().any(|line| line == "quit")
    });

    // Both spooled lines were read, in order, and nothing else reached the
    // game.
    assert_eq!(
        game.sent(),
        vec!["look".to_string(), "quit".to_string()],
        "stdout: {}\nstderr: {}",
        text(&out),
        text(&err)
    );
    assert_eq!(game.received().len(), 2, "{:?}", game.received());

    // The game closes (as a real one does when the session ends), and the run
    // ends by itself with status 0.
    drop(game);
    let status = wait_for_exit(&mut child, &out, &err);
    assert_eq!(status.code(), Some(0), "stderr: {}", text(&err));

    // The recovered lines are where they belong.
    let printed = text(&out);
    assert!(printed.contains("Lich is at "), "{printed}");
    assert!(printed.contains("[status] the session has ended"), "{printed}");
    assert!(!eaccess.sni_seen(), "the login sent SNI");
    assert!(eaccess.errors().is_empty(), "{:?}", eaccess.errors());
    // The stage lines went to stderr exactly as `hydra login` prints them.
    assert!(
        text(&err).contains("[login] [stage: cert_pin]"),
        "{}",
        text(&err)
    );

    eaccess.join().expect("the mock eaccess server ends cleanly");
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

#[test]
fn detach_rests_in_the_background_and_logs_the_child() {
    let data = TempDir::with_lich("detach");
    // A game that answers its stream and closes: the child's relay then ends by
    // itself, so no detached process is left behind.
    let game = MockGame::start(STREAM).expect("the mock game server starts");
    let mut script = EaccessScript::synthetic();
    script.launch.host = "127.0.0.1".to_string();
    script.launch.port = game.addr().port();
    let eaccess = MockEaccess::start(script.clone()).expect("the mock eaccess server starts");

    let mut command = Command::new(binary());
    command
        .args(["serve", "Aelith", "--account", "main", "--detach"])
        .env("HYDRA_DATA_DIR", &data.path)
        .env("PATH", fake_path())
        .env("HYDRA_LICH_SETTINGS", data.path.join("lich.json"))
        .env("HYDRA_EACCESS_HOST", "127.0.0.1")
        .env("HYDRA_EACCESS_PORT", eaccess.addr().port().to_string())
        .env("HYDRA_PASSWORD_MAIN", script.password.clone())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    wait_until("log its first stage", || {
        fs::read_to_string(&log)
            .map(|text| text.contains("[login] [stage: cert_pin]"))
            .unwrap_or(false)
    });

    // The child ends with the session (the game closed), so nothing is left.
    wait_until("finish the detached session", || {
        fs::read_to_string(&log)
            .map(|text| text.contains("[status] the session has ended"))
            .unwrap_or(false)
    });
    let logged = fs::read_to_string(&log).unwrap_or_default();
    assert!(logged.contains("Lich is starting, from "), "{logged}");

    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}
