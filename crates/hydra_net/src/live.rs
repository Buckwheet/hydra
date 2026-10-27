//! The live game connection: its socket and its certificate.
//!
//! Recovered from `hydra_net::live` (`0x101c64510` .. `0x101c64830`).  The
//! original opened a `tokio` TCP stream, set `TCP_NODELAY` and keepalive on it,
//! wrapped it in `tokio-rustls`, and handed the peer certificate to
//! [`crate::eaccess::pin`].

use std::io;
use std::net::TcpStream;

/// The message the original gives when the certificate cannot be had.
///
/// ESTABLISHED literal `could not read the server's certificate: {}`
/// (0x103e220b8).
pub fn unreadable_certificate(why: impl std::fmt::Display) -> String {
    format!("could not read the server's certificate: {}", why)
}

/// The keepalive warning the original logs when the socket will not take it.
///
/// ESTABLISHED literal prefix `[socket] WARNING: TCP keepalive could not be set (`
/// (found in the same pool as the `[socket]` messages).
pub fn keepalive_warning(why: impl std::fmt::Display) -> String {
    format!("[socket] WARNING: TCP keepalive could not be set ({})", why)
}

/// Configure the game socket the way the original does.
///
/// Recovered from `0x101c64760`: `TCP_NODELAY` is turned on so commands are not
/// held back by Nagle, and keepalive with a short idle time is asked for with a
/// warning (not an error) when the platform refuses.
///
/// DEPARTURE: `std` has no keepalive knob, so only `set_nodelay` is applied; the
/// warning text is kept for the platforms where the original still needed it.
pub fn configure_game_socket(stream: &TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)
}

/// A live connection and the certificate it presented.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveSource {
    /// The peer's DER certificate, as the TLS layer handed it over.
    pub peer_certificate: Option<Vec<u8>>,
}

impl LiveSource {
    /// A source with no certificate yet.
    pub fn new() -> LiveSource {
        LiveSource::default()
    }

    /// A source that already has the certificate the TLS layer presented.
    pub fn with_certificate(der: Vec<u8>) -> LiveSource {
        LiveSource {
            peer_certificate: Some(der),
        }
    }

    /// The peer's certificate in DER form.
    ///
    /// Recovered from `0x101c64510`: the certificate is taken off the TLS
    /// connection and presented as DER; no certificate at all is an error the
    /// caller may not ignore (the pin cannot be checked without it).
    pub fn peer_certificate_der(&self) -> Result<Vec<u8>, String> {
        match &self.peer_certificate {
            Some(der) if !der.is_empty() => Ok(der.clone()),
            Some(_) => Err(unreadable_certificate("the server presented no certificate")),
            None => Err(unreadable_certificate("the server presented no certificate")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_presented_certificate_is_handed_over() {
        let source = LiveSource::with_certificate(vec![0x30, 0x82, 0x01]);
        assert_eq!(source.peer_certificate_der().unwrap(), vec![0x30, 0x82, 0x01]);
    }

    #[test]
    fn no_certificate_is_an_error() {
        let err = LiveSource::new().peer_certificate_der().unwrap_err();
        assert_eq!(
            err,
            "could not read the server's certificate: the server presented no certificate"
        );
        let err = LiveSource::with_certificate(Vec::new())
            .peer_certificate_der()
            .unwrap_err();
        assert!(err.starts_with("could not read the server's certificate: "));
    }

    #[test]
    fn the_socket_gets_nodelay() {
        // A loopback listener gives us a real connected socket to configure.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::net::TcpStream::connect(addr).unwrap();
        let _server = listener.accept().unwrap();
        configure_game_socket(&client).unwrap();
        assert_eq!(client.nodelay().unwrap(), true);
    }

    #[test]
    fn the_keepalive_warning_matches_the_literal() {
        assert_eq!(
            keepalive_warning("not supported"),
            "[socket] WARNING: TCP keepalive could not be set (not supported)"
        );
    }
}
