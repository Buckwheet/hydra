//! `hydra gui <character>`: the hub window with a **live** session in it.
//!
//! # The chain
//!
//! ```text
//! hydra gui Aelith [--account A] [--game G]
//!   │
//!   ├─ login::parse / resolve / password      (the same three steps as `login`/`play`)
//!   ├─ login::sign_in                         the eight eaccess stages, on stderr
//!   ├─ login::open_game                       the two-line game handshake
//!   │     └─ [Live] the socket + the live model
//!   ├─ Live::start                            two threads:
//!   │     ├─ reader: socket → CRLF strip → Parser → LiveSession::feed_line
//!   │     └─ writer: mpsc::Receiver<String> → socket, one `\n`-terminated line
//!   ├─ hydra_gui::app::build_live             NativeOptions + Hub around the model
//!   └─ Launch::run                            eframe: the only window in the crate
//! ```
//!
//! **Lich is not in this chain.**  `play` and `serve` start it; the window talks
//! to the game socket itself, which is what `login` does too.  The reason is the
//! same one Phase 6 gives for `login`: the recovery has a real game login, and
//! the GUI's parser is fed the game's own stream.  Lich's keyed accept
//! (`docs/LICH_WIRE.md`) is a front end *of* the game, so putting it between the
//! window and the game would mean the window draws Lich's relabelled stream
//! through a second relay while `play`/`serve` already own that path.
//!
//! # What is split, and why
//!
//! [`gui_connect_from`] is everything up to (and including) the open game
//! socket, and it touches no event loop: a test can run the whole
//! login → game → parser → model chain against the loopback mocks and never open
//! a window.  [`run`] is the only part that launches eframe, and nothing in this
//! crate's tests calls it.
//!
//! # Failures
//!
//! Every failure is `hydra gui: <the login's own sentence>` on stderr with
//! status 1 (the shape `hydra::main` reports every other failure in).  A login
//! or game-socket refusal never opens the window, so the player is never shown
//! an empty one.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use hydra_gui::app::{self, Launch, Start};
use hydra_gui::live::LiveSession;

use crate::login;

/// The command this module implements, as its diagnostics spell it.
pub const LABEL: &str = "hydra gui";

/// The login and the game socket, before any thread is started.
///
/// [`gui_connect_from`] returns this: the live model (still empty) and the open
/// game socket, which is what makes the chain testable without a window.
pub(crate) struct Live {
    /// The model the reader thread feeds and the window paints.
    pub live: Arc<Mutex<LiveSession>>,
    /// The game socket, positioned at the game's XML.
    pub socket: TcpStream,
}

/// A live session with its two threads running.
pub(crate) struct Running {
    /// The model, shared with the reader thread and the window.
    pub live: Arc<Mutex<LiveSession>>,
    /// Where the window's input line sends a typed command.
    pub input: Sender<String>,
    /// A handle on the game socket, so the window closing can shut it down.
    pub socket: TcpStream,
}

/// Lock the model, surviving a poisoned lock (a panicking thread is not a
/// reason to take the window down with it).
fn locked(live: &Arc<Mutex<LiveSession>>) -> MutexGuard<'_, LiveSession> {
    live.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Log in for `target` and open the game socket: the whole chain minus the
/// window.
///
/// The stage lines and every refusal are [`crate::login`]'s, so `hydra gui`
/// logs in exactly as `hydra login`/`hydra play` do.  Nothing here touches a
/// display, a thread or an event loop.
pub(crate) fn gui_connect_from(
    target: &login::Target,
    password: &str,
) -> Result<Live, String> {
    let login = login::sign_in(target, password)?;
    let launch = login.launch().clone();
    let socket = login::open_game(&launch)?;
    Ok(Live {
        live: Arc::new(Mutex::new(LiveSession::new(target.character.clone()))),
        socket,
    })
}

