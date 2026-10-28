//! The eaccess login, against the loopback mocks — real TLS, no network.
//!
//! `hydra_mocks::MockEaccess` is a real `rustls` server whose certificate is
//! generated at test time, and `hydra_mocks::MockGame` is the game socket the
//! launch points at.  Everything binds `127.0.0.1:0`; no test resolves a name,
//! reaches a real host or sees a real credential.  Every account, character,
//! password and key below is invented (`EaccessScript::synthetic`).
//!
//! What is covered:
//!
//! * `the_whole_login_runs_and_the_game_socket_opens` — all eight stages, in
//!   order, with no SNI, the pin recorded, the launch honoured and the game
//!   socket's two handshake lines exactly right;
//! * `a_two_game_script_logs_in_to_the_second_game` — a two-game list answered
//!   as ONE `M` record: the second game is chosen and the session stays in
//!   step (the record-per-game framing desynced here);
//! * `a_changed_certificate_is_refused_before_anything_is_sent` — trust on
//!   first use: a second server with a different certificate is refused at
//!   `cert_pin`, and it never receives a line;
//! * `a_wrong_password_is_refused_without_echoing_the_hash`;
//! * `a_short_key_is_refused_by_the_hash_itself`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hydra_mocks::{EaccessScript, MockEaccess, MockGame};
use hydra_net::eaccess::game;
use hydra_net::eaccess::handshake::{self, LoginConfig, Stage, CLIENT_IDENTIFICATION};
use hydra_net::eaccess::pin;

/// The synthetic game stream the mock game server writes, byte for byte.
const STREAM: &str = "<settings/>\r\n<room>synthetic</room>\r\n";

