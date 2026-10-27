//! The tiny HTTP/1.1 handshake layer (`http::request`, `read_head`,
//! `respond`).
//!
//! The original used `tokio` to read the request head off the accepted socket,
//! upgraded to a websocket (via `tokio-tungstenite`) when the request asked for
//! it, and otherwise served one asset.  Both halves are here:
//!
//! * the *bytes* are recovered from the binary's format strings and stay
//!   [`switching`]/[`respond`], and [`request`]/[`read_head`] are the same
//!   synchronous parsers as before;
//! * the *I/O* is real: [`read_head_async`] reads the head off a tokio stream
//!   and [`handshake`] answers it with exactly those bytes, while
//!   [`accept_websocket`] hands the stream to `tokio-tungstenite` for the
//!   protocol layer.  The accept key is `tungstenite`'s own
//!   (`handshake::derive_accept_key`), which is the crate the original linked.
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

// ---------------------------------------------------------------------------
// The async side (tokio + tokio-tungstenite)
// ---------------------------------------------------------------------------

/// Largest request head [`read_head_async`] buffers before giving up.
pub const MAX_HEAD: usize = 16 * 1024;

/// Read one request head off an async stream.
///
/// This is the recovered `http::read_head` (a `tokio::time::timeout` around a
/// socket reader) with the timeout left to the caller: the answer is everything
/// up to **and including** the blank line.  `Ok(None)` means the peer closed
/// before the head was complete; `Err` means the socket failed or the head grew
/// past [`MAX_HEAD`].
///
/// As in the original, bytes after the head are not kept: a websocket client
/// waits for the `101` before it sends its first frame.
pub async fn read_head_async<S>(stream: &mut S) -> std::io::Result<Option<Vec<u8>>>
where
    S: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(None);
        }
        head.extend_from_slice(&chunk[..read]);
        if let Some(end) = read_head(&head) {
            head.truncate(end);
            return Ok(Some(head));
        }
        if head.len() > MAX_HEAD {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("the request head is longer than {MAX_HEAD} bytes"),
            ));
        }
    }
}

/// Answer a websocket handshake on an accepted stream.
///
/// The recovered flow (`http::read_head` → `http::request` → `http::answer`):
/// read the head, parse it with [`request`], and — when the page asked for the
/// upgrade — write [`switching`] carrying [`accept_key`]'s answer.  Returns the
/// `Sec-WebSocket-Accept` value that was sent; `Ok(None)` means the request was
/// not a websocket handshake (a plain `GET`, so the caller serves an asset) or
/// the peer hung up first.
pub async fn handshake<S>(stream: &mut S) -> std::io::Result<Option<String>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;

    let Some(head) = read_head_async(stream).await? else {
        return Ok(None);
    };
    // The head is ASCII by definition (`request` only looks for `\r\n`, `':'`,
    // `' '` and ASCII header names), so a lossy decode cannot change the parse.
    let text = String::from_utf8_lossy(&head);
    let Some(request) = request(&text) else {
        return Ok(None);
    };
    let Some(key) = request.key.as_deref() else {
        return Ok(None);
    };
    let accept = accept_key(key);
    stream.write_all(&switching(&accept)).await?;
    stream.flush().await?;
    Ok(Some(accept))
}

/// Hand an accepted stream to `tokio-tungstenite` for the protocol layer.
///
/// The original upgraded through `tokio_tungstenite::accept_async`; that is what
/// this is, with the recovered types kept on this side (the stream itself is the
/// caller's).  The relay that drives the resulting socket is
/// [`crate::client::relay`].
pub async fn accept_websocket<S>(
    stream: S,
) -> Result<tokio_tungstenite::WebSocketStream<S>, tungstenite::Error>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    tokio_tungstenite::accept_async(stream).await
}

