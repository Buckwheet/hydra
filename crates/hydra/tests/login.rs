//! `hydra login` end to end: a real login against the loopback mocks.
//!
//! Everything here runs the built binary (`env!("CARGO_BIN_EXE_hydra")`) with
//! `HYDRA_EACCESS_HOST`/`HYDRA_EACCESS_PORT` pointed at
//! `hydra_mocks::MockEaccess` and `HYDRA_PASSWORD_MAIN` set to the script's own
//! synthetic password, so no prompt, no keychain and no real host is touched.
//!
//! Covered:
//!
//! * `login_streams_the_raw_xml_of_the_mock_game` — the whole path: eight stage
//!   lines on stderr in order, the game's XML on stdout byte for byte, the
//!   certificate pinned in `<data>/simu.pem`;
//! * `login_refuses_a_changed_certificate` — trust on first use: a second server
//!   with a fresh certificate is refused at `cert_pin` and never sees a line;
//! * `login_reports_an_unreachable_server_at_the_connect_stage` — a dead
//!   loopback port is a clear refusal, not a hang (this replaces the old
//!   "`login` is not wired" check in `tests/cli.rs`).
//!
//! Every run is bounded by `WAIT`; a run that overruns fails the test.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hydra_mocks::{EaccessScript, MockEaccess, MockGame};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_hydra")
}

/// Upper bound on one `hydra` run.
const WAIT: Duration = Duration::from_secs(30);

/// The synthetic game stream: two CRLF-terminated lines, written verbatim.
const STREAM: &str = "<settings/>\r\n<room>synthetic</room>\r\n";

/// The eight stages, in order, as stderr spells them.
const STAGES: [&str; 8] = [
    "[login] [stage: cert_pin]",
    "[login] [stage: k_response]",
    "[login] [stage: a_response]",
    "[login] [stage: m_response]",
    "[login] [stage: f_response]",
    "[login] [stage: g_response]",
    "[login] [stage: c_response]",
    "[login] [stage: l_response]",
];