impl Live {
    /// Start the reader and the writer thread.
    ///
    /// The reader owns a duplicate of the socket: it reads one line at a time
    /// (CRLF stripped by [`LiveSession::feed_bytes`]), hands it to the parser
    /// and then to the model, and marks the model disconnected at EOF.  The
    /// writer owns the original socket and writes only what the window's input
    /// line sends.
    ///
    /// **No lock is held while a socket operation blocks**: the reader reads
    /// first and locks only to fold the finished line in, and the writer never
    /// touches the model at all.
    pub(crate) fn start(self) -> Result<Running, String> {
        let read = self
            .socket
            .try_clone()
            .map_err(|error| format!("the game socket could not be duplicated: {error}"))?;
        let reader_live = Arc::clone(&self.live);
        thread::Builder::new()
            .name("hydra-gui-reader".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(read);
                let _ = pump(&mut reader, &reader_live);
                // The game closed, or the socket broke: either way the window
                // says so and the input line is disabled.
                locked(&reader_live).closed();
            })
            .map_err(|error| format!("the reader thread could not be started: {error}"))?;

        let write = self
            .socket
            .try_clone()
            .map_err(|error| format!("the game socket could not be duplicated: {error}"))?;
        let (input, inbox) = mpsc::channel::<String>();
        thread::Builder::new()
            .name("hydra-gui-writer".to_string())
            .spawn(move || {
                let _ = write_lines(write, inbox);
            })
            .map_err(|error| format!("the writer thread could not be started: {error}"))?;

        Ok(Running {
            live: self.live,
            input,
            socket: self.socket,
        })
    }
}

/// Read the game's lines until EOF, folding each one into the model.
///
/// `read_until` is used rather than `read_line` because the game's stream is
/// bytes: a line that is not valid UTF-8 must not end the session (the parser's
/// own `decode_wire_line` maps those bytes one-to-one, and
/// [`LiveSession::feed_bytes`] does the same).
pub(crate) fn pump(
    reader: &mut impl BufRead,
    live: &Arc<Mutex<LiveSession>>,
) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        locked(live).feed_bytes(&line);
    }
}

/// Write every line the window sends to the game socket, one `\n`-terminated
/// line each, flushed (the shape `docs/LICH_WIRE.md` §3 describes for a typed
/// command; `lich::write_text` writes the same bytes on the `play` path).
fn write_lines(mut socket: TcpStream, inbox: Receiver<String>) -> io::Result<()> {
    for line in inbox {
        if line.is_empty() {
            continue;
        }
        socket.write_all(line.as_bytes())?;
        socket.write_all(b"\n")?;
        socket.flush()?;
    }
    Ok(())
}

/// The window value for a live session: the recovered `"Hydra"` window around a
/// [`hydra_gui::app::Hub`] holding the model.
///
/// Split out of [`run`] so a test can build (and assert) the launch without an
/// event loop — exactly like `crate::gui_launch` for the hub window.
pub(crate) fn launch(
    start: &Start,
    live: Arc<Mutex<LiveSession>>,
    input: Sender<String>,
) -> Launch {
    app::build_live(start, true, live, input)
}

/// `hydra gui <character> [--account A] [--game G]`.
pub(crate) fn run(tokens: &[String]) -> Result<(), String> {
    run_with(LABEL, tokens)
}

/// The body of [`run`], labelled (so a test can assert the prefix without the
/// process's own name).
pub(crate) fn run_with(label: &str, tokens: &[String]) -> Result<(), String> {
    let arguments = login::parse(tokens).map_err(|message| format!("{label}: {message}"))?;
    let target = login::resolve(&arguments).map_err(|message| format!("{label}: {message}"))?;
    let password =
        login::password(&target.account).map_err(|message| format!("{label}: {message}"))?;
    // Login and game socket first, window second: a refusal is reported on
    // stderr and never as an empty window.
    let live = gui_connect_from(&target, &password).map_err(|message| format!("{label}: {message}"))?;
    let running = live
        .start()
        .map_err(|message| format!("{label}: {message}"))?;

    let start = Start::new(target.character.clone(), data_dir(&target));
    let window = launch(&start, Arc::clone(&running.live), running.input.clone());
    let (width, height) = window.size();
    eprintln!(
        "{label}: opening the live window {:?} ({width:.0}x{height:.0}, wgpu) for {}",
        window.title, target.character
    );
    let outcome = window.run();

    // The window is gone.  Shut the game socket down so the game sees the
    // session end, and so the reader thread is not left waiting on it.
    let _ = running.socket.shutdown(Shutdown::Both);
    outcome.map_err(|error| format!("{label}: the window could not be opened: {error}"))
}

