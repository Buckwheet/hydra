//! The WebSocket shim (the Lich `shim` door).
//!
//! Recovered from `hydra_net::gemstone::shim` (`0x101c64dc0` .. `0x101c66d0c`).
//! Hydra can be handed a "shim" URL instead of a direct game socket: it dials
//! `wss://<host>/shim/<character>` and then speaks the game protocol over the
//! websocket.

use std::io::{Read, Write};

/// The websocket URL of the shim for a character.
///
/// Recovered from `0x101c64dc0`: `wss://{}/shim/{}` (0x103e7ff50).
pub fn shim_request(host: &str, character: &str) -> String {
    format!("wss://{}/shim/{}", host, character)
}

/// The host to dial for a shim URL, in its `https://` form.
///
/// Recovered from `0x101c65428`: the URL is reduced to its host and presented as
/// `https://{}` (0x103e7ff61) so it can be used as the request origin.  The
/// `spcircname{}` literal (0x103e7ff6c) is the query key the shim uses to carry
/// the character name.
pub fn shim_host(url: &str) -> String {
    let rest = url
        .strip_prefix("wss://")
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("ws://"))
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    format!("https://{}", host)
}

/// The query the shim wants for a character name.
pub fn shim_query(character: &str) -> String {
    format!("spcircname{}", character)
}

/// A reader and a writer presented as one duplex stream.
///
/// The original wraps `tokio_tungstenite`'s `WebSocketStream` (which is itself a
/// `Stream + Sink`) in an adapter that splits it into the read and write halves
/// the game loop needs.  With `std` there is no async split, so the stream is
/// held as the two halves it was split into.
pub struct Duplex<R, W> {
    pub read: R,
    pub write: W,
}

impl<R: Read, W: Write> Read for Duplex<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read.read(buf)
    }
}

impl<R: Read, W: Write> Write for Duplex<R, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.write.flush()
    }
}

/// Present the two halves of a WebSocket as one stream.
///
/// Recovered from `0x101c65334`: the message stream is turned into an
/// `AsyncRead + AsyncWrite` pair by the shim adapter; here the two halves are
/// simply paired.
pub fn ws_to_io<R: Read, W: Write>(read: R, write: W) -> Duplex<R, W> {
    Duplex { read, write }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shim_url_names_the_character() {
        assert_eq!(
            shim_request("shim.example.com", "Dridarn"),
            "wss://shim.example.com/shim/Dridarn"
        );
    }

    #[test]
    fn the_shim_host_drops_the_path() {
        assert_eq!(shim_host("wss://shim.example.com/shim/Dridarn"), "https://shim.example.com");
        assert_eq!(shim_host("https://shim.example.com/x?y=1"), "https://shim.example.com");
        assert_eq!(shim_host("shim.example.com"), "https://shim.example.com");
    }

    #[test]
    fn the_character_query_matches_the_literal() {
        assert_eq!(shim_query("Dridarn"), "spcircnameDridarn");
    }

    #[test]
    fn a_duplex_reads_and_writes_its_halves() {
        let mut duplex = ws_to_io(&b"in"[..], Vec::new());
        let mut buf = [0u8; 2];
        assert_eq!(duplex.read(&mut buf).unwrap(), 2);
        assert_eq!(&buf, b"in");
        duplex.write_all(b"out").unwrap();
        assert_eq!(duplex.write, b"out");
    }
}
