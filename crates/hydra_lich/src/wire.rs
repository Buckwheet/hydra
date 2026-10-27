//! The observed Hydra <-> Lich wire protocol.
//!
//! Ground truth: `docs/LICH_WIRE.md`, captured from the real Hydra (v0.1.0-beta.7,
//! Windows build) driving the real Lich 5.22.0.  This module holds the pieces of
//! that protocol that are pure data handling — constants, the two-line stdin
//! handshake, the keyed accept on the listening socket, `<c>`-versus-bare
//! command framing and the 0x1F stdout chunk prefix.  The process side lives in
//! [`crate::relay`] and [`crate::child`].
//!
//! ```text
//! Hydra (listens on 127.0.0.1:<ephemeral>)        Lich (`-g 127.0.0.1:<port>`)
//!   spawn ────────────────────────────────────────▶ lich.rbw --pipe --stormfront …
//!   stdin:  <token>\n                                        │
//!           /FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n │
//!   accept ◀──────────────────────────────────────────────────┘
//!           socket line 1: <token>\n              (the key)
//!           socket line 2: /FE:WRAYTH … /XML\n    (client identification)
//!   socket: game XML  ──▶ Lich          Lich ──▶ <c>command\n | bare line\n
//!   stdout: game chunks carry a leading 0x1F byte per chunk; Lich's own
//!           lines (banner, `<output class="mono"/>…`) are unprefixed.
//! ```
//!
//! Everything here is synthetic-testable on loopback; no game text is involved.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// The client-identification line, exactly as observed on both channels.
///
/// Hydra writes it to Lich's stdin as the second line, and Lich sends the same
/// line back as the second line of the keyed socket handshake.
pub const CLIENT_IDENTIFICATION: &str = "/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML";

/// Convenience alias: the same line, named for the socket channel.
pub const CLIENT_IDENTIFICATION_LINE: &str = CLIENT_IDENTIFICATION;

/// The byte Lich puts in front of every forwarded game chunk on its stdout
/// (US, 0x1F).  Lich's own output lines carry no prefix.
pub const GAME_CHUNK_PREFIX: u8 = 0x1F;

/// The Stormfront command prefix: a line Lich originated starts with `<c>`.
/// Text the player typed and Lich merely forwarded is the bare line.
pub const COMMAND_TAG: &str = "<c>";

/// The recovered accept deadline: "Lich did not connect within 60 seconds"
/// (`Ended::DidNotConnect { seconds: 60 }`).
pub const ACCEPT_TIMEOUT: Duration = Duration::from_secs(60);

/// Longest handshake line accepted before the peer is refused.  The real lines
/// are 64 and 51 bytes; the cap only stops a hostile/garbled peer from feeding
/// an unbounded line into the accept.
pub const MAX_HANDSHAKE_LINE: usize = 4096;

// ---------------------------------------------------------------------------
// Handshake data
// ---------------------------------------------------------------------------

/// `true` when `line` is exactly the client-identification line.
pub fn is_client_identification(line: &str) -> bool {
    line == CLIENT_IDENTIFICATION
}

