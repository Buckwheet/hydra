//! Loopback integration tests for the observed Hydra <-> Lich handshake.
//!
//! Ground truth: `docs/LICH_WIRE.md`, reproduced here with the synthetic
//! `fake_lich` stand-in (`tests/bin/fake_lich.rs`) instead of Ruby and the real
//! Lich.  Nothing in this file is game text: names, ids and XML are invented,
//! and every wait is bounded so a broken handshake fails a test instead of
//! parking it.
//!
//! The four scenarios, one test each:
//! * [`good_handshake_streams_both_directions`] — accept, then both stream
//!   directions (`send_game` echo, `<c>` command, bare line);
//! * [`refused_handshakes_fail_fast`] — wrong token and wrong client line;
//! * [`typed_look_reaches_the_socket_as_a_bare_line`] — the stdin typing path;
//! * [`game_chunks_carry_the_0x1f_prefix_and_are_stripped`] — Lich's stdout
//!   chunk marker, through the relay line reader and through `read_lich_out`.

use std::fs;
use std::io::{BufReader, Write};
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hydra_lich::{
    accept_keyed_listener, build_command, keyed_listener, read_lich_out, start_keyed,
    write_lich_handshake, KeyedRelay, Launch, LichFrame, LichOut, Relay,
};

/// A synthetic 64-hex token, the shape `new_token` mints.  Invented, not
/// captured: the real token was masked in the capture.
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Upper bound on every wait: a stalled handshake or a silent pipe fails the
/// test quickly instead of hanging it.
const WAIT: Duration = Duration::from_secs(10);

/// Upper bound on a *refused* handshake.  Well under the relay's recovered
/// 60-second accept deadline, so "no hang" is actually asserted.
const REFUSAL_WAIT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Synthetic fixture helpers
// ---------------------------------------------------------------------------

/// A temp folder for the synthetic `lich.rbw`, removed when the test ends.
///
/// `Launch` makes `parent(lich.rbw)` the child's cwd (the recovered
/// `current_dir`), so the fixture doubles as the working directory.
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
            "hydra-lich-loopback-{tag}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("cannot create the temp folder");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A [`Launch`] whose "ruby" is the `fake_lich` bin target and whose
/// `lich.rbw` is a synthetic placeholder in a fresh temp folder.
///
/// `fake_lich` never reads `lich.rbw`; the file only exists because the
/// recovered `build_command` passes the path and sets the cwd from it.
fn synthetic_launch(dir: &TempDir) -> Launch {
    let lich = dir.join("lich.rbw");
    fs::write(&lich, "# synthetic stand-in for lich.rbw; fake_lich ignores it\n")
        .expect("cannot write the synthetic lich.rbw");
    Launch::new(env!("CARGO_BIN_EXE_fake_lich"), lich)
}

/// Bound the accepted socket's reads, which the accept left at the recovered
/// 60-second timeout.  `SO_RCVTIMEO` is per socket, so setting it through the
/// reader's clone also bounds [`KeyedRelay::read_frame`].
fn bound_socket_reads(keyed: &KeyedRelay, within: Duration) {
    keyed
        .reader
        .get_ref()
        .set_read_timeout(Some(within))
        .expect("cannot bound the socket read timeout");
}

/// Read stdout lines until `done` matches one or the deadline passes; returns
/// every line seen, in order.
fn read_until(relay: &mut Relay, within: Duration, done: impl Fn(&str) -> bool) -> Vec<String> {
    let mut seen = Vec::new();
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        match relay.next_line_timeout(left) {
            Some(line) => {
                let hit = done(&line);
                seen.push(line);
                if hit {
                    break;
                }
            }
            None => break,
        }
    }
    seen
}

/// Close the accepted socket and wait for the fake Lich to exit on its own.
///
/// This is the "exit cleanly when stdin/socket close" half of the helper's
/// contract: the child is reaped here, not killed by `Relay::drop`.
fn shutdown_and_wait(keyed: &mut KeyedRelay) -> std::process::ExitStatus {
    let _ = keyed.socket.shutdown(Shutdown::Both);
    keyed
        .relay
        .run_to_end()
        .expect("waiting for the fake Lich failed")
}

