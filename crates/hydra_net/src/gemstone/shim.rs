//! The WebSocket shim (the Lich `shim` door).
//!
//! Recovered from `hydra_net::gemstone::shim` (`0x101c64dc0` .. `0x101c66d0c`).
//! Hydra can be handed a "shim" URL instead of a direct game socket: it dials
//! `wss://<host>/shim/<character>` and then speaks the game protocol over the
//! websocket.
//!
//! The original wrapped `tokio-tungstenite`'s `WebSocketStream` and turned it
//! into the pair of halves its game loop wanted ([`ws_to_io`]); that is what this
//! does again — [`shim_over`] hands back the real async `WebSocketStream` over a
//! connection the caller has already established (the original's was TLS through
//! `tokio-rustls`), and [`shim_plain`] dials a plaintext `ws://` shim.  The
//! `Duplex` type below is kept as the synchronous compatibility shape for the
//! rest of this workspace, which is still std-only.
//!
//! `wss://` cannot be dialled here: TLS needs a `tokio-tungstenite` feature
//! (`native-tls`/rustls) that is not in the workspace pins, and the original used
//! `tokio-rustls`, which is deliberately not pinned either (see NOTES.md).  A
//! caller that has a TLS stream therefore goes through [`shim_over`], which asks
//! nothing about schemes.

use std::io::{Read, Write};

pub use tokio_tungstenite::tungstenite::{Error as WsError, Message};
pub use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

/// The websocket URL of the shim for a character.
///
/// Recovered from `0x101c64dc0`: `wss://{}/shim/{}` (0x103e7ff50).
pub fn shim_request(host: &str, character: &str) -> String {
    format!("wss://{}/shim/{}", host, character)
}

/// The same URL over plaintext, for a shim that is not behind TLS.
///
/// The recovered literal is `wss://`; only the scheme differs here, so the host
/// and path keep the recovered shape.
pub fn shim_request_plain(host: &str, character: &str) -> String {
    shim_request(host, character).replacen("wss://", "ws://", 1)
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

/// A dialled, plaintext shim websocket.
pub type Shimmer = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Speak the shim protocol over a connection the caller already has.
///
/// This is the recovering call (0x101c65334): the original performed the
/// websocket handshake over the TLS stream `tokio-rustls` had handed it, and
/// nothing here cares which scheme the URL names — the stream is already
/// connected, which is exactly the seam the original's `ws_to_io` sat on.
pub async fn shim_over<S>(
    stream: S,
    host: &str,
    character: &str,
) -> Result<WebSocketStream<S>, WsError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (socket, _response) =
        tokio_tungstenite::client_async(shim_request(host, character), stream).await?;
    Ok(socket)
}

/// Dial a plaintext `ws://` shim and hand back the live websocket.
///
/// A shim on the loopback interface (`ws://localhost:4200/shim/…`) needs no TLS;
/// anything reached over the network goes through [`shim_over`] with a TLS
/// stream.  Frames are sent and received through the returned
/// `WebSocketStream`'s `Sink`/`Stream` implementations, which need
/// `futures-util` — not a workspace pin, so the pump is the caller's (see
/// NOTES.md).
pub async fn shim_plain(host: &str, character: &str) -> Result<Shimmer, WsError> {
    let (socket, _response) =
        tokio_tungstenite::connect_async(shim_request_plain(host, character)).await?;
    Ok(socket)
}

/// A reader and a writer presented as one duplex stream.
///
/// The original wraps `tokio_tungstenite`'s `WebSocketStream` (which is itself a
/// `Stream + Sink`) in an adapter that splits it into the read and write halves
/// the game loop needs.  [`shim_over`] is that websocket again for async callers;
/// this is the synchronous pair the std-only siblings in this workspace use.
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
    use std::sync::{Arc, Mutex};

    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    #[test]
    fn the_shim_url_names_the_character() {
        assert_eq!(
            shim_request("shim.example.com", "Dridarn"),
            "wss://shim.example.com/shim/Dridarn"
        );
        // Only the scheme differs in the plaintext spelling.
        assert_eq!(
            shim_request_plain("shim.example.com", "Dridarn"),
            "ws://shim.example.com/shim/Dridarn"
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

    #[test]
    fn everything_the_async_docs_need_is_re_exported() {
        // The crate the recovery names, without a second dependency on it.
        let frame = Message::text("look");
        assert!(frame.is_text());
    }

    /// The shim's own handshake, over a real socket, against a real
    /// `tokio-tungstenite` server: no network, but every byte is a websocket.
    #[tokio::test]
    async fn the_shim_hands_the_character_to_the_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(String::new()));
        let server_seen = Arc::clone(&seen);

        let server = tokio::spawn(async move {
            let (tcp, _peer) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                tcp,
                move |request: &Request, response: Response| {
                    *server_seen.lock().unwrap() = request.uri().path().to_string();
                    Ok(response)
                },
            )
            .await
            .unwrap();
            socket.close(None).await.ok();
        });

        let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let mut socket = shim_over(tcp, &format!("127.0.0.1:{}", port), "Dridarn")
            .await
            .unwrap();

        // The path the server's handshake saw is the recovered `/shim/{}`.
        assert_eq!(*seen.lock().unwrap(), "/shim/Dridarn");
        socket.close(None).await.ok();
        server.await.unwrap();
    }

    /// The plaintext dial (`ws://`) completes against a real server half.
    #[tokio::test]
    async fn a_plain_dial_completes_the_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(String::new()));
        let server_seen = Arc::clone(&seen);

        let server = tokio::spawn(async move {
            let (tcp, _peer) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                tcp,
                move |request: &Request, response: Response| {
                    *server_seen.lock().unwrap() = request.uri().path().to_string();
                    Ok(response)
                },
            )
            .await
            .unwrap();
            socket.close(None).await.ok();
        });

        let mut socket = shim_plain(&format!("127.0.0.1:{}", port), "Dridarn")
            .await
            .unwrap();

        assert_eq!(*seen.lock().unwrap(), "/shim/Dridarn");
        socket.close(None).await.ok();
        server.await.unwrap();
    }

    /// A shim that is not listening is an error, not a hang.
    #[tokio::test]
    async fn a_shim_that_is_not_there_is_an_error() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let error = shim_plain(&format!("127.0.0.1:{}", port), "Dridarn")
            .await
            .unwrap_err();
        assert!(matches!(error, WsError::Io(_)), "{error}");
    }
}
