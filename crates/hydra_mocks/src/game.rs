//! The mock game server: plain TCP, the two-line handshake, then XML.
//!
//! Once a login has handed Hydra a `GAMEHOST`/`GAMEPORT`/`KEY` (SPEC_P6.md §1.7,
//! `docs/LICH_WIRE.md` §2), Hydra dials the game and sends exactly two
//! newline-terminated lines: the launch `KEY`, then [`CLIENT_IDENTIFICATION`].
//! Everything after that is the game's XML stream.
//!
//! This server does the minimum a client can be tested against: it reads those
//! two lines, records them, writes the synthetic stream it was started with
//! *verbatim* (nothing added, not even a newline), flushes, and closes.  No TLS,
//! no game logic, no captured game text.
//!
//! [`MockGame::start_lingering`] is the same server with one difference: it
//! keeps the connection open after the stream, so a client that types (the
//! `hydra play` stdin tests) has a live game socket to write to, and every line
//! it sends after the two handshake lines is recorded by [`MockGame::sent`].
//!
//! [`MockGame::start_series`] answers **several** connections in a row, one
//! stream each: the multi-connection game a reconnecting client needs (`hydra
//! serve` rests and dials again when the game drops, so a single-connection
//! mock would answer the first connection and refuse every retry).  Every
//! handshake pair and every typed line from every connection is recorded, in
//! order.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The front-end identification line a Hydra client sends after the launch key.
pub const CLIENT_IDENTIFICATION: &str = "/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML";

/// How long the client has to send its handshake before the mock gives up.
///
/// Without it a client that connects and then goes quiet would park the server
/// thread forever, and [`MockGame::join`] with it.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for the client to close after the stream has been written.
///
/// Draining is what makes the close orderly: a socket dropped with unread input
/// can be reset, and a reset can lose the output the client is about to read.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// How long a lingering server keeps the connection open at most.
///
/// A client that never closes must not park the thread (and [`MockGame::join`]
/// with it) forever; a test that needs longer is asking the wrong question.
const LINGER_LIMIT: Duration = Duration::from_secs(60);

fn lock(received: &Arc<Mutex<Vec<String>>>) -> MutexGuard<'_, Vec<String>> {
    received
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A game server on `127.0.0.1` that answers one connection with one stream
/// (or one connection per stream, for [`MockGame::start_series`]).
pub struct MockGame {
    addr: SocketAddr,
    received: Arc<Mutex<Vec<String>>>,
    sent: Arc<Mutex<Vec<String>>>,
    connections: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<io::Result<()>>>,
}

impl MockGame {
    /// Bind `127.0.0.1:0` and start serving `stream` verbatim.
    ///
    /// Returns as soon as the listener is bound: nothing waits for a client.
    pub fn start(stream: &str) -> io::Result<MockGame> {
        MockGame::with_linger(stream, false)
    }

    /// [`MockGame::start`] with the connection left open after the stream.
    ///
    /// A client that types into a live session needs a game socket that stays
    /// up (and a game that reads what it types): this one writes the stream,
    /// does **not** half-close, and records every line the client sends
    /// afterwards for [`MockGame::sent`].  It closes when the client does, when
    /// [`MockGame::join`] is called, or after [`LINGER_LIMIT`].
    pub fn start_lingering(stream: &str) -> io::Result<MockGame> {
        MockGame::with_linger(stream, true)
    }

    /// Answer a **series** of connections, one stream each.
    ///
    /// `streams` is `(stream, linger)` per connection, in the order the client
    /// is expected to arrive: the first pair answers the first connection, the
    /// second pair the second, and so on.  `linger` means the same as it does
    /// for [`MockGame::start_lingering`] — the connection is left open after
    /// its stream.  Every handshake pair goes to [`MockGame::received`] and
    /// every line typed after a handshake to [`MockGame::sent`], so a test can
    /// see all of them in order; [`MockGame::connections`] counts the
    /// handshakes that completed.
    ///
    /// A client that stops connecting leaves the remaining streams unserved and
    /// [`MockGame::join`] returns as soon as the stop flag is seen.
    pub fn start_series(streams: Vec<(String, bool)>) -> io::Result<MockGame> {
        MockGame::with_streams(streams)
    }