/// Wait for `child` to exit, killing it if it outlives `within` (so a failing
/// assertion still leaves no child behind).
fn wait_for_exit(child: &mut Child, within: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + within;
    loop {
        match child.try_wait().expect("cannot poll the child") {
            Some(status) => return Some(status),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// `kill -0 <pid>`: is the process still there?
#[cfg(unix)]
fn process_alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Assert the fake Lich reported a pid and is gone now (reaped by the failed
/// handshake, not left running).
#[cfg(unix)]
fn assert_reaped(pid_file: &Path) {
    let pid = fs::read_to_string(pid_file).expect("the fake Lich never reported its pid");
    let pid = pid.trim().to_string();
    assert!(!pid.is_empty(), "the fake Lich wrote an empty pid");
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !process_alive(&pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the refused handshake left fake Lich {pid} running");
}

#[cfg(not(unix))]
fn assert_reaped(_pid_file: &Path) {}

// ---------------------------------------------------------------------------
// Scenario 1: the good handshake, and both stream directions
// ---------------------------------------------------------------------------

#[test]
fn good_handshake_streams_both_directions() {
    let dir = TempDir::new("good");
    let launch = synthetic_launch(&dir);

    // `start_keyed` succeeding is itself the handshake assertion: it refuses
    // unless the socket's first line is this token and its second line is the
    // client-identification literal.  The fake echoes the token and client line
    // it read on *stdin* (docs/LICH_WIRE.md §2: the two redundant channels
    // carry the same two lines), so an accept also proves the stdin channel.
    let mut keyed = start_keyed(&launch, TOKEN).expect("the observed handshake must be accepted");
    assert_eq!(keyed.token, TOKEN);
    bound_socket_reads(&keyed, WAIT);

    // Hydra -> Lich: the game stream rides the accepted socket, and Lich echoes
    // what it receives onto its stdout as front-end output.
    let chunk = "<hydra-test chunk=\"1\"/>";
    keyed.send_game(chunk).expect("cannot write the game stream");
    let seen = read_until(&mut keyed.relay, WAIT, |line| line == chunk);
    assert!(
        seen.iter().any(|line| line == chunk),
        "the game line never came back on the relay's stdout: {seen:?}"
    );
    // The stdout the relay sees is already stripped of the 0x1F chunk byte...
    assert!(
        seen.iter().all(|line| !line.contains('\u{1f}')),
        "a raw 0x1F byte reached the relay's line receiver: {seen:?}"
    );
    // ...while Lich's own banner stayed unprefixed and passed through.
    assert!(
        seen.iter().any(|line| line.contains("fake_lich")),
        "the fake Lich's own banner is missing: {seen:?}"
    );

    // Lich -> Hydra: the `<c>` command the fake originates right after the
    // handshake is a `Command`...
    assert_eq!(
        keyed.read_frame().expect("no command frame"),
        LichFrame::Command("exp".into())
    );
    // ...and a line the player typed (forwarded on stdin) comes back bare.
    keyed.relay.send("look").expect("cannot write to Lich's stdin");
    assert_eq!(
        keyed.read_frame().expect("no bare frame"),
        LichFrame::Bare("look".into())
    );

    let status = shutdown_and_wait(&mut keyed);
    assert!(status.success(), "the fake Lich exited with {status:?}");
}

// ---------------------------------------------------------------------------
// Scenario 2: a wrong token, and a wrong client line, are refused
// ---------------------------------------------------------------------------

#[test]
fn refused_handshakes_fail_fast() {
    // (a) wrong token on the socket: `PermissionDenied`, no hang, child reaped.
    {
        let dir = TempDir::new("wrong-token");
        let pid_file = dir.join("child.pid");
        let mut launch = synthetic_launch(&dir);
        launch
            .env
            .push(("FAKE_LICH_TOKEN".into(), "f".repeat(64).into()));
        launch.env.push((
            "FAKE_LICH_PID_FILE".into(),
            pid_file.clone().into_os_string(),
        ));

        let started = Instant::now();
        let err = start_keyed(&launch, TOKEN)
            .err()
            .expect("a wrong token must not be accepted");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        // The token never appears in the error (it must not be logged).
        assert!(!err.to_string().contains(TOKEN), "the token leaked: {err}");
        assert!(
            started.elapsed() < REFUSAL_WAIT,
            "a refused handshake took {:?}",
            started.elapsed()
        );
        assert_reaped(&pid_file);
    }

    // (b) wrong client-identification line: `InvalidData`, same reaping.
    {
        let dir = TempDir::new("wrong-client");
        let pid_file = dir.join("child.pid");
        let mut launch = synthetic_launch(&dir);
        launch.env.push((
            "FAKE_LICH_CLIENT".into(),
            "/FE:SOMETHING /VERSION:0.0.0 /XML".into(),
        ));
        launch.env.push((
            "FAKE_LICH_PID_FILE".into(),
            pid_file.clone().into_os_string(),
        ));

        let started = Instant::now();
        let err = start_keyed(&launch, TOKEN)
            .err()
            .expect("a wrong client line must not be accepted");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            started.elapsed() < REFUSAL_WAIT,
            "a refused handshake took {:?}",
            started.elapsed()
        );
        assert_reaped(&pid_file);
    }
}

// ---------------------------------------------------------------------------
// Scenario 3: typed input reaches the socket as a bare line
// ---------------------------------------------------------------------------

#[test]
fn typed_look_reaches_the_socket_as_a_bare_line() {
    let dir = TempDir::new("typed");
    let launch = synthetic_launch(&dir);
    let mut keyed = start_keyed(&launch, TOKEN).expect("the handshake must be accepted");
    bound_socket_reads(&keyed, WAIT);

    // The fake sends one Lich-originated command right after the handshake;
    // draining it proves the `<c>`/bare distinction is not a parsing accident
    // of this test's own line.
    assert_eq!(
        keyed.read_frame().expect("no command frame"),
        LichFrame::Command("exp".into())
    );

    // The player's typing goes to Lich's stdin and leaves Lich as a bare line
    // (docs/LICH_WIRE.md §3: the "Lich takes typing" path).
    keyed.relay.send("look").expect("cannot write to Lich's stdin");
    assert_eq!(
        keyed.read_frame().expect("no bare frame"),
        LichFrame::Bare("look".into())
    );

    let status = shutdown_and_wait(&mut keyed);
    assert!(status.success(), "the fake Lich exited with {status:?}");
}

// ---------------------------------------------------------------------------
// Scenario 4: Lich's stdout 0x1F chunk marker is recognised and stripped
// ---------------------------------------------------------------------------

#[test]
fn game_chunks_carry_the_0x1f_prefix_and_are_stripped() {
    // Part 1: through the relay.  The fake prefixes every forwarded socket line
    // with 0x1F; the relay's line receiver hands out the text without it.
    let chunk = "<hydra-test chunk=\"2\"/>";
    {
        let dir = TempDir::new("chunk-relay");
        let launch = synthetic_launch(&dir);
        let mut keyed = start_keyed(&launch, TOKEN).expect("the handshake must be accepted");

        keyed.send_game(chunk).expect("cannot write the game stream");
        let seen = read_until(&mut keyed.relay, WAIT, |line| line == chunk);
        assert!(
            seen.iter().any(|line| line == chunk),
            "the 0x1F-prefixed chunk did not arrive stripped: {seen:?}"
        );
        assert!(
            seen.iter().all(|line| !line.contains('\u{1f}')),
            "the relay's line receiver kept a 0x1F byte: {seen:?}"
        );

        let status = shutdown_and_wait(&mut keyed);
        assert!(status.success(), "the fake Lich exited with {status:?}");
    }

    // Part 2: the classification itself.  `read_lich_out` must report the
    // banner as `Own` (unprefixed) and the echoed chunk as `Game` with the text
    // intact — which is only possible if the prefix really was on the wire.
    let dir = TempDir::new("chunk-classify");
    let launch = synthetic_launch(&dir);
    let (listener, port) = keyed_listener().expect("cannot bind the loopback listener");
    let mut command = build_command(&launch, port);
    let mut child = command.spawn().expect("cannot spawn the fake Lich");
    let mut stdin = child.stdin.take().expect("the child has no stdin");
    write_lich_handshake(&mut stdin, TOKEN).expect("cannot write the stdin handshake");
    let mut socket =
        accept_keyed_listener(&listener, TOKEN).expect("the handshake must be accepted");

    socket
        .write_all(format!("{chunk}\n").as_bytes())
        .expect("cannot write the game stream");
    socket.flush().expect("cannot flush the game stream");

    let out = read_lich_out(BufReader::new(child.stdout.take().expect("no child stdout")));
    let mut own: Vec<String> = Vec::new();
    let mut game: Vec<String> = Vec::new();
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline && game.is_empty() {
        match out.recv_timeout(Duration::from_millis(250)) {
            Ok(LichOut::Own(text)) => own.push(text),
            Ok(LichOut::Game(text)) => game.push(text),
            Err(_) => {}
        }
    }
    assert!(
        own.iter().any(|line| line.contains("fake_lich")),
        "the unprefixed banner did not classify as `Own`: {own:?}"
    );
    assert_eq!(
        game,
        vec![chunk.to_string()],
        "the forwarded chunk did not classify as `Game` with the 0x1F byte stripped"
    );

    // Teardown: close both directions, then let the child exit by itself.
    let _ = socket.shutdown(Shutdown::Both);
    drop(stdin);
    let status = wait_for_exit(&mut child, WAIT);
    assert!(
        matches!(status, Some(status) if status.success()),
        "the fake Lich did not exit cleanly: {status:?}"
    );
}
