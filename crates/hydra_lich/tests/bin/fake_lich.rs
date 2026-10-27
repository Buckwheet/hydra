//! `fake_lich` — a synthetic Lich stand-in for loopback integration tests.
//!
//! It exists so `tests/loopback.rs` can exercise the observed handshake
//! (`docs/LICH_WIRE.md`) end to end with no Ruby, no real Lich install and no
//! game connection.  Everything it sends is invented; there is no captured
//! game text here.  It deliberately does not depend on the crate under test
//! (the wire literals are re-stated, not imported), so a bug in `hydra_lich`
//! cannot make the fake agree with itself.
//!
//! # CLI contract (exactly what `relay::build_command` produces)
//!
//! ```text
//! fake_lich -e <ruby prelude> -e $0=ARGV.shift <lich.rbw> \
//!           --pipe --stormfront --sentinel -g 127.0.0.1:<port> [extra...]
//! ```
//!
//! * `-e <code>` — the two Ruby snippets Hydra passes; the flag *and* its value
//!   are skipped, and the `lich.rbw` path is ignored along with the
//!   `--pipe/--stormfront/--sentinel` flags.
//! * `-g 127.0.0.1:<port>` (or `-g127.0.0.1:<port>`) — the loopback address
//!   Lich connects back to.  Only loopback hosts are accepted: this helper
//!   never talks to any other host.
//!
//! Exit codes: `0` when the socket closes, `1` when the socket cannot be
//! reached/written, `2` when argv has no usable `-g` or stdin has no handshake.
//!
//! # Environment (negative-test seams)
//!
//! * `FAKE_LICH_TOKEN` — send this token on the socket instead of the one read
//!   from stdin (the wrong-token scenario).
//! * `FAKE_LICH_CLIENT` — send this client-identification line instead (the
//!   wrong-client-line scenario).
//! * `FAKE_LICH_PID_FILE` — write this process's pid there first thing, so a
//!   test can check the child was reaped.
//!
//! # Behaviour (the Lich half of the observed protocol)
//!
//! 1. Read exactly two lines from stdin: the token and the client line.
//! 2. Connect to the `-g` port and send `<token>\n<client line>\n` as the first
//!    bytes of the stream, then one synthetic Lich-originated command,
//!    `<c>exp\n`.
//! 3. Forward every further stdin line to the socket as a *bare* line
//!    (`look\n`) — the observed "Lich takes typing" path.  A stdin EOF only
//!    stops that forwarding; the socket decides when the process ends.
//! 4. Echo every line received on the socket to stdout as front-end output,
//!    prefixed with the 0x1F chunk byte and ended `\r\n`.  Its own banner is
//!    unprefixed, like Lich's.
//! 5. Exit as soon as the socket closes (or a refused handshake resets it), so
//!    a test never waits on a parked child.

use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::exit;
use std::thread;

/// The byte Lich puts in front of every forwarded game chunk on its stdout
/// (US, 0x1F).  Lich's own lines carry no prefix.
const GAME_CHUNK_PREFIX: u8 = 0x1F;

/// The one synthetic command the fake Lich originates right after the
/// handshake, so a test can tell `<c>` commands from bare lines.
const LICH_COMMAND: &str = "<c>exp";

