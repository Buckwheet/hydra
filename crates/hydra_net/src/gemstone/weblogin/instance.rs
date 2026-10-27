//! The play.net instance pages.
//!
//! Recovered from `hydra_net::gemstone::weblogin::instance` (`0x101c549d0` ..
//! `0x101c54ac4`).  The three `Instance` records are static rows in the binary
//! (`0x1044a8ca8`, `0x1044a8d00`, `0x1044a8d58`), each four `&'static str`s.

/// A GemStone instance's web identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instance {
    /// The short code a launch quotes (`GS3`, `GST`, `GSF`).
    pub code: &'static str,
    /// The instance name the game itself uses.
    pub name: &'static str,
    /// The path segment on play.net (`gs4`).
    pub web_code: &'static str,
    /// The home page on play.net (`/gs4/play/home.asp`).
    pub home: &'static str,
}

/// The three instances the original knows, in its table order.
pub const INSTANCES: [Instance; 3] = [
    Instance { code: "GS3", name: "GS4", web_code: "gs4", home: "/gs4/play/home.asp" },
    Instance { code: "GST", name: "GST", web_code: "gs4", home: "/gs4/play/play_test.asp" },
    Instance { code: "GSF", name: "GSF", web_code: "gs4", home: "/gs4/play/playf.asp" },
];

/// The instance a three-letter code names.
///
/// Recovered from `0x101c549d0`: the string must be exactly three bytes and is
/// compared against `GS3`, `GST` and `GSF` in that order; anything else is
/// `None`.
pub fn instance_for(short: &str) -> Option<&'static Instance> {
    if short.len() != 3 {
        return None;
    }
    INSTANCES.iter().find(|instance| instance.code == short)
}

impl Instance {
    /// The instance's login error page, as a path on play.net.
    ///
    /// Recovered from `0x101c54a54`: `/{}/login_error.asp` with the instance's
    /// web code (0x103e7faa4).
    pub fn error_page(&self) -> String {
        format!("/{}/login_error.asp", self.web_code)
    }

    /// The instance's "you are logged in" page, as a path on play.net.
    ///
    /// Recovered from `0x101c54a8c`: `/{}/play/home.asp` with the instance's web
    /// code (0x103e7fab9).
    pub fn okay_page(&self) -> String {
        format!("/{}/play/home.asp", self.web_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_rows_are_recovered() {
        assert_eq!(instance_for("GS3"), Some(&INSTANCES[0]));
        assert_eq!(instance_for("GST"), Some(&INSTANCES[1]));
        assert_eq!(instance_for("GSF"), Some(&INSTANCES[2]));
        assert_eq!(INSTANCES[0].home, "/gs4/play/home.asp");
        assert_eq!(INSTANCES[1].home, "/gs4/play/play_test.asp");
        assert_eq!(INSTANCES[2].home, "/gs4/play/playf.asp");
    }

    #[test]
    fn the_code_must_be_three_bytes() {
        assert_eq!(instance_for("GS"), None);
        assert_eq!(instance_for("GS4 "), None);
        assert_eq!(instance_for("gs4"), None);
        assert_eq!(instance_for(""), None);
    }

    #[test]
    fn the_pages_are_built_from_the_web_code() {
        let instance = instance_for("GS3").unwrap();
        assert_eq!(instance.error_page(), "/gs4/login_error.asp");
        assert_eq!(instance.okay_page(), "/gs4/play/home.asp");
    }
}