/// The websocket GUID from RFC 6455, as used by the original's handshake.
pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// `base64(sha1(key + WS_GUID))` — the `Sec-WebSocket-Accept` value.
///
/// The recovered handshake is exactly RFC 6455's; `tungstenite` — the crate the
/// original linked — computes it as `handshake::derive_accept_key`, which is
/// what this calls.  [`sha1`] + [`base64()`] are kept because the recovered
/// template spells the computation out and the tests pin both halves; a test
/// asserts the two agree for the RFC 6455 vector.
pub fn accept_key(key: &str) -> String {
    tungstenite::handshake::derive_accept_key(key.as_bytes())
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

/// Standard (padded) base64 — the `base64` crate the original linked.
pub fn base64(data: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)
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

    /// The hand-rolled accept key and `tungstenite`'s must agree on the RFC 6455
    /// vector (the recovered template's value).
    #[test]
    fn accept_key_agrees_with_tungstenite() {
        let mut buf = String::new();
        buf.push_str("dGhlIHNhbXBsZSBub25jZQ==");
        buf.push_str(WS_GUID);
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), base64(&sha1(buf.as_bytes())));
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            tungstenite::handshake::derive_accept_key(b"dGhlIHNhbXBsZSBub25jZQ==")
        );
    }

    #[tokio::test]
    async fn read_head_async_waits_for_the_blank_line() {
        use tokio::io::AsyncWriteExt;

        let (mut server, mut client) = tokio::io::duplex(4096);
        let head = HEAD.to_string();
        tokio::spawn(async move {
            // Two writes: the reader must not answer on a partial head.
            let (first, second) = head.split_at(head.len() / 2);
            client.write_all(first.as_bytes()).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            client.write_all(second.as_bytes()).await.unwrap();
            client.flush().await.unwrap();
        });
        let bytes = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_head_async(&mut server),
        )
        .await
        .unwrap()
        .unwrap()
        .expect("a head");
        assert_eq!(String::from_utf8(bytes).unwrap(), HEAD);
    }

    #[tokio::test]
    async fn read_head_async_reports_a_closed_peer() {
        let (mut server, client) = tokio::io::duplex(64);
        drop(client);
        assert_eq!(read_head_async(&mut server).await.unwrap(), None);
    }

    /// The recovered `101` bytes, on the wire, accepted by a real websocket
    /// client (`tokio-tungstenite`): the client verifies the accept key itself,
    /// so a wrong key or a wrong header name fails the handshake.
    #[tokio::test]
    async fn handshake_bytes_are_accepted_by_tungstenites_client() {
        use tungstenite::client::IntoClientRequest;

        let (mut server, client) = tokio::io::duplex(4096);
        let peer = tokio::spawn(async move {
            let request = "ws://localhost/".into_client_request().unwrap();
            tokio_tungstenite::client_async(request, client).await
        });
        let accept = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            handshake(&mut server),
        )
        .await
        .unwrap()
        .unwrap();
        let accept = accept.expect("the handshake was answered");
        // 28 base64 characters for the 20-byte SHA-1 digest.
        assert_eq!(accept.len(), 28, "{accept}");
        // `client_async` verifies `Sec-WebSocket-Accept` against its own key, so
        // reaching here means our recovered bytes were accepted.
        let (socket, response) = peer.await.unwrap().expect("the client accepted our 101");
        assert_eq!(response.status(), 101);
        drop(socket);
    }

    #[tokio::test]
    async fn a_plain_get_gets_no_101() {
        use tokio::io::AsyncWriteExt;

        let (mut server, mut client) = tokio::io::duplex(1024);
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        assert_eq!(handshake(&mut server).await.unwrap(), None);
    }

    /// `tokio-tungstenite`'s own handshake over a local TCP pair.
    #[tokio::test]
    async fn accept_websocket_over_a_local_tcp_pair() {
        use tungstenite::client::IntoClientRequest;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let request = "ws://localhost/".into_client_request().unwrap();
            tokio_tungstenite::client_async(request, stream).await
        });
        let (stream, _) = listener.accept().await.unwrap();
        let server = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            accept_websocket(stream),
        )
        .await
        .unwrap();
        assert!(server.is_ok(), "{:?}", server.err());
        assert!(peer.await.unwrap().is_ok());
    }
}