/// The eight stages a complete login walks, in order.
const STAGES: [&str; 8] = [
    "cert_pin",
    "k_response",
    "a_response",
    "m_response",
    "f_response",
    "g_response",
    "c_response",
    "l_response",
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
            "hydra-p6-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("cannot create the temp folder");
        TempDir { path }
    }

    /// `<temp>/simu.pem`: the pin file a login records and checks.
    fn pin(&self) -> PathBuf {
        self.path.join("simu.pem")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Run one login and collect the stage names it reported.
fn login(
    port: u16,
    account: &str,
    character: &str,
    game_code: Option<&str>,
    password: &str,
    pin_path: &Path,
) -> (Result<handshake::Login, hydra_net::eaccess::wire::Refusal>, Vec<&'static str>) {
    let config = LoginConfig {
        account,
        character,
        game: game_code,
        password: password.as_bytes(),
        pin_path,
    };
    let mut stages = Vec::new();
    let outcome = handshake::login_tls("127.0.0.1", port, &config, &mut |stage: Stage| {
        stages.push(stage.name())
    });
    (outcome, stages)
}

#[test]
fn the_whole_login_runs_and_the_game_socket_opens() {
    let dir = TempDir::new("happy");

    // The game socket first, so the launch can point at a real port.
    let game_server = MockGame::start(STREAM).expect("the mock game server starts");
    let mut script = EaccessScript::synthetic();
    script.launch.host = "127.0.0.1".to_string();
    script.launch.port = game_server.addr().port();
    let eaccess = MockEaccess::start(script.clone()).expect("the mock eaccess server starts");

    let (outcome, stages) = login(
        eaccess.addr().port(),
        &script.account,
        "Aelith",
        Some("GS3"),
        &script.password,
        &dir.pin(),
    );
    let login = outcome.expect("the login succeeds");

    // All eight stages, in the recovered order, exactly once each.
    assert_eq!(stages, STAGES.to_vec());

    // The account, the game and the character came back as asked.
    assert_eq!(login.account(), script.account);
    assert_eq!(login.holder(), script.holder);
    assert_eq!(login.game_code(), "GS3");
    assert_eq!(login.game_name(), "GemStone IV");
    assert_eq!(login.tier(), script.tier);
    assert_eq!(login.character().name, "Aelith");
    assert_eq!(login.character().code, script.characters[0].0);
    assert_eq!(login.characters().len(), 1);
    // The session key is kept but the `G` metadata is only stored.
    assert_eq!(login.session_key(), script.session_key);
    assert!(login.metadata().contains("ROOT=sgc/gs"), "{}", login.metadata());

    // The launch names the mock game server and carries its key.
    assert_eq!(login.launch().host, "127.0.0.1");
    assert_eq!(login.launch().port, game_server.addr().port());
    assert_eq!(login.launch().key, script.launch.key);

    // Trust on first use recorded the certificate the server presented.
    assert!(dir.pin().is_file(), "the pin file was recorded");
    let pinned = pin::pem_decode(&std::fs::read_to_string(dir.pin()).unwrap())
        .expect("the pin is a PEM certificate");
    assert_eq!(pinned, eaccess.certificate_der());

    // A login by address sends no SNI.
    assert!(!eaccess.sni_seen(), "the ClientHello must carry no server name");

    // The server saw the sequence, and the keyed password was masked on the way
    // into the record: no corrupt or wrong `A` line was ever sent.
    assert_eq!(
        eaccess.requests(),
        vec![
            "K".to_string(),
            format!("A\t{}\t<key>", script.account),
            "M".to_string(),
            "F\tGS3".to_string(),
            "G\tGS3".to_string(),
            "C".to_string(),
            format!("L\t{}\tSTORM", script.characters[0].0),
        ]
    );
    assert_eq!(eaccess.errors(), Vec::<String>::new());

    // The game socket: the two handshake lines, then exactly the XML stream.
    let mut socket = game::connect(
        &login.launch().host,
        login.launch().port,
        &login.launch().key,
    )
    .expect("the game socket opens");
    let mut stream = String::new();
    socket.read_to_string(&mut stream).expect("the stream is read");
    assert_eq!(stream, STREAM);
    drop(socket);

    assert_eq!(
        game_server.received(),
        vec![
            script.launch.key.clone(),
            CLIENT_IDENTIFICATION.to_string()
        ]
    );
    game_server.join().expect("the mock game server ends cleanly");
    eaccess.join().expect("the mock eaccess server ends cleanly");
}

#[test]
fn a_two_game_script_logs_in_to_the_second_game() {
    let dir = TempDir::new("two-games");

    // The game socket first, so the launch can point at a real port.
    let game_server = MockGame::start(STREAM).expect("the mock game server starts");
    let mut script = EaccessScript::synthetic();
    script.games.push(("DR".to_string(), "DragonRealms".to_string()));
    script.launch.host = "127.0.0.1".to_string();
    script.launch.port = game_server.addr().port();
    let eaccess = MockEaccess::start(script.clone()).expect("the mock eaccess server starts");

    // Ask for the SECOND game: the client must read both pairs out of the one
    // `M` record and stay in step through the close.
    let (outcome, stages) = login(
        eaccess.addr().port(),
        &script.account,
        "Aelith",
        Some("DR"),
        &script.password,
        &dir.pin(),
    );
    let login = outcome.expect("the login succeeds with two games offered");

    // All eight stages, in the recovered order, exactly once each.
    assert_eq!(stages, STAGES.to_vec());

    // The second game came back, by code and by name.
    assert_eq!(login.game_code(), "DR");
    assert_eq!(login.game_name(), "DragonRealms");
    assert_eq!(login.account(), script.account);
    assert_eq!(login.character().name, "Aelith");
    assert_eq!(login.launch().port, game_server.addr().port());
    assert_eq!(login.launch().key, script.launch.key);

    // `M` was asked for once and answered once; every later stage named the
    // second game, so nothing was read out of step.
    assert_eq!(
        eaccess.requests(),
        vec![
            "K".to_string(),
            format!("A\t{}\t<key>", script.account),
            "M".to_string(),
            "F\tDR".to_string(),
            "G\tDR".to_string(),
            "C".to_string(),
            format!("L\t{}\tSTORM", script.characters[0].0),
        ]
    );
    assert_eq!(eaccess.errors(), Vec::<String>::new());

    // The game socket opens and streams exactly as it does for one game.
    let mut socket = game::connect(
        &login.launch().host,
        login.launch().port,
        &login.launch().key,
    )
    .expect("the game socket opens");
    let mut stream = String::new();
    socket.read_to_string(&mut stream).expect("the stream is read");
    assert_eq!(stream, STREAM);
    drop(socket);
    assert_eq!(
        game_server.received(),
        vec![
            script.launch.key.clone(),
            CLIENT_IDENTIFICATION.to_string()
        ]
    );

    game_server.join().expect("the mock game server ends cleanly");
    eaccess.join().expect("the mock eaccess server ends cleanly");
}

#[test]
fn a_changed_certificate_is_refused_before_anything_is_sent() {
    let dir = TempDir::new("changed");

    // First use: this certificate is recorded as the pin.
    let first = MockEaccess::start(EaccessScript::synthetic()).expect("the first server starts");
    let (outcome, stages) = login(
        first.addr().port(),
        "main",
        "Aelith",
        Some("GS3"),
        &EaccessScript::synthetic().password,
        &dir.pin(),
    );
    let pinned = outcome.expect("the first login succeeds");
    assert_eq!(pinned.game_code(), "GS3");
    assert_eq!(stages, STAGES.to_vec());
    assert_eq!(login_pin(&dir), first.certificate_der());
    first.join().expect("the first server ends cleanly");

    // A second server has its own, freshly generated certificate, and a new
    // port; the pin path is the same.
    let second = MockEaccess::start(EaccessScript::synthetic()).expect("the second server starts");
    assert_ne!(second.certificate_der(), login_pin(&dir));
    let (outcome, stages) = login(
        second.addr().port(),
        "main",
        "Aelith",
        Some("GS3"),
        &EaccessScript::synthetic().password,
        &dir.pin(),
    );
    let refusal = outcome.expect_err("a changed certificate is refused");
    assert_eq!(refusal.stage, "cert_pin");
    // The stage was announced before the pin was consulted ...
    assert_eq!(stages, vec!["cert_pin"]);
    // ... and the pin module's own sentence names the change.
    assert!(
        refusal.detail.contains("certificate CHANGED"),
        "{}",
        refusal.detail
    );

    // The pin is checked before `K`: the second server never received a line.
    assert_eq!(second.requests(), Vec::<String>::new());
    second.join().expect("the second server ends cleanly");
}

#[test]
fn a_wrong_password_is_refused_without_echoing_the_hash() {
    let dir = TempDir::new("wrong-password");
    let script = EaccessScript::synthetic();
    let server = MockEaccess::start(script.clone()).expect("the server starts");

    let wrong = "not-the-script-password";
    let (outcome, stages) = login(
        server.addr().port(),
        &script.account,
        "Aelith",
        Some("GS3"),
        wrong,
        &dir.pin(),
    );
    let refusal = outcome.expect_err("the login is refused");
    assert_eq!(refusal.stage, "a_response");
    // The stages up to and including the refused one were announced.
    assert_eq!(stages, vec!["cert_pin", "k_response", "a_response"]);

    // The refusal says what was refused and nothing else: no hash, no password,
    // no key.
    assert!(refusal.detail.contains("PASSWORD"), "{}", refusal.detail);
    for secret in [wrong, script.password.as_str(), script.key.as_str()] {
        assert!(
            !refusal.detail.contains(secret),
            "the refusal leaked {secret:?}: {}",
            refusal.detail
        );
    }
    // What the server recorded is masked, and it saw only the one `A` line.
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert_eq!(requests[0], "K");
    assert_eq!(requests[1], format!("A\t{}\t<key>", script.account));
    server.join().expect("the server ends cleanly");
}

#[test]
fn a_short_key_is_refused_by_the_hash_itself() {
    let dir = TempDir::new("short-key");
    let mut script = EaccessScript::synthetic();
    // A four-byte key and an eighteen-byte password: the hash refuses to
    // truncate rather than send a wrong password.
    script.key = "abcd".to_string();
    script.password = "synthetic-password".to_string();
    let server = MockEaccess::start(script.clone()).expect("the server starts");

    let (outcome, stages) = login(
        server.addr().port(),
        &script.account,
        "Aelith",
        Some("GS3"),
        &script.password,
        &dir.pin(),
    );
    let refusal = outcome.expect_err("the login is refused");
    assert_eq!(refusal.stage, "hash");
    assert!(
        refusal
            .detail
            .contains("key (4 bytes) shorter than password (18 bytes)"),
        "{}",
        refusal.detail
    );
    // The certificate was pinned and the key fetched; nothing was hashed.
    assert_eq!(stages, vec!["cert_pin", "k_response"]);
    // The server never saw an `A` line at all.
    assert_eq!(server.requests(), vec!["K".to_string()]);
    server.join().expect("the server ends cleanly");
}

/// The DER recorded in the pin file of `dir`.
fn login_pin(dir: &TempDir) -> Vec<u8> {
    pin::pem_decode(&std::fs::read_to_string(dir.pin()).expect("the pin exists"))
        .expect("the pin is a PEM certificate")
}
