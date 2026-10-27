//! The game socket: the launch key, the front-end line, then the XML stream.
//!
//! Ground truth: `SPEC_P6.md` §7 and `docs/LICH_WIRE.md` §2 — after a login
//! hands Hydra `GAMEHOST`/`GAMEPORT`/`KEY` (the `L` response,
//! [`crate::eaccess::handshake`]), Hydra dials the game and sends exactly two
//! newline-terminated lines:
//!
//! ```text
//! <launch key>\n
//! /FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n
//! ```
//!
//! Everything after that is the game's XML stream.  Those two lines are the
//! same two lines Hydra hands Lich on the keyed socket later
//! (`hydra_lich::wire`), which is why they are sent in one place.
//!
//! The original file is named in the `__cstring` source paths
//! (`crates/hydra-net/src/eaccess/game.rs`, see `NOTES.md`); it is not in the
//! symbol table, so the shape here is the protocol's, not a recovered signature.
//!
//! Nothing in this module prints anything: the key goes onto the wire and no
//! further.

use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};

use crate::eaccess::handshake::CLIENT_IDENTIFICATION;
use crate::live;

/// Dial the game socket `host:port`, send the two-line handshake and return the
/// stream, positioned at the game's XML.
pub fn connect(host: &str, port: u16, key: &str) -> io::Result<TcpStream> {
    let address = resolve(host, port)?;
    connect_to(address, key)
}

/// [`connect`] to an address that has already been resolved.
pub fn connect_to(address: SocketAddr, key: &str) -> io::Result<TcpStream> {
    let mut stream = TcpStream::connect(address)?;
    // `TCP_NODELAY` (the recovered `configure_game_socket`): commands must not
    // wait for Nagle.
    live::configure_game_socket(&stream)?;
    stream.write_all(&handshake_bytes(key))?;
    stream.flush()?;
    Ok(stream)
}

/// The bytes the game socket receives before any game data:
/// `<key>\n<client identification>\n`.
pub fn handshake_bytes(key: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(key.len() + CLIENT_IDENTIFICATION.len() + 2);
    bytes.extend_from_slice(key.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(CLIENT_IDENTIFICATION.as_bytes());
    bytes.push(b'\n');
    bytes
}

/// The first address `host:port` resolves to.
///
/// A launch names a host and a port; the reference build handed them straight
/// to its connector.  Resolving here keeps [`connect`] usable with a host name,
/// while every test dials `127.0.0.1`.
fn resolve(host: &str, port: u16) -> io::Result<SocketAddr> {
    (host, port).to_socket_addrs()?.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("{host}:{port} resolved to no address"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn the_handshake_is_the_key_then_the_front_end_line() {
        assert_eq!(
            String::from_utf8(handshake_bytes("abc")).unwrap(),
            "abc\n/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n"
        );
        // Nothing else: no trailing junk after the second line.
        assert_eq!(handshake_bytes("abc").iter().filter(|byte| **byte == b'\n').count(), 2);
    }

    #[test]
    fn a_loopback_server_sees_exactly_the_two_lines() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (socket, _peer) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket);
            let mut lines = Vec::new();
            for _ in 0..2 {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                lines.push(line.trim_end().to_string());
            }
            lines
        });

        let key = "9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d";
        let stream = connect_to(address, key).unwrap();
        // `configure_game_socket` really ran.
        assert!(stream.nodelay().unwrap());
        drop(stream);
        assert_eq!(
            server.join().unwrap(),
            vec![key.to_string(), CLIENT_IDENTIFICATION.to_string()]
        );
    }

    #[test]
    fn connect_accepts_a_loopback_host_and_port() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (socket, _peer) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket);
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            first.trim_end().to_string()
        });
        let stream = connect("127.0.0.1", address.port(), "key").unwrap();
        drop(stream);
        assert_eq!(server.join().unwrap(), "key");
    }

    #[test]
    fn a_closed_port_is_an_error_not_a_panic() {
        // A port nothing listens on: connect refuses.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(connect("127.0.0.1", port, "key").is_err());
        // A host that resolves to nothing is an error too (no name is resolved
        // to a real host: `.invalid` is reserved by RFC 2606).
        assert!(connect("eaccess.invalid", 1, "key").is_err());
    }
}
