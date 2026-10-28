//! Hydra's local web/phone control surface.
//!
//! Recovered from the arm64 `hydra` binary.  The original crate was a
//! hand-rolled HTTP + websocket server (tokio, tokio-tungstenite, tungstenite,
//! serde/serde_json, base64, mdns-sd, rustls) that served one page and the game
//! state behind it.  The JSON documents, the routing, the pairing tokens, the
//! file layout and the drawing model are the recovered ones, and the crates the
//! original linked are reconnected at the workspace pins: serde/serde_json for
//! every wire document, tokio for the driver, tungstenite/tokio-tungstenite for
//! the handshake and the frames, base64 for the accept key and mdns-sd behind
//! the `bonjour::Daemon` seam.  What is left as a `std` seam — rustls, getrandom,
//! `bytes`, futures-util, and the unrecovered `hydra_ui`/`hydra_board` types —
//! is listed in `NOTES.md` §5.
//!
//! Sibling crates that *were* recovered are used directly: `hydra_session`
//! (notes, the atomic save helper) and `hydra_model` (the inventory snapshot).
//! Crates that were not (`hydra_ui`, `hydra_board`) are represented by the small
//! documented stand-ins [`client::Seat`], [`highlights::Rule`] and
//! [`inventory::Act`].

#![allow(dead_code)]

pub mod bonjour;
pub mod client;
pub mod follow;
pub mod highlights;
pub mod http;
pub mod injuries;
pub mod inventory;
pub mod json;
pub mod notes;
pub mod pairing;
pub mod phone;
pub mod tasks;
pub mod trainer;
pub mod wire;
pub mod workspace;

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How long [`Server::serve_blocking`] waits for a request head before it
/// answers what it has (a client that connects and says nothing must not park
/// the accept loop).
pub const HEAD_TIMEOUT: Duration = Duration::from_secs(2);

/// How long the accept loop sleeps between polls of a non-blocking listener.
pub const POLL: Duration = Duration::from_millis(10);

/// The content type of the served page.
pub const PAGE_TYPE: &str = "text/html; charset=utf-8";

/// The UDP probe target [`lan_address`] connects to (`192.0.2.1:9`).
///
/// **ESTABLISHED** from the immediates at `0x1002cbf88` (`0x010200c0` stored at
/// `+0x12` of the `SocketAddr`, i.e. the octets `192.0.2.1` in network order)
/// and `0x1002cbf94` (port `9`).  `192.0.2.0/24` is TEST-NET-1, so no packet
/// reaches anything: connecting only makes the kernel pick the outgoing
/// interface.
pub const PROBE: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 9);

/// The best guess at the LAN address of this machine.
///
/// **ESTABLISHED** (`0x1002cbf3c`, 280 bytes):
///
/// 1. `UdpSocket::bind(0.0.0.0:0)` — any bind error is `None`;
/// 2. `connect(192.0.2.1:9)` — any connect error is `None`;
/// 3. `local_addr()` — any error is `None`;
/// 4. `SocketAddr::V4(ip)` → `Some(IpAddr::V4(ip))`, `V6` → `Some(IpAddr::V6(ip))`;
/// 5. an all-zero address is `None` (the two `cbz` checks at `0x1002cc018` and
///    `0x1002cc024`).
///
/// The result is an `Option<IpAddr>`: tag `0` = `V4`, `1` = `V6`, `2` = `None`,
/// in the 17-byte sret.
pub fn lan_address() -> Option<IpAddr> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    sock.connect(PROBE).ok()?;
    let addr = sock.local_addr().ok()?;
    let ip = addr.ip();
    if ip.is_unspecified() {
        None
    } else {
        Some(ip)
    }
}

/// One connection at a time.
///
/// `// UNKNOWN:` this 156-byte function (`0x1002cc054`) is a pure predicate over
/// a value it dereferences (`ldr x8, [x0]`, then `and x9, x8, #3` selects one of
/// four cases).  Cases 0 and 1 read a byte at `+0x10` / `+0xf` of the pointee and
/// compare it with 36; cases 2 and 3 shift the value right by 32 and bit-test it
/// against the masks `0x3060_0008_0000_0010` (bits 4, 35, 45, 46, 52, 53) and
/// `0x0000_0008_0040_204c` (bits 2, 3, 6, 13, 14, 35).  Its only caller is the
/// accept loop's `poll_fn` (`0x100155da0`), which drops the just-accepted stream
/// when this returns `false`.  Nothing in the binary names the four cases, so the
/// safest behaviour is to keep the connection: the parameter is ignored and the
/// answer is `true`.
pub fn one_connections(_value: usize) -> bool {
    true
}

/// The server: what the accept loop owns and what the page needs to exist.
///
/// The original `hydra_web::Server` held the token, the port watch channel, the
/// highlight file lock and the session handle.  Here it is the model: the storage
/// root, the pairing token, the port and whether the page is announced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {
    /// Storage root (`web.token`, `web/`, `phone/` live under it).
    pub root: std::path::PathBuf,
    /// The pairing token (`pairing::pairing`).
    pub token: String,
    /// The port the page is served on, once bound.
    pub port: Option<u16>,
    /// Did [`bonjour::Announced`] get a registration out?
    pub announced: bool,
}

