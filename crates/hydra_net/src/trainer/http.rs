//! The trainer's HTTP side.
//!
//! Recovered from `hydra_net::trainer::http` (`0x101c64b2c` .. `0x101c64dc0`).
//! The original used `reqwest` with a cookie jar; this keeps the shape of the
//! thing — the base URL, the cookie and the request that submits the form.

use crate::trainer::goals::SkillGoals;
use crate::trainer::page::{self, page_url};

/// A trainer session: who is training, and where the pages live.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trainer {
    /// The account the trainer pages belong to.
    pub account: String,
    /// The character whose skills are being trained.
    pub character: String,
    /// The cookie the store/trainer set at sign-in.
    pub cookie: String,
    /// The base the pages live under.
    pub base: String,
}

impl Trainer {
    /// A trainer session for a character.
    ///
    /// Recovered from `0x101c64c30`: the struct is built from the account, the
    /// character and the cookie; the base is the play.net root.
    pub fn new(account: &str, character: &str, cookie: &str) -> Trainer {
        Trainer {
            account: account.to_string(),
            character: character.to_string(),
            cookie: cookie.to_string(),
            base: "https://www.play.net".to_string(),
        }
    }

    /// The page to GET to see the character's skills.
    pub fn page_url(&self) -> String {
        page_url(&self.character)
    }

    /// The `Cookie` header for the request.
    pub fn cookie_header(&self) -> String {
        format!("Cookie: {}", self.cookie)
    }

    /// The body that submits a set of goals.
    pub fn submit(&self, goals: &SkillGoals) -> String {
        let mut body = page::form(goals);
        body.push_str("&train=1");
        body
    }
}

/// True when the page that came back is the trainer answering, not an error.
///
/// Recovered from `0x101c64b2c`: the answer page is accepted when
/// [`page::resolved`] says it is the trainer's own page.
pub fn landed(page: &str) -> bool {
    page::resolved(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trainer_knows_its_page_and_cookie() {
        let trainer = Trainer::new("acct", "Dridarn", "sid=1");
        assert_eq!(trainer.page_url(), "https://www.play.net/gs4/play/cm/Dridarn");
        assert_eq!(trainer.cookie_header(), "Cookie: sid=1");
    }

    #[test]
    fn a_submission_carries_the_goal_fields() {
        let trainer = Trainer::new("acct", "Dridarn", "sid=1");
        let mut goals = SkillGoals::new();
        goals.aim("Climbing", 3);
        let body = trainer.submit(&goals);
        assert!(body.contains("skill0=3"), "{body}");
        assert!(body.ends_with("&train=1"), "{body}");
    }

    #[test]
    fn an_error_page_did_not_land() {
        assert!(!landed("<html><h1>Error</h1></html>"));
        assert!(landed("<b class=\"normS2\">Climbing</b>"));
    }
}