/// The exact bytes Hydra writes to Lich's stdin right after the spawn:
///
/// ```text
/// <token>\n/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n
/// ```
pub fn handshake_bytes(token: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(token.len() + CLIENT_IDENTIFICATION.len() + 2);
    bytes.extend_from_slice(token.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(CLIENT_IDENTIFICATION.as_bytes());
    bytes.push(b'\n');
    bytes
}

/// Write [`handshake_bytes`] to Lich's stdin and flush it.
///
/// Called once, immediately after the spawn (`docs/LICH_WIRE.md` §2, step 1).
/// The token is never logged by this crate.
pub fn write_lich_handshake<W: Write>(stdin: &mut W, token: &str) -> io::Result<()> {
    stdin.write_all(&handshake_bytes(token))?;
    stdin.flush()
}

/// Read one `\n`-terminated handshake line, stripping the `\n` and a
/// preceding `\r`.
///
/// Bytes after the delimiter stay in the stream: the accept reads one byte at a
/// time so the game stream that follows the handshake is not swallowed.  A peer
/// that closes early yields the empty string, which equals neither handshake
/// line, so the accept still refuses it.
pub fn read_handshake_line<R: Read>(stream: &mut R) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
                if line.len() > MAX_HANDSHAKE_LINE {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Lich sent an over-long handshake line",
                    ));
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// Server-side keyed accept: validate the two handshake lines Lich sends.
///
/// Observed (`docs/LICH_WIRE.md` §2, step 2): the first socket line must equal
/// the token Hydra generated (`new_token`, 64 lowercase hex), the second must
/// equal [`CLIENT_IDENTIFICATION`].  Any other content — including a short or
/// absent line — is a refusal: the caller drops the stream, which closes it.
///
/// The token is never echoed in an error message.
pub fn accept_keyed<R: Read>(stream: &mut R, token: &str) -> io::Result<()> {
    let first = read_handshake_line(stream)?;
    if first != token {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Lich's first line was not this session's token",
        ));
    }
    let second = read_handshake_line(stream)?;
    if !is_client_identification(&second) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Lich's second line was not the client identification line",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Listener helpers
// ---------------------------------------------------------------------------

/// Bind the loopback listener Hydra plays the game server on.
///
/// The port is ephemeral (`127.0.0.1:0`); Lich is told it through
/// `-g 127.0.0.1:<port>` and connects to it.
pub fn listen_loopback() -> io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", 0u16))
}

/// The port a listener is bound to.
pub fn loopback_port(listener: &TcpListener) -> io::Result<u16> {
    Ok(listener.local_addr()?.port())
}

/// [`listen_loopback`] plus the chosen port.
pub fn keyed_listener() -> io::Result<(TcpListener, u16)> {
    let listener = listen_loopback()?;
    let port = loopback_port(&listener)?;
    Ok((listener, port))
}

/// Accept one connection and run [`accept_keyed`] on it.
///
/// On success the stream is returned with the recovered [`ACCEPT_TIMEOUT`] on
/// reads and writes.  On a mismatch or timeout the stream is dropped before the
/// error is returned, so a peer that guessed wrong sees the connection close
/// instead of a live socket.
pub fn accept_keyed_listener(listener: &TcpListener, token: &str) -> io::Result<TcpStream> {
    let (mut stream, _addr) = listener.accept()?;
    stream.set_read_timeout(Some(ACCEPT_TIMEOUT))?;
    stream.set_write_timeout(Some(ACCEPT_TIMEOUT))?;
    match accept_keyed(&mut stream, token) {
        Ok(()) => Ok(stream),
        Err(err) => {
            // Dropping the stream closes it: the peer gets a reset/EOF, never
            // an accepted connection.
            drop(stream);
            Err(err)
        }
    }
}

// ---------------------------------------------------------------------------
// `<c>` command framing vs bare lines (Lich -> Hydra, socket)
// ---------------------------------------------------------------------------

/// One newline-delimited line Lich sent up the socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LichFrame {
    /// `<c>exp` — a command Lich (or one of its scripts) originated.
    Command(String),
    /// A bare line: text the player typed that Lich forwarded (`look`), or a
    /// bare protocol line such as `_injury 2`.
    Bare(String),
}

impl LichFrame {
    /// Classify one line.  A trailing `\n`/`\r` is ignored, so a caller may
    /// pass either a raw socket line or an already-split one.
    pub fn parse(line: &str) -> Self {
        let line = line.trim_end_matches(['\n', '\r']);
        match line.strip_prefix(COMMAND_TAG) {
            Some(command) => LichFrame::Command(command.to_string()),
            None => LichFrame::Bare(line.to_string()),
        }
    }

    /// The line without its `<c>` prefix and without the line ending.
    pub fn text(&self) -> &str {
        match self {
            LichFrame::Command(text) | LichFrame::Bare(text) => text,
        }
    }

    /// Was this a Lich-originated `<c>` command?
    pub fn is_command(&self) -> bool {
        matches!(self, LichFrame::Command(_))
    }

