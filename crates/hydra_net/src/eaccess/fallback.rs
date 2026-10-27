//! The web-login fallback for when eaccess cannot be reached.
//!
//! Recovered from `hydra_net::eaccess::fallback` (`0x101c57bb8` .. `0x101c57dc0`).
//! The original logged `[fallback] eaccess unreachable at stage {}; trying web
//! login` and then either carried the first failure forward alongside the second
//! one, or reported that only the web login was available.

use crate::gemstone::weblogin::failure::WebLoginFailure;

/// A launch the web login produced, in the shape the eaccess launch has.
///
/// The struct is two owned strings plus a `u16` (0x32 bytes in the sret of
/// `0x101c57d34`), which is what `fallback::from_web` copies field by field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebLaunch {
    pub host: String,
    pub character: String,
    pub port: u16,
}

/// Log line for falling back to the web login.
///
/// ESTABLISHED literal: `[fallback] eaccess unreachable at stage {}; trying web
/// login` (0x103e2213b).
pub fn unreachable_note(stage: &str) -> String {
    format!("[fallback] eaccess unreachable at stage {}; trying web login", stage)
}

/// Carry the eaccess failure forward when the web login also failed.
///
/// Recovered from `0x101c57bb8`: `{} (web-login fallback also failed: {})`
/// (0x103e7fd11), with the "already a web-login failure" flag of the first
/// failure or-ed into the result.
pub fn carry_forward(primary: &str, fallback: &WebLoginFailure) -> WebLoginFailure {
    WebLoginFailure::new(format!(
        "{} (web-login fallback also failed: {})",
        primary,
        fallback.detail
    ))
}

/// The error when the web login was the only route and it failed.
///
/// Recovered from `0x101c57c48`: the failure's `Display` text is used as the
/// detail, and the marker `[fallback] web login also failed: {}` is what the
/// original logged at 0x103e22179.
pub fn web_only_error(fallback: &WebLoginFailure) -> WebLoginFailure {
    WebLoginFailure::new(format!("[fallback] web login also failed: {}", fallback))
}

/// Present a web-login launch as an eaccess launch.
///
/// Recovered from `0x101c57d34`: the two strings and the port are copied (the
/// original clones both strings and moves the `u16`).
pub fn from_web(web: &WebLaunch) -> WebLaunch {
    WebLaunch {
        host: web.host.clone(),
        character: web.character.clone(),
        port: web.port,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_note_names_the_stage() {
        assert_eq!(
            unreachable_note("a_response"),
            "[fallback] eaccess unreachable at stage a_response; trying web login"
        );
    }

    #[test]
    fn a_second_failure_is_carried_after_the_first() {
        let web = WebLoginFailure::new("the store did not take the password");
        let combined = carry_forward("tls handshake failed", &web);
        assert_eq!(
            combined.detail,
            "tls handshake failed (web-login fallback also failed: the store did not take the password)"
        );
    }

    #[test]
    fn the_web_only_error_says_which_route_failed() {
        let web = WebLoginFailure::new("could not reach play.net: timed out");
        assert_eq!(
            web_only_error(&web).detail,
            "[fallback] web login also failed: could not reach play.net: timed out"
        );
    }

    #[test]
    fn from_web_copies_the_launch() {
        let web = WebLaunch {
            host: "gs-play.simutronics.net".into(),
            character: "Dridarn".into(),
            port: 10121,
        };
        let launch = from_web(&web);
        assert_eq!(launch, web);
    }
}
