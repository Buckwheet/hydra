//! The web login's failure type.
//!
//! `crates/hydra-net/src/gemstone/weblogin/failure.rs` contributes no exported
//! symbol of its own, but its `Display` impl is called from
//! `eaccess::fallback::web_only_error` (`0x101c55ff0`).  The messages the web
//! login produces are the recovered literals
//! `unexpected response from play.net at {}` (0x103e7fb96) and
//! `could not reach play.net: {}` (0x103e7fbbe).
//!
//! INFERRED: the original type has several variants; without a symbol table
//! entry for them this reconstruction keeps the detail text only.

/// Something the web login could not do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebLoginFailure {
    /// The sentence shown to the user.
    pub detail: String,
}

impl WebLoginFailure {
    pub fn new(detail: impl Into<String>) -> WebLoginFailure {
        WebLoginFailure {
            detail: detail.into(),
        }
    }

    /// The page came back with something the scraper did not expect.
    pub fn unexpected(url: &str) -> WebLoginFailure {
        WebLoginFailure::new(format!("unexpected response from play.net at {}", url))
    }

    /// The site could not be reached at all.
    pub fn unreachable(why: impl std::fmt::Display) -> WebLoginFailure {
        WebLoginFailure::new(format!("could not reach play.net: {}", why))
    }
}

impl std::fmt::Display for WebLoginFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for WebLoginFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_messages_match_the_recovered_literals() {
        assert_eq!(
            WebLoginFailure::unexpected("https://www.play.net/gs4/play/home.asp").detail,
            "unexpected response from play.net at https://www.play.net/gs4/play/home.asp"
        );
        assert_eq!(
            WebLoginFailure::unreachable("connection refused").detail,
            "could not reach play.net: connection refused"
        );
        assert_eq!(
            WebLoginFailure::new("x").to_string(),
            "x",
            "Display is the detail verbatim"
        );
    }
}
