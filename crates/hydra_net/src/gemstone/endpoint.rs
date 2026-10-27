//! GemStone instance endpoints: the short spellings and the hosts they mean.
//!
//! Recovered from `hydra_net::gemstone::endpoint` (`0x101c447b4` .. `0x101c44dd4`).
//! The original compared the host string and its port against a table of known
//! `gs-play` / `www.play.net` spellings and returned the canonical endpoint from
//! a static table.

/// A game endpoint: where the play connection goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// The DNS name to connect to.
    pub host: &'static str,
    /// The TCP port.
    pub port: u16,
    /// The game code the server expects (`GS4`, `GS3`, ...).
    pub game: &'static str,
}

/// The instance spellings the original knows.
///
/// ESTABLISHED for the two host literals the function compares against
/// (`www.play.net` and `gs-play.simutronics.net`, both with the module's
/// port numbers 4900 and 10121); INFERRED for the canonical rows, which live in
/// a static table that is not reachable from the symbol table.
// UNKNOWN: only the host/port compares are recovered; the canonical rows that
// the original returned are a static table.
pub const KNOWN: [Endpoint; 3] = [
    Endpoint { host: "gs-play.simutronics.net", port: 10121, game: "GS4" },
    Endpoint { host: "gs-play.simutronics.net", port: 10121, game: "GS3" },
    Endpoint { host: "www.play.net", port: 4900, game: "GS4" },
];

/// The canonical endpoint for a short instance spelling such as `GS3`.
///
/// Recovered from `0x101c44af8`: the string is matched against the table and
/// the matching [`Endpoint`] is returned.
pub fn instance(short: &str) -> Option<&'static Endpoint> {
    KNOWN.iter().find(|endpoint| endpoint.game.eq_ignore_ascii_case(short))
}

/// Translate a host/port pair the server spelled differently into a canonical
/// endpoint.
///
/// Recovered from `0x101c447b4`: the string is compared byte-for-byte against
/// `gs-play.simutronics.net` (port 10121) and `www.play.net` (port 4900) before
/// the port is compared, and the canonical endpoint is returned when both agree.
pub fn other_spelling(host: &str, port: u16) -> Option<&'static Endpoint> {
    let port = if port == 0 { 10121 } else { port };
    KNOWN.iter().find(|endpoint| {
        endpoint.port == port && endpoint.host.eq_ignore_ascii_case(host)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_spellings_resolve() {
        assert_eq!(instance("GS4").map(|e| e.host), Some("gs-play.simutronics.net"));
        assert_eq!(instance("gs3").map(|e| e.game), Some("GS3"));
        assert_eq!(instance("GST"), None);
    }

    #[test]
    fn other_spellings_need_the_port_too() {
        let endpoint = other_spelling("gs-play.simutronics.net", 10121).unwrap();
        assert_eq!(endpoint.host, "gs-play.simutronics.net");
        assert_eq!(endpoint.port, 10121);
        assert!(other_spelling("gs-play.simutronics.net", 4900).is_none());
        assert!(other_spelling("example.com", 10121).is_none());
    }

    #[test]
    fn the_web_host_is_known() {
        assert_eq!(other_spelling("www.play.net", 4900).map(|e| e.game), Some("GS4"));
    }
}