    fn with_linger(stream: &str, linger: bool) -> io::Result<MockGame> {
        MockGame::with_streams(vec![(stream.to_string(), linger)])
    }

    fn with_streams(streams: Vec<(String, bool)>) -> io::Result<MockGame> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let received = Arc::new(Mutex::new(Vec::new()));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let handle = {
            let received = Arc::clone(&received);
            let sent = Arc::clone(&sent);
            let connections = Arc::clone(&connections);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("mock-game".to_string())
                .spawn(move || serve(listener, streams, received, sent, connections, stop))?
        };

        Ok(MockGame {
            addr,
            received,
            sent,
            connections,
            stop,
            handle: Some(handle),
        })
    }

    /// The loopback address the game server is listening on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The handshake lines the client sent, in order: the `KEY`, then the
    /// front-end identification.
    ///
    /// A series records two lines per connection, so a client that reconnected
    /// left four.
    pub fn received(&self) -> Vec<String> {
        lock(&self.received).clone()
    }

    /// The lines the client sent *after* the handshake, in order: what a live
    /// session types at the game (empty for a client that never types).
    pub fn sent(&self) -> Vec<String> {
        lock(&self.sent).clone()
    }

    /// How many handshakes completed: one per connection served.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Wait for the server thread to end.
    ///
    /// The accept is woken by a throwaway connection, so this returns even if no
    /// client ever connected.
    pub fn join(mut self) -> io::Result<()> {
        self.shutdown()
    }

    fn shutdown(&mut self) -> io::Result<()> {
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        match handle.join() {
            Ok(result) => result,
            Err(_) => Err(io::Error::new(
                io::ErrorKind::Other,
                "the mock game server thread panicked",
            )),
        }
    }
}