    /// Render the line back to its wire form (no line ending added).
    pub fn to_wire(&self) -> String {
        match self {
            LichFrame::Command(text) => command_frame(text),
            LichFrame::Bare(text) => text.clone(),
        }
    }
}

/// Build a `<c>` command frame: `command_frame("exp") == "<c>exp"`.
///
/// The line ending is the caller's business (the socket writer appends `\n`).
pub fn command_frame(command: &str) -> String {
    let mut frame = String::with_capacity(COMMAND_TAG.len() + command.len());
    frame.push_str(COMMAND_TAG);
    frame.push_str(command);
    frame
}

/// Build a bare line (typed input / bare protocol line); the inverse of the
/// `Command` half of [`LichFrame`] and a no-op on un-prefixed text.
pub fn bare_frame(line: &str) -> String {
    line.trim_end_matches(['\n', '\r']).to_string()
}

/// `true` when the line is a Lich-originated `<c>` command.
pub fn is_command_line(line: &str) -> bool {
    line.trim_end_matches(['\n', '\r']).starts_with(COMMAND_TAG)
}

/// The command text of a `<c>` line, or `None` for a bare line.
pub fn strip_command_tag(line: &str) -> Option<&str> {
    line.trim_end_matches(['\n', '\r'])
        .strip_prefix(COMMAND_TAG)
}

// ---------------------------------------------------------------------------
// 0x1F chunk prefix (Lich stdout)
// ---------------------------------------------------------------------------

/// `true` when Lich prefixed this stdout line with [`GAME_CHUNK_PREFIX`].
pub fn has_game_chunk_prefix(line: &str) -> bool {
    line.starts_with(GAME_CHUNK_PREFIX as char)
}

/// The chunk with its 0x1F prefix removed, or `None` when the line is Lich's
/// own output.
pub fn strip_game_chunk_prefix(line: &str) -> Option<&str> {
    line.strip_prefix(GAME_CHUNK_PREFIX as char)
}

/// One line of Lich's stdout, split by provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LichOut {
    /// Game XML Lich forwarded, with the 0x1F chunk byte stripped: this is what
    /// the protocol parser may see.
    Game(String),
    /// Lich's own output (the banner, `<output class="mono"/>…`, errors),
    /// unprefixed and passed through unchanged.
    Own(String),
}

impl LichOut {
    /// Classify one stdout line, stripping the 0x1F chunk prefix from game
    /// chunks so it never reaches the protocol parser.
    pub fn parse(line: &str) -> Self {
        match strip_game_chunk_prefix(line) {
            Some(text) => LichOut::Game(text.to_string()),
            None => LichOut::Own(line.to_string()),
        }
    }

    /// The line's text without the chunk prefix or the line ending.
    pub fn text(&self) -> &str {
        match self {
            LichOut::Game(text) | LichOut::Own(text) => text,
        }
    }

    /// Was this a forwarded game chunk?
    pub fn is_game(&self) -> bool {
        matches!(self, LichOut::Game(_))
    }