/// The per-character data folder the window's placements are read from: the
/// folder the login's certificate pin already lives in.
fn data_dir(target: &login::Target) -> PathBuf {
    target
        .pin
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use hydra_mocks::{EaccessScript, MockEaccess, MockGame};

    /// A synthetic game stream: a room (both forms), a story line, vitals, a
    /// hand, a compass and a prompt.  No captured game text — every value is
    /// invented for the test.
    const STREAM: &str = concat!(
        "<component id=\"roomName\" value=\"A Synthetic Crossing\"/>\r\n",
        "<component id=\"roomDesc\" value=\"A wide synthetic road.&#10;Obvious exits: north, southeast.\"/>\r\n",
        "<nav rm=\"4242\"/>\r\n",
        "A synthetic breeze crosses the road.\r\n",
        "<progressBar id=\"health\" value=\"80\"/>\r\n",
        "<left>a synthetic shield</left>\r\n",
        "<compass><dir value=\"n\"/><dir value=\"se\"/></compass>\r\n",
        "<prompt>synthetic&gt;</prompt>\r\n",
    );

    /// Wait until `done` holds, or fail after `limit`.
    ///
    /// The mocks and the session threads are asynchronous: a test that asserted
    /// straight away would be racing them.
    fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + limit;
        while !done() {
            assert!(Instant::now() < deadline, "the condition never held within {limit:?}");
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// A temp folder, removed when the test ends.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir()
                .join(format!("hydra-gui-{tag}-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&path).expect("cannot create the temp folder");
            TempDir { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// A mock eaccess + mock game pair, and the [`login::Target`] that points at
    /// them.  No process environment is touched: the host, the port and the pin
    /// are handed over explicitly.
    struct Mocks {
        game: MockGame,
        eaccess: MockEaccess,
        script: EaccessScript,
        target: login::Target,
        _data: TempDir,
    }

    fn mocks(tag: &str, lingering: bool) -> Mocks {
        let data = TempDir::new(tag);
        let game = if lingering {
            MockGame::start_lingering(STREAM)
        } else {
            MockGame::start(STREAM)
        }
        .expect("the mock game server starts");
        let mut script = EaccessScript::synthetic();
        script.launch.host = "127.0.0.1".to_string();
        script.launch.port = game.addr().port();
        let eaccess =
            MockEaccess::start(script.clone()).expect("the mock eaccess server starts");
        let target = login::Target {
            character: "Aelith".to_string(),
            account: script.account.clone(),
            game: script.launch.code.clone(),
            pin: data.path.join(login::PIN_FILE),
            host: "127.0.0.1".to_string(),
            port: eaccess.addr().port(),
        };
        Mocks {
            game,
            eaccess,
            script,
            target,
            _data: data,
        }
    }

    #[test]
    fn the_chain_runs_from_the_login_to_the_model() {
        let mocks = mocks("chain", false);
        let live = gui_connect_from(&mocks.target, &mocks.script.password)
            .expect("the loopback login succeeds");

        // The whole login and the game socket happened: the game saw the two
        // handshake lines, the certificate was pinned, no SNI was sent.  The
        // mock records the handshake on its own thread, so the read is waited
        // for rather than assumed to have landed already.
        wait_for(Duration::from_secs(10), || {
            mocks.game.received().len() == 2
        });
        assert_eq!(
            mocks.game.received(),
            vec![
                mocks.script.launch.key.clone(),
                hydra_mocks::CLIENT_IDENTIFICATION.to_string()
            ]
        );
        assert!(mocks.target.pin.is_file(), "the certificate was pinned");
        assert!(!mocks.eaccess.sni_seen());

        // Now the stream, through the parser, into the model — no window.
        let mut reader = BufReader::new(live.socket.try_clone().expect("a socket clone"));
        pump(&mut reader, &live.live).expect("the mock game closes its side");
        locked(&live.live).closed();

        let session = locked(&live.live);
        assert_eq!(session.character, "Aelith");
        assert_eq!(session.view.room, "A Synthetic Crossing");
        assert_eq!(session.description(), &[
            "A wide synthetic road.".to_string(),
            "Obvious exits: north, southeast.".to_string()
        ]);
        assert_eq!(session.view.exits, vec!["n".to_string(), "se".to_string()]);
        assert_eq!(session.room_number, "4242");
        assert_eq!(session.vitals.words(), "Sp ?, HP 80%, MP ?, SP ?");
        assert_eq!(session.hands.left(), Some("a synthetic shield"));
        assert_eq!(session.compass, vec!["n".to_string(), "se".to_string()]);
        assert_eq!(session.view.prompt, "synthetic>");
        assert!(session
            .story
            .text(8)
            .iter()
            .any(|line| line == "A synthetic breeze crosses the road."));
        // `closed()` was called: the indicator says disconnected, and the
        // recovered sentence is the last story line.
        assert!(!session.connected);
        assert_eq!(
            session.story.text(1),
            vec![hydra_gui::live::GAME_CLOSED.to_string()]
        );
        drop(session);

        mocks.eaccess.join().expect("the mock eaccess ends cleanly");
        mocks.game.join().expect("the mock game ends cleanly");
    }

    #[test]
    fn a_typed_line_reaches_the_game_socket() {
        let mocks = mocks("typing", true);
        let live = gui_connect_from(&mocks.target, &mocks.script.password)
            .expect("the loopback login succeeds");
        let running = live.start().expect("the two threads start");

        // What the window's input line does: hand the bare line to the writer.
        running
            .input
            .send("look".to_string())
            .expect("the writer thread is alive");

        // The game really received it, `\n`-terminated and alone.
        wait_for(Duration::from_secs(10), || !mocks.game.sent().is_empty());
        assert_eq!(mocks.game.sent(), vec!["look".to_string()]);

        // The reader thread is still following the stream; closing the socket
        // is what ends it (the window does this when it closes).
        let _ = running.socket.shutdown(Shutdown::Both);
        wait_for(Duration::from_secs(10), || !locked(&running.live).connected);
        assert_eq!(
            locked(&running.live).story.text(1),
            vec![hydra_gui::live::GAME_CLOSED.to_string()]
        );

        mocks.eaccess.join().expect("the mock eaccess ends cleanly");
        mocks.game.join().expect("the mock game ends cleanly");
    }

    #[test]
    fn the_window_is_the_recovered_hub_window() {
        let live = Arc::new(Mutex::new(LiveSession::new("Aelith")));
        let (input, _inbox) = mpsc::channel::<String>();
        let window = launch(&Start::new("Aelith", "/synthetic/data"), live, input);
        assert_eq!(window.title, "Hydra");
        assert_eq!(window.size(), (660.0, 520.0));
        assert_eq!(window.options.renderer, eframe::Renderer::Wgpu);
    }

    #[test]
    fn a_bad_command_line_is_a_labelled_message_before_any_network() {
        // No character at all, and an option `hydra gui` does not take: both are
        // `hydra gui: …` before a socket is touched.
        let message = run_with(LABEL, &[]).unwrap_err();
        assert!(message.starts_with("hydra gui: "), "{message}");
        assert!(message.contains("no character was named"), "{message}");
        let message = run_with(LABEL, &["Aelith".into(), "--acount".into(), "main".into()])
            .unwrap_err();
        assert!(message.starts_with("hydra gui: "), "{message}");
        assert!(
            message.contains("not an option this command takes"),
            "{message}"
        );
    }

    #[test]
    fn the_data_folder_is_the_one_the_pin_lives_in() {
        let target = login::Target {
            character: "Aelith".to_string(),
            account: "main".to_string(),
            game: "GS3".to_string(),
            pin: PathBuf::from("/synthetic/data/simu.pem"),
            host: "127.0.0.1".to_string(),
            port: 1,
        };
        assert_eq!(data_dir(&target), PathBuf::from("/synthetic/data"));
    }
}