/// A temp folder, removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is before the epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hydra-login-{tag}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("cannot create the temp folder");
        TempDir { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// What a finished run produced.
struct Finished {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run `hydra login …` against the mock at `port`, with `data` as the data
/// folder, waiting at most [`WAIT`].
fn login(port: u16, data: &TempDir, password: &str, extra: &[&str]) -> Finished {
    let mut args: Vec<String> = vec!["login".to_string(), "Aelith".to_string()];
    args.extend(
        ["--account", "main", "--game", "GS3"]
            .iter()
            .map(|arg| arg.to_string()),
    );
    args.extend(extra.iter().map(|arg| arg.to_string()));

    let mut child: Child = Command::new(binary())
        .args(&args)
        .env("HYDRA_DATA_DIR", &data.path)
        .env("HYDRA_EACCESS_HOST", "127.0.0.1")
        .env("HYDRA_EACCESS_PORT", port.to_string())
        .env("HYDRA_PASSWORD_MAIN", password)
        // No Lich settings file is wanted here, and no keychain prompt either.
        .env("HYDRA_LICH_SETTINGS", data.path.join("no-lich.json"))
        .stdin(Stdio::null())
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

/// The DER pinned in `data/simu.pem`.
fn pinned(data: &TempDir) -> Vec<u8> {
    let text = fs::read_to_string(data.path.join("simu.pem")).expect("the pin file exists");
    hydra_net::eaccess::pin::pem_decode(&text).expect("the pin is a PEM certificate")
}

#[test]
fn login_streams_the_raw_xml_of_the_mock_game() {
    let data = TempDir::new("stream");
    let game = MockGame::start(STREAM).expect("the mock game server starts");
    let mut script = EaccessScript::synthetic();
    script.launch.host = "127.0.0.1".to_string();
    script.launch.port = game.addr().port();
    let eaccess = MockEaccess::start(script.clone()).expect("the mock eaccess server starts");

    let finished = login(eaccess.addr().port(), &data, &script.password, &[]);

    // The game's XML, byte for byte, on stdout.
    assert_eq!(finished.code, Some(0), "stderr: {}", finished.stderr);
    assert_eq!(finished.stdout, STREAM);

    // The eight stage lines, in order, each exactly once.
    let mut at = 0;
    for stage in STAGES {
        let found = finished.stderr[at..].find(stage).unwrap_or_else(|| {
            panic!("{stage} missing after byte {at}: {}", finished.stderr)
        });
        at += found + stage.len();
        assert_eq!(
            finished.stderr.matches(stage).count(),
            1,
            "{stage} appears more than once: {}",
            finished.stderr
        );
    }
    // Nothing else is said about the login.
    assert_eq!(
        finished
            .stderr
            .lines()
            .filter(|line| line.starts_with("[login]"))
            .count(),
        9,
        "eight stages and the game connect: {}",
        finished.stderr
    );
    assert!(
        finished.stderr.contains("[login] [stage: game_connect]"),
        "{}",
        finished.stderr
    );

    // No key material, no password, no holder name.
    for secret in [
        script.key.as_str(),
        script.password.as_str(),
        script.session_key.as_str(),
        script.launch.key.as_str(),
        script.holder.as_str(),
    ] {
        assert!(
            !finished.stderr.contains(secret),
            "the login printed {secret:?}: {}",
            finished.stderr
        );
        assert!(!finished.stdout.contains(secret), "the stream carried {secret:?}");
    }

    // The certificate was pinned for next time.
    assert_eq!(pinned(&data), eaccess.certificate_der());
    // The login sent no SNI, and the game saw the two handshake lines.
    assert!(!eaccess.sni_seen());
    assert_eq!(
        game.received(),
        vec![
            script.launch.key.clone(),
            hydra_mocks::CLIENT_IDENTIFICATION.to_string()
        ]
    );
    eaccess.join().expect("the mock eaccess server ends cleanly");
    game.join().expect("the mock game server ends cleanly");
}

#[test]
fn login_refuses_a_changed_certificate() {
    let data = TempDir::new("changed");

    // First run pins the first server's certificate.
    let game = MockGame::start(STREAM).expect("the mock game server starts");
    let mut script = EaccessScript::synthetic();
    script.launch.host = "127.0.0.1".to_string();
    script.launch.port = game.addr().port();
    let first = MockEaccess::start(script.clone()).expect("the first server starts");
    let finished = login(first.addr().port(), &data, &script.password, &[]);
    assert_eq!(finished.code, Some(0), "stderr: {}", finished.stderr);
    let certificate = pinned(&data);
    assert_eq!(certificate, first.certificate_der());
    first.join().expect("the first server ends cleanly");
    game.join().expect("the first game server ends cleanly");

    // A second server presents a different certificate on a different port; the
    // same data folder (and so the same pin) is used.
    let second = MockEaccess::start(script).expect("the second server starts");
    assert_ne!(second.certificate_der(), certificate);
    let finished = login(second.addr().port(), &data, "synthetic-password", &[]);

    assert_eq!(finished.code, Some(1), "stdout: {}", finished.stdout);
    assert_eq!(finished.stdout, "", "nothing is streamed");
    assert!(
        finished.stderr.contains("[login] [stage: cert_pin]"),
        "{}",
        finished.stderr
    );
    assert!(
        finished.stderr.contains("certificate CHANGED"),
        "{}",
        finished.stderr
    );
    // The pin is checked before `K`: the second server received no line at all.
    assert_eq!(second.requests(), Vec::<String>::new());
    second.join().expect("the second server ends cleanly");
}

#[test]
fn login_reports_an_unreachable_server_at_the_connect_stage() {
    let data = TempDir::new("dead-port");
    // A loopback port nothing listens on: bind, read the port, drop the listener.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    drop(listener);

    let finished = login(port, &data, "synthetic-password", &[]);
    assert_eq!(finished.code, Some(1));
    assert_eq!(finished.stdout, "");
    assert!(
        finished.stderr.contains("[stage: connect]"),
        "{}",
        finished.stderr
    );
    assert!(
        finished.stderr.contains(&format!("127.0.0.1:{port}")),
        "{}",
        finished.stderr
    );
    assert!(
        finished.stderr.starts_with("hydra login: "),
        "{}",
        finished.stderr
    );
    // Nothing was pinned: no connection ever reached a server.
    assert!(!data.path.join("simu.pem").exists());
}