/// The unprefixed "Lich's own output" line written before the handshake.
const BANNER: &str = "--- fake_lich: synthetic Lich stand-in (no game, no Ruby)";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let address = match game_address(&args) {
        Some(address) => address,
        None => {
            eprintln!("fake_lich: no -g <host:port> argument in argv: {args:?}");
            exit(2);
        }
    };
    let port = match loopback_port(&address) {
        Some(port) => port,
        None => {
            eprintln!("fake_lich: -g value is not 127.0.0.1:<port>: {address}");
            exit(2);
        }
    };

    if let Ok(path) = env::var("FAKE_LICH_PID_FILE") {
        let _ = std::fs::write(&path, format!("{}\n", std::process::id()));
    }

    say_own(BANNER);

    // Step 1: the two stdin handshake lines.  The same reader is handed to the
    // forwarding thread below, so nothing read here is lost.
    let mut stdin = BufReader::new(io::stdin());
    let token = read_stdin_line(&mut stdin);
    let client = read_stdin_line(&mut stdin);
    if token.is_empty() {
        eprintln!("fake_lich: stdin ended before the two handshake lines");
        exit(2);
    }

    // Negative-test seams: send something else than what stdin carried.
    let socket_token = env::var("FAKE_LICH_TOKEN").unwrap_or(token);
    let socket_client = env::var("FAKE_LICH_CLIENT").unwrap_or(client);

    // Step 2: the keyed accept, plus one `<c>` command.
    let socket = match TcpStream::connect(("127.0.0.1", port)) {
        Ok(socket) => socket,
        Err(err) => {
            eprintln!("fake_lich: cannot connect to 127.0.0.1:{port}: {err}");
            exit(1);
        }
    };
    let _ = socket.set_nodelay(true);
    let handshake = format!("{socket_token}\n{socket_client}\n{LICH_COMMAND}\n");
    if let Err(err) = (&socket).write_all(handshake.as_bytes()).and_then(|()| (&socket).flush()) {
        eprintln!("fake_lich: handshake write failed: {err}");
        exit(1);
    }
    let mut writer = match socket.try_clone() {
        Ok(writer) => writer,
        Err(err) => {
            eprintln!("fake_lich: cannot clone the socket: {err}");
            exit(1);
        }
    };

    // Step 3: typed input reaches Lich on stdin and leaves as a bare line.
    thread::spawn(move || {
        let mut stdin = stdin;
        let mut line = String::new();
        loop {
            line.clear();
            match stdin.read_line(&mut line) {
                Ok(0) | Err(_) => break, // stdin closed: stop forwarding
                Ok(_) => {}
            }
            let bare = line.trim_end_matches(['\n', '\r']);
            let framed = format!("{bare}\n");
            if writer
                .write_all(framed.as_bytes())
                .and_then(|()| writer.flush())
                .is_err()
            {
                break;
            }
        }
    });

    // Step 4: echo the socket to stdout, one 0x1F-prefixed chunk per line.
    let mut socket = BufReader::new(socket);
    let mut out = io::stdout();
    let mut line = Vec::new();
    loop {
        line.clear();
        match socket.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break, // Hydra closed, or our handshake was refused
            Ok(_) => {}
        }
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        let mut chunk = Vec::with_capacity(line.len() + 3);
        chunk.push(GAME_CHUNK_PREFIX);
        chunk.extend_from_slice(&line);
        chunk.extend_from_slice(b"\r\n");
        if out.write_all(&chunk).and_then(|()| out.flush()).is_err() {
            break;
        }
    }
    // Leaving `main` ends the process even if the stdin forwarder is still
    // blocked on its read: nothing here may keep a test waiting.
}

/// The `-g` value from Hydra's argv, skipping the two `-e <code>` pairs.
fn game_address(args: &[String]) -> Option<String> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            // `ruby -e <code> -e <code> lich.rbw …`: flag and code ignored.
            // The `lich.rbw` path and the `--pipe` style flags simply fall
            // through, exactly like a program that never reads them.
            "-e" => {
                args.next();
            }
            "-g" => return args.next().cloned(),
            other => {
                if let Some(attached) = other.strip_prefix("-g") {
                    if !attached.is_empty() {
                        return Some(attached.to_string());
                    }
                }
            }
        }
    }
    None
}

/// The port of a loopback `host:port`; `None` for anything else (this helper
/// never connects off-box).
fn loopback_port(address: &str) -> Option<u16> {
    let (host, port) = address.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host != "127.0.0.1" && host != "localhost" {
        return None;
    }
    port.parse().ok()
}

/// One CRLF/LF-stripped line from stdin; empty at EOF.
fn read_stdin_line<R: BufRead>(reader: &mut R) -> String {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) | Err(_) => String::new(),
        Ok(_) => line.trim_end_matches(['\n', '\r']).to_string(),
    }
}

/// Write one of Lich's own (unprefixed) stdout lines.
fn say_own(message: &str) {
    let mut out = io::stdout();
    let _ = out
        .write_all(message.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .and_then(|()| out.flush());
}