impl Server {
    /// Load (or mint) the pairing token and prepare the root.
    ///
    /// Recovered shape: `serve` joined the storage root, called
    /// `pairing::pairing`, then `lan_address` and the announce step.
    pub fn new(root: impl Into<std::path::PathBuf>) -> Result<Server, String> {
        let root = root.into();
        let token = pairing::pairing(&root)?;
        Ok(Server {
            root,
            token,
            port: None,
            announced: false,
        })
    }

    /// The URL the phone should open, e.g. `http://192.168.1.10:7788/`.
    ///
    /// The recovered pieces are `http://`, `:`, `/#pairing=` (`0x103e23590`).
    pub fn url(&self) -> String {
        let host = match lan_address() {
            Some(IpAddr::V4(ip)) => ip.to_string(),
            Some(IpAddr::V6(ip)) => format!("[{ip}]"),
            None => "127.0.0.1".to_string(),
        };
        format!(
            "http://{host}:{}/#pairing={}",
            self.port.unwrap_or(0),
            self.token
        )
    }

    /// Bind the page to `port` (0 = pick one).
    ///
    /// The original spawned a tokio listener and a `watch` channel carrying the
    /// chosen port; this reports the port the model is configured for.
    pub fn listen_on(&mut self, port: u16) -> Result<u16, String> {
        let listener = self.bind(port)?;
        let port = self.port.unwrap_or(0);
        drop(listener);
        Ok(port)
    }

    /// Bind `0.0.0.0:port` (the recovered address) and record the port chosen.
    ///
    /// The listener comes back **non-blocking** so [`Server::serve_blocking`] can
    /// notice `stop` while nothing connects.  `port` 0 asks the kernel for one;
    /// the answer is then in [`Server::port`] (and in the return value).
    pub fn bind(&mut self, port: u16) -> Result<(TcpListener, u16), String> {
        let listener = TcpListener::bind(("0.0.0.0", port))
            .map_err(|e| format!("The web page cannot be served on port {port}: {e}"))?;
        let chosen = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("The web page cannot be served on port {port}: {e}"))?;
        self.port = Some(chosen);
        Ok((listener, chosen))
    }

    /// The synthetic page `serve` answers a plain `GET /` with.
    ///
    /// **INFERRED / reduced:** the recovered page is the whole phone client
    /// (websocket, inventory, injuries, …), which this crate models but does not
    /// draw.  This is the headless stand-in: it says `Hydra`, the character and
    /// the pairing token, and nothing else.  It carries no game text.
    pub fn page_html(&self, character: &str) -> String {
        let character = if character.is_empty() {
            "no character"
        } else {
            character
        };
        format!(
            "<!doctype html>\n\
             <html lang=\"en\">\n\
             <head><meta charset=\"utf-8\"><title>Hydra</title></head>\n\
             <body>\n\
             <h1>Hydra</h1>\n\
             <p>character: {character}</p>\n\
             <p>pairing: {token}</p>\n\
             <p>This page is the headless stand-in: the phone client is not served here.</p>\n\
             </body>\n\
             </html>\n",
            token = self.token
        )
    }

    /// The whole one-shot response for `GET /`: `200`, the page's content type
    /// and [`Server::page_html`].
    pub fn page(&self, character: &str) -> Vec<u8> {
        http::respond(200, PAGE_TYPE, self.page_html(character).as_bytes())
    }

    /// Accept requests on `listener` and answer each one with `page`, until
    /// `stop` is set.
    ///
    /// One request per connection: the response carries `Connection: close`
    /// (`http::respond`), so the socket is shut down and dropped after it.  A
    /// `GET /` (or `GET` with any target, which is what a browser that follows
    /// the pairing fragment sends) gets the page; anything else gets a `404`.
    /// Nothing here announces the page on the network — the mDNS path is
    /// `bonjour` and is deliberately not used.
    pub fn serve_blocking(
        &self,
        listener: TcpListener,
        page: Vec<u8>,
        stop: &Arc<AtomicBool>,
    ) -> Result<(), String> {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((socket, _peer)) => {
                    if let Err(error) = answer(socket, &page) {
                        // One bad client is not the end of the loop.
                        let _ = error;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(POLL);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(format!("the web page stopped listening: {error}")),
            }
        }
        Ok(())
    }

    /// The message logged when the page could not be announced
    /// (`0x103e235ae`): `[hydra] the web page is not announced on the network: {}`.
    pub fn not_announced(why: &str) -> String {
        format!("[hydra] the web page is not announced on the network: {why}")
    }
}