    /// Consume the classification and keep the text.
    pub fn into_text(self) -> String {
        match self {
            LichOut::Game(text) | LichOut::Own(text) => text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpStream;
    use std::thread;

    fn synthetic_token() -> String {
        // 64 lowercase hex characters, the shape `new_token` produces.
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()
    }

    #[test]
    fn handshake_bytes_are_the_two_exact_lines() {
        let token = synthetic_token();
        let expected = format!("{token}\n/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n");
        assert_eq!(handshake_bytes(&token), expected.into_bytes());
        // No CR: the observed stdin lines end in a bare newline.
        assert!(!handshake_bytes(&token).contains(&b'\r'));
    }

    #[test]
    fn write_lich_handshake_writes_and_flushes() {
        let token = synthetic_token();
        let mut stdin = Cursor::new(Vec::new());
        write_lich_handshake(&mut stdin, &token).unwrap();
        assert_eq!(stdin.into_inner(), handshake_bytes(&token));
    }

    #[test]
    fn the_client_line_matches_the_captured_literal() {
        assert_eq!(CLIENT_IDENTIFICATION, CLIENT_IDENTIFICATION_LINE);
        assert_eq!(
            CLIENT_IDENTIFICATION,
            "/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML"
        );
        assert!(!CLIENT_IDENTIFICATION.contains('\n'));
        assert!(is_client_identification(CLIENT_IDENTIFICATION));
        assert!(!is_client_identification(
            "/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN"
        ));
        assert!(!is_client_identification(""));
    }

    #[test]
    fn accept_keyed_accepts_the_token_and_client_line() {
        let token = synthetic_token();
        let mut stream = Cursor::new(handshake_bytes(&token));
        accept_keyed(&mut stream, &token).unwrap();
    }

    #[test]
    fn accept_keyed_tolerates_crlf_and_keeps_the_stream_position() {
        let token = synthetic_token();
        let mut bytes = format!("{token}\r\n{CLIENT_IDENTIFICATION}\r\n").into_bytes();
        bytes.extend_from_slice(b"<room>\n");
        let mut stream = Cursor::new(bytes);
        accept_keyed(&mut stream, &token).unwrap();
        // The game stream that follows the handshake is still there.
        let mut rest = String::new();
        stream.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "<room>\n");
    }

    #[test]
    fn accept_keyed_rejects_a_wrong_token() {
        let token = synthetic_token();
        let mut stream = Cursor::new(handshake_bytes(&"f".repeat(64)));
        let err = accept_keyed(&mut stream, &token).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        // The message must not leak the token.
        assert!(!err.to_string().contains(&token));
    }

    #[test]
    fn accept_keyed_rejects_a_wrong_second_line() {
        let token = synthetic_token();
        let mut stream = Cursor::new(
            format!("{token}\n/FE:WRAYTH /VERSION:0.0.0 /P:WIN_UNKNOWN /XML\n").into_bytes(),
        );
        let err = accept_keyed(&mut stream, &token).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn accept_keyed_rejects_a_truncated_handshake() {
        let token = synthetic_token();
        let mut stream = Cursor::new(format!("{token}\n").into_bytes());
        let err = accept_keyed(&mut stream, &token).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn accept_keyed_rejects_an_over_long_line() {
        let token = synthetic_token();
        let mut stream = Cursor::new(vec![b'a'; MAX_HANDSHAKE_LINE + 2]);
        let err = accept_keyed(&mut stream, &token).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn the_listener_binds_loopback_and_accepts_a_good_handshake() {
        let (listener, port) = keyed_listener().unwrap();
        assert_eq!(loopback_port(&listener).unwrap(), port);
        assert_ne!(port, 0);

        let token = synthetic_token();
        let client_token = token.clone();
        let client = thread::spawn(move || {
            let mut stream =
                TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(5))
                    .unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            // Handshake plus the first game-stream line in one write: the
            // accept must not swallow the byte after the second line.
            let mut bytes = handshake_bytes(&client_token);
            bytes.extend_from_slice(b"look\n");
            stream.write_all(&bytes).unwrap();
            let mut line = String::new();
            let mut byte = [0u8; 1];
            while stream.read(&mut byte).unwrap() == 1 {
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0] as char);
            }
            line
        });

        let mut accepted = accept_keyed_listener(&listener, &token).unwrap();
        let mut rest = Vec::new();
        let mut byte = [0u8; 1];
        while rest.len() < 5 {
            assert_eq!(accepted.read(&mut byte).unwrap(), 1);
            rest.push(byte[0]);
        }
        assert_eq!(rest, b"look\n");
        accepted.write_all(b"<room>\n").unwrap();

        assert_eq!(client.join().unwrap(), "<room>");
    }

    #[test]
    fn a_wrong_token_is_refused_and_the_stream_closed() {
        let (listener, port) = keyed_listener().unwrap();
        let token = synthetic_token();
        let client = thread::spawn(move || {
            let mut stream =
                TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(5))
                    .unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream.write_all(b"deadbeef\n").unwrap();
            stream
                .write_all(format!("{CLIENT_IDENTIFICATION}\n").as_bytes())
                .unwrap();
            let mut buf = [0u8; 16];
            match stream.read(&mut buf) {
                // Clean close after the refusal.
                Ok(0) => "closed",
                // An RST because our second line was never read.
                Ok(n) => panic!("server answered {n} bytes after refusing"),
                Err(err) => {
                    assert!(
                        !matches!(
                            err.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ),
                        "the server left the socket open: {err}"
                    );
                    "reset"
                }
            }
        });

        let err = accept_keyed_listener(&listener, &token).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert!(["closed", "reset"].contains(&client.join().unwrap()));
    }

