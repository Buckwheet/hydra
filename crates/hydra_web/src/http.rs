//! The tiny HTTP/1.1 handshake layer (`http::request`, `read_head`,
//! `respond`).
//!
//! The original used `tokio` to read the request head off the accepted socket,
//! upgraded to a websocket (via `tokio-tungstenite`) when the request asked for
//! it, and otherwise served one asset.  This is the `std`-only model of that:
//! the request head is parsed from a `&str`, and the response bytes are built
//! exactly as the binary's format strings described them.
//!
//! Recovered literals (`__cstring`):
//!
//! ```text
//! HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n
//! HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n
//! ```

/// A parsed request head.
///
/// Recovered from `http::request` (`0x1002c2160`): the head is split on
/// `\r\n`, the first line on `' '` (method, target, version) and every other
/// line on the first `':'`; header names are compared ASCII-lowercased.
///
/// `// UNKNOWN:` the original's result struct carries one extra word in front of
/// `method` (the `Option` niche lands there); the field is not readable from the
/// code, so it is not modelled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request<'a> {
    /// `GET` (the first word of the request line).
    pub method: &'a str,
    /// The request target, e.g. `/` or `/wheel-editor.js`.
    pub target: &'a str,
    /// `HTTP/1.1` (the third word, empty when absent).
    pub version: &'a str,
    /// `Sec-WebSocket-Key`, when the client sent one.
    pub key: Option<String>,
}

impl<'a> Request<'a> {
    /// Did the client ask to upgrade?
    pub fn websocket(&self) -> bool {
        self.key.is_some()
    }
}

/// Parse an HTTP request head.
///
/// Returns `None` when the head is not a websocket handshake *and* carries no
/// key — the binary returned `None` exactly when
/// `!upgrade_websocket && sec_websocket_key.is_none()` (`0x1002c2880`).
pub fn request(head: &str) -> Option<Request<'_>> {
    let mut lines = head.split("\r\n");
    let first = lines.next()?;

    let mut words = first.split(' ');
    let method = words.next().unwrap_or("").trim();
    let target = words.next().unwrap_or("").trim();
    let version = words.next().unwrap_or("").trim();

    // Strip any fragment; the browser never sends one, but the original
    // scanned for `'#'` in the target.
    let target = match target.find('#') {
        Some(at) => &target[..at],
        None => target,
    };

    let mut upgrade_web = false;
    let mut key: Option<String> = None;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("upgrade") {
            if value.eq_ignore_ascii_case("websocket") {
                upgrade_web = true;
            }
        } else if name.eq_ignore_ascii_case("sec-websocket-key") {
            key = Some(value.to_string());
        }
    }

    if !upgrade_web && key.is_none() {
        return None;
    }
    Some(Request {
        method,
        target,
        version,
        key,
    })
}

/// Read a request head out of a buffer.
///
/// The original `http::read_head` was a `tokio::time::timeout`-wrapped reader;
/// this is the same rule without the I/O: everything up to (and excluding) the
/// blank line.  `None` means the head is not complete yet.
pub fn read_head(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
}

/// The websocket GUID from RFC 6455, as used by the original's handshake.
pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// `base64(sha1(key + WS_GUID))` — the `Sec-WebSocket-Accept` value.
pub fn accept_key(key: &str) -> String {
    let mut buf = String::with_capacity(key.len() + WS_GUID.len());
    buf.push_str(key);
    buf.push_str(WS_GUID);
    base64(&sha1(buf.as_bytes()))
}

/// The `101 Switching Protocols` handshake response, byte for byte the
/// recovered format string.
pub fn switching(accept: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    )
    .into_bytes()
}

/// A one-shot HTTP response with the binary's exact header set.
pub fn respond(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-cache\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

/// SHA-1 (RFC 3174), needed only for the websocket accept key.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let bit_len = (data.len() as u64).wrapping_mul(8);

    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Standard (padded) base64, the alphabet the original's `base64` crate used.
pub fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "GET /wheel-editor.js HTTP/1.1\r\n\
                        Host: 192.168.1.10:7681\r\n\
                        Upgrade: websocket\r\n\
                        Connection: Upgrade\r\n\
                        Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                        Sec-WebSocket-Version: 13\r\n\r\n";

    #[test]
    fn parses_the_recovered_handshake() {
        let r = request(HEAD).expect("handshake");
        assert_eq!(r.method, "GET");
        assert_eq!(r.target, "/wheel-editor.js");
        assert_eq!(r.version, "HTTP/1.1");
        assert_eq!(r.key.as_deref(), Some("dGhlIHNhbXBsZSBub25jZQ=="));
        assert!(r.websocket());
    }

    #[test]
    fn header_names_are_case_insensitive() {
        let head = "GET / HTTP/1.1\r\nupgrade: WebSocket\r\nSEC-WEBSOCKET-KEY:  abc \r\n\r\n";
        let r = request(head).unwrap();
        assert_eq!(r.key.as_deref(), Some("abc"));
    }

    #[test]
    fn a_plain_get_with_no_key_is_not_a_handshake() {
        assert!(request("GET / HTTP/1.1\r\nHost: x\r\n\r\n").is_none());
        assert!(request("").is_none());
    }

    #[test]
    fn a_key_alone_is_enough() {
        // Recovered: the original bailed only when there was neither an
        // `Upgrade: websocket` nor a key.
        let r = request("GET / HTTP/1.1\r\nSec-WebSocket-Key: k\r\n\r\n").unwrap();
        assert_eq!(r.key.as_deref(), Some("k"));
    }

    #[test]
    fn fragment_is_stripped_from_the_target() {
        let r = request("GET /#pairing=abc HTTP/1.1\r\nUpgrade: websocket\r\n\r\n").unwrap();
        assert_eq!(r.target, "/");
    }

    #[test]
    fn rfc6455_accept_vector() {
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn sha1_and_base64_vectors() {
        let d = sha1(b"abc");
        assert_eq!(
            d.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn switching_response_is_the_recovered_template() {
        let text = String::from_utf8(switching("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")).unwrap();
        assert_eq!(
            text,
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n"
        );
    }

    #[test]
    fn response_headers_match_the_binary() {
        let text = String::from_utf8(respond(200, "text/html; charset=utf-8", b"hi")).unwrap();
        assert_eq!(
            text,
            "HTTP/1.1 200\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 2\r\n\
             Cache-Control: no-cache\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\nhi"
        );
    }

    #[test]
    fn read_head_finds_the_blank_line() {
        assert_eq!(read_head(HEAD.as_bytes()), Some(HEAD.len()));
        assert_eq!(read_head(b"GET / HTTP/1.1\r\n"), None);
    }
}