/// Answer one accepted connection with `page` (a whole `http::respond` value).
///
/// The request head is read with [`HEAD_TIMEOUT`]; whatever arrived is parsed
/// with the recovered [`http::request`] rule (a `None` there — no websocket
/// upgrade, no key — is *not* an error: a plain browser `GET` is exactly that)
/// and the page is served for a `GET`.  Anything else is a `404`.
fn answer(mut socket: TcpStream, page: &[u8]) -> std::io::Result<()> {
    let _ = socket.set_nonblocking(false);
    socket.set_read_timeout(Some(HEAD_TIMEOUT))?;
    let mut head = Vec::new();
    let mut buffer = [0u8; 1024];
    while http::read_head(&head).is_none() && head.len() < 16 * 1024 {
        match socket.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => head.extend_from_slice(&buffer[..n]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break
            }
            Err(error) => return Err(error),
        }
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let mut words = head.split(' ').take(2);
    let method = words.next().unwrap_or("").trim();
    let target = words.next().unwrap_or("").trim();
    let answer = if method.eq_ignore_ascii_case("GET") && (target == "/" || target.is_empty()) {
        page.to_vec()
    } else {
        http::respond(404, "text/plain; charset=utf-8", b"not here\n")
    };
    socket.write_all(&answer)?;
    socket.flush()?;
    let _ = socket.shutdown(std::net::Shutdown::Both);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lan_address_is_a_real_interface_or_none() {
        // Can't be asserted to be `Some` (a machine may be offline), but when it
        // is `Some` it must never be the unspecified address the function filters.
        if let Some(ip) = lan_address() {
            assert!(!ip.is_unspecified());
            match ip {
                IpAddr::V4(v4) => assert!(!v4.is_unspecified()),
                IpAddr::V6(v6) => assert!(!v6.is_unspecified()),
            }
        }
        assert_eq!(PROBE.port(), 9);
        assert_eq!(PROBE.ip().to_string(), "192.0.2.1");
    }

    #[test]
    fn one_connection_helper_is_conservative() {
        assert!(one_connections(0));
        assert!(one_connections(3));
    }

    #[test]
    fn server_url_and_port() {
        let dir = std::env::temp_dir().join(format!("hydra-web-serve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut server = Server::new(&dir).unwrap();
        assert_eq!(server.token.len(), pairing::TOKEN_LEN);
        assert!(server.url().contains("/#pairing="));
        assert!(server.url().starts_with("http://"));

        let port = server.listen_on(0).unwrap();
        assert!(port > 0);
        assert!(server.url().contains(&format!(":{port}/")));
        assert_eq!(
            Server::not_announced("no daemon"),
            "[hydra] the web page is not announced on the network: no daemon"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_page_says_hydra_the_character_and_the_token() {
        let dir = std::env::temp_dir().join(format!("hydra-web-headless-page-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let server = Server::new(&dir).unwrap();
        let html = server.page_html("Aelith");
        assert!(html.contains("Hydra"), "{html}");
        assert!(html.contains("character: Aelith"), "{html}");
        assert!(html.contains(&server.token), "{html}");
        assert!(!html.contains('\u{1f}'), "a chunk byte reached the page");

        // The whole response: 200, the page's type, the body, one connection.
        let response = server.page("Aelith");
        let text = String::from_utf8(response).unwrap();
        assert!(text.starts_with("HTTP/1.1 200\r\n"), "{text}");
        assert!(
            text.contains("Content-Type: text/html; charset=utf-8\r\n"),
            "{text}"
        );
        assert!(text.contains("Connection: close\r\n"), "{text}");
        assert!(text.ends_with(&server.page_html("Aelith")), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plain_get_gets_the_page_from_the_bound_port() {
        let dir = std::env::temp_dir().join(format!("hydra-web-served-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut server = Server::new(&dir).unwrap();
        let page = server.page("Aelith");
        let (listener, port) = server.bind(0).unwrap();
        assert!(port > 0);
        assert_eq!(server.port, Some(port));
        let stop = Arc::new(AtomicBool::new(false));
        let serving = {
            let stop = Arc::clone(&stop);
            let server = server.clone();
            std::thread::spawn(move || server.serve_blocking(listener, page, &stop))
        };

        // One plain `GET /`, the way a browser sends it.
        let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
        socket
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        socket.flush().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut got = Vec::new();
        socket.read_to_end(&mut got).unwrap();
        let text = String::from_utf8(got).unwrap();
        assert!(text.starts_with("HTTP/1.1 200\r\n"), "{text}");
        assert!(text.contains("body"), "{text}");
        assert!(text.contains("Hydra"), "{text}");
        assert!(text.contains(&server.token), "{text}");

        // Anything else is a 404, from the same loop.
        let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
        socket
            .write_all(b"GET /wheel-editor.js HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        socket.flush().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut got = Vec::new();
        socket.read_to_end(&mut got).unwrap();
        let text = String::from_utf8(got).unwrap();
        assert!(text.starts_with("HTTP/1.1 404\r\n"), "{text}");

        stop.store(true, Ordering::SeqCst);
        serving
            .join()
            .expect("the accept loop ends")
            .expect("the accept loop ends cleanly");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