    #[test]
    fn lich_frames_split_commands_from_bare_lines() {
        assert_eq!(
            LichFrame::parse("<c>exp\n"),
            LichFrame::Command("exp".to_string())
        );
        assert_eq!(
            LichFrame::parse("<c>locker info\r\n"),
            LichFrame::Command("locker info".to_string())
        );
        assert_eq!(
            LichFrame::parse("look\n"),
            LichFrame::Bare("look".to_string())
        );
        assert_eq!(
            LichFrame::parse("_injury 2\n"),
            LichFrame::Bare("_injury 2".to_string())
        );
        // The `<c>` prefix is the only marker: a bare protocol line such as
        // `_injury 2` and a typed `look` both stay `Bare`, and an empty command
        // (`<c>`) is still `Command`.
        assert_eq!(LichFrame::parse("<c>"), LichFrame::Command(String::new()));
        assert!(LichFrame::parse("<c>").is_command());
    }

    #[test]
    fn command_frames_round_trip() {
        assert_eq!(command_frame("exp"), "<c>exp");
        assert_eq!(command_frame(""), "<c>");
        assert_eq!(bare_frame("look\n"), "look");
        assert_eq!(
            LichFrame::Command("open #123456".into()).to_wire(),
            "<c>open #123456"
        );
        assert_eq!(LichFrame::Bare("look".into()).to_wire(), "look");
        assert_eq!(LichFrame::parse("<c>exp").text(), "exp");
        assert_eq!(LichFrame::parse("look").text(), "look");
    }

    #[test]
    fn command_line_helpers_agree_with_the_enum() {
        assert!(is_command_line("<c>exp\n"));
        assert!(!is_command_line("look\n"));
        assert_eq!(strip_command_tag("<c>exp"), Some("exp"));
        assert_eq!(strip_command_tag("look"), None);
    }

    #[test]
    fn game_chunks_are_recognised_and_stripped() {
        let chunk = format!("{}<room>1234</room>", GAME_CHUNK_PREFIX as char);
        assert!(has_game_chunk_prefix(&chunk));
        assert_eq!(strip_game_chunk_prefix(&chunk), Some("<room>1234</room>"));
        let own = "--- Lich v5.22.0 is active.  Type ;help for usage info.";
        assert!(!has_game_chunk_prefix(own));
        assert_eq!(strip_game_chunk_prefix(own), None);
        assert_eq!(GAME_CHUNK_PREFIX, 0x1F);
    }

    #[test]
    fn lich_out_separates_game_chunks_from_lichs_own_output() {
        let chunk = format!(
            "{}<prompt time=\"0\">&gt;</prompt>",
            GAME_CHUNK_PREFIX as char
        );
        let game = LichOut::parse(&chunk);
        assert!(game.is_game());
        assert_eq!(game.text(), "<prompt time=\"0\">&gt;</prompt>");
        // The 0x1F byte never reaches a protocol parser.
        assert!(!game.text().contains(GAME_CHUNK_PREFIX as char));
        assert_eq!(game.into_text(), "<prompt time=\"0\">&gt;</prompt>");

        let own = LichOut::parse("<output class=\"mono\"/>Lich is running.<output class=\"\"/>");
        assert!(!own.is_game());
        assert!(own.text().starts_with("<output class=\"mono\"/>"));
    }

    #[test]
    fn accept_timeout_is_the_recovered_sixty_seconds() {
        assert_eq!(ACCEPT_TIMEOUT, Duration::from_secs(60));
    }
}
