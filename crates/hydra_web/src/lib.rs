//! Hydra's local web/phone control surface.
//!
//! Recovered from the arm64 `hydra` binary.  The original crate was a
//! hand-rolled HTTP + websocket server (tokio, tokio-tungstenite, serde_json,
//! mdns-sd, rustls) that served one page and the game state behind it.  This
//! reconstruction is `std`-only: the JSON documents, the routing, the pairing
//! tokens, the file layout and the drawing model are the recovered ones, and
//! every piece that was I/O or a runtime in the original is a documented seam
//! (see `NOTES.md`).
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

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

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
        let listener = std::net::TcpListener::bind(("0.0.0.0", port))
            .map_err(|e| format!("The web page cannot be served on port {port}: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        self.port = Some(port);
        Ok(port)
    }

    /// The message logged when the page could not be announced
    /// (`0x103e235ae`): `[hydra] the web page is not announced on the network: {}`.
    pub fn not_announced(why: &str) -> String {
        format!("[hydra] the web page is not announced on the network: {why}")
    }
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
}