impl Drop for MockGame {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Accept one connection per stream in `streams` and serve each of them.
///
/// The stop flag is checked right after each accept, so the throwaway
/// connection [`MockGame::join`] makes to wake the accept is dropped instead of
/// being served as the next stream.
fn serve(
    listener: TcpListener,
    streams: Vec<(String, bool)>,
    received: Arc<Mutex<Vec<String>>>,
    sent: Arc<Mutex<Vec<String>>>,
    connections: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    for (stream, linger) in streams {
        let (socket, _peer) = listener.accept()?;
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        serve_one(socket, &stream, &received, &sent, &connections, &stop, linger)?;
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
    }
    Ok(())
}

fn serve_one(
    socket: TcpStream,
    stream: &str,
    received: &Arc<Mutex<Vec<String>>>,
    sent: &Arc<Mutex<Vec<String>>>,
    connections: &Arc<AtomicUsize>,
    stop: &Arc<AtomicBool>,
    linger: bool,
) -> io::Result<()> {
    socket.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut socket = BufReader::new(socket);

    // The two handshake lines: the launch key, then the front-end line.
    for line_number in 1..=2 {
        let mut line = Vec::new();
        if socket.read_until(b'\n', &mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("the client closed before handshake line {line_number}"),
            ));
        }
        let line = stripped(&line);
        lock(received).push(line);
    }
    connections.fetch_add(1, Ordering::SeqCst);

    {
        let socket = socket.get_mut();
        socket.write_all(stream.as_bytes())?;
        socket.flush()?;
        if !linger {
            let _ = socket.shutdown(Shutdown::Write);
        }
    }

    // Read what the client sends after the handshake, so the close is an
    // orderly one: a socket dropped with unread input can be reset, and a reset
    // can lose the stream the client is about to read.  In linger mode this is
    // also how a typed line reaches the test (`MockGame::sent`).
    let _ = socket.get_ref().set_read_timeout(Some(DRAIN_TIMEOUT));
    let deadline = Instant::now() + LINGER_LIMIT;
    let mut line = Vec::new();
    loop {
        if linger && (stop.load(Ordering::SeqCst) || Instant::now() >= deadline) {
            break;
        }
        line.clear();
        match socket.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => lock(sent).push(stripped(&line)),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                // A drained client is done; a lingering one is just quiet.
                if !linger {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    Ok(())
}

/// One line of the stream, without its line ending.
fn stripped(line: &[u8]) -> String {
    let mut line = line;
    while line.last() == Some(&b'\n') || line.last() == Some(&b'\r') {
        line = &line[..line.len() - 1];
    }
    String::from_utf8_lossy(line).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A stream that is deliberately *not* newline-terminated: the mock must not
    /// add one, so a client sees exactly what the test asked for.
    const SYNTHETIC_STREAM: &str = "<mode id=\"GS3\"/>\n\
<output class=\"mono\"/>synthetic line</output>\n\
<prompt time=\"1\"/>";

    fn read_all(mut socket: TcpStream) -> String {
        let mut got = Vec::new();
        socket.read_to_end(&mut got).unwrap();
        String::from_utf8(got).unwrap()
    }

    #[test]
    fn the_client_sees_exactly_the_synthetic_stream() {
        let game = MockGame::start(SYNTHETIC_STREAM).unwrap();
        let mut socket = TcpStream::connect(game.addr()).unwrap();

        let key = "9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d";
        socket.write_all(format!("{key}\n").as_bytes()).unwrap();
        socket
            .write_all(format!("{CLIENT_IDENTIFICATION}\n").as_bytes())
            .unwrap();
        socket.flush().unwrap();

        assert_eq!(read_all(socket), SYNTHETIC_STREAM);

        assert_eq!(
            game.received(),
            vec![key.to_string(), CLIENT_IDENTIFICATION.to_string()]
        );
        game.join().unwrap();
    }

    #[test]
    fn the_handshake_is_recorded_when_it_arrives_in_one_packet() {
        let game = MockGame::start("").unwrap();
        let mut socket = TcpStream::connect(game.addr()).unwrap();
        socket
            .write_all(b"key\n/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n")
            .unwrap();
        socket.flush().unwrap();

        assert_eq!(read_all(socket), "");
        assert_eq!(
            game.received(),
            vec![
                "key".to_string(),
                CLIENT_IDENTIFICATION.to_string()
            ]
        );
        game.join().unwrap();
    }

    /// The handshake a client sends, then the server's stream read back
    /// exactly: a lingering server never closes, so this cannot read to EOF.
    fn connect(game: &MockGame) -> TcpStream {
        let mut socket = TcpStream::connect(game.addr()).unwrap();
        socket
            .write_all(b"key\n")
            .and_then(|()| socket.write_all(format!("{CLIENT_IDENTIFICATION}\n").as_bytes()))
            .and_then(|()| socket.flush())
            .unwrap();
        let mut got = vec![0u8; SYNTHETIC_STREAM.len()];
        socket.read_exact(&mut got).unwrap();
        assert_eq!(String::from_utf8(got).unwrap(), SYNTHETIC_STREAM);
        socket
    }

    #[test]
    fn a_lingering_server_records_what_the_client_types() {
        let game = MockGame::start_lingering(SYNTHETIC_STREAM).unwrap();
        let mut socket = connect(&game);
        assert_eq!(game.received().len(), 2);
        assert_eq!(game.sent(), Vec::<String>::new());

        socket.write_all(b"look\nquit\n").unwrap();
        socket.flush().unwrap();

        let deadline = Instant::now() + READ_TIMEOUT;
        while game.sent().len() < 2 {
            assert!(
                Instant::now() < deadline,
                "the server never read the typed lines: {:?}",
                game.sent()
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(game.sent(), vec!["look".to_string(), "quit".to_string()]);
        // The handshake lines stay separate from what was typed.
        assert_eq!(game.received().len(), 2);
        drop(socket);
        game.join().unwrap();
    }

    #[test]
    fn a_client_that_never_connects_still_joins() {
        let game = MockGame::start(SYNTHETIC_STREAM).unwrap();
        assert_eq!(game.addr().ip().to_string(), "127.0.0.1");
        assert_ne!(game.addr().port(), 0);
        assert_eq!(game.received(), Vec::<String>::new());
        game.join().unwrap();
    }

    #[test]
    fn a_client_that_closes_early_is_reported() {
        let game = MockGame::start(SYNTHETIC_STREAM).unwrap();
        let mut socket = TcpStream::connect(game.addr()).unwrap();
        socket.write_all(b"only-one-line\n").unwrap();
        socket.flush().unwrap();
        // Wait for the server to have read the first line before walking away,
        // so the test cannot race the accept: it must then report the missing
        // second line rather than hang or panic.
        let deadline = std::time::Instant::now() + READ_TIMEOUT;
        while game.received().is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "the server never read the first line"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(socket);

        let error = game.join().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(error.to_string().contains("handshake line 2"), "{error}");
    }

    /// Send the two handshake lines with `key`, and read `length` bytes back.
    fn handshake(socket: &mut TcpStream, key: &str, length: usize) -> String {
        socket
            .write_all(format!("{key}\n{CLIENT_IDENTIFICATION}\n").as_bytes())
            .and_then(|()| socket.flush())
            .unwrap();
        let mut got = vec![0u8; length];
        socket.read_exact(&mut got).unwrap();
        String::from_utf8(got).unwrap()
    }

    #[test]
    fn a_series_answers_every_connection_in_order() {
        // The first connection answers its stream and closes; the second
        // lingers, so a reconnected client has a live socket to type at.
        let second = "<prompt time=\"2\"/>";
        let game = MockGame::start_series(vec![
            (SYNTHETIC_STREAM.to_string(), false),
            (second.to_string(), true),
        ])
        .unwrap();
        assert_eq!(game.connections(), 0);

        let mut first = TcpStream::connect(game.addr()).unwrap();
        assert_eq!(
            handshake(&mut first, "first-key", SYNTHETIC_STREAM.len()),
            SYNTHETIC_STREAM
        );
        // The first connection is half-closed by the server: the client sees
        // EOF without closing its own end.
        assert_eq!(read_all(first), "");

        let mut second_socket = TcpStream::connect(game.addr()).unwrap();
        assert_eq!(handshake(&mut second_socket, "second-key", second.len()), second);
        second_socket.write_all(b"look\n").unwrap();
        second_socket.flush().unwrap();
        let deadline = Instant::now() + READ_TIMEOUT;
        while game.sent().is_empty() {
            assert!(Instant::now() < deadline, "the second connection read nothing");
            thread::sleep(Duration::from_millis(5));
        }

        // Both handshakes are recorded, in order, and both typed lines are kept
        // apart from them.
        assert_eq!(
            game.received(),
            vec![
                "first-key".to_string(),
                CLIENT_IDENTIFICATION.to_string(),
                "second-key".to_string(),
                CLIENT_IDENTIFICATION.to_string(),
            ]
        );
        assert_eq!(game.sent(), vec!["look".to_string()]);
        assert_eq!(game.connections(), 2);

        drop(second_socket);
        game.join().unwrap();
    }

    #[test]
    fn a_series_with_unused_streams_still_joins() {
        // The client connects once and leaves: the rest of the series is never
        // served, and `join` must not wait for it.
        let game = MockGame::start_series(vec![
            (SYNTHETIC_STREAM.to_string(), false),
            (SYNTHETIC_STREAM.to_string(), true),
        ])
        .unwrap();
        let mut socket = TcpStream::connect(game.addr()).unwrap();
        assert_eq!(
            handshake(&mut socket, "only-key", SYNTHETIC_STREAM.len()),
            SYNTHETIC_STREAM
        );
        drop(socket);
        game.join().unwrap();
    }
}
