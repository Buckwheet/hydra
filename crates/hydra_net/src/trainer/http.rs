//! The trainer's HTTP side.
//!
//! Recovered from `hydra_net::trainer::http` (`0x101c64b2c` .. `0x101c64dc0`).
//! The original used `reqwest` with a cookie: it GETs the skill page and POSTs
//! the goals' form back.  Both requests are here again ([`Trainer::get_page`],
//! [`Trainer::post_goals`]); the cookie travels as the explicit `Cookie:` header
//! the recovered code built, because the workspace's `reqwest` has no feature
//! for a cookie jar.

use reqwest::header::{CONTENT_TYPE, COOKIE};

use crate::http;
use crate::trainer::goals::SkillGoals;
use crate::trainer::page::{self, page_url};

/// The content type of the goal form.
const FORM: &str = "application/x-www-form-urlencoded";

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

    /// GET the character's skill page and hand back its text.
    ///
    /// The recovered request (0x101c64b2c) carried the session cookie and
    /// followed play.net's redirects; what the page means is [`page::read_page`]'s
    /// business.
    pub async fn get_page(&self) -> Result<String, reqwest::Error> {
        self.get_page_at(&self.page_url()).await
    }

    /// [`Trainer::get_page`] against another origin.
    ///
    /// The recovered URL is a literal ([`page_url`]), so a caller that has to go
    /// somewhere else — a test, a proxy, an archived copy of the page — needs a
    /// way to say so; this is it.  The cookie still travels.
    pub async fn get_page_at(&self, url: &str) -> Result<String, reqwest::Error> {
        self.request(reqwest::Method::GET, url)
            .send()
            .await?
            .text()
            .await
    }

    /// POST the goals and hand back the trainer's answer page.
    pub async fn post_goals(&self, goals: &SkillGoals) -> Result<String, reqwest::Error> {
        self.post_goals_at(&self.page_url(), goals).await
    }

    /// [`Trainer::post_goals`] against another origin (see
    /// [`Trainer::get_page_at`]).
    pub async fn post_goals_at(
        &self,
        url: &str,
        goals: &SkillGoals,
    ) -> Result<String, reqwest::Error> {
        self.request(reqwest::Method::POST, url)
            .header(CONTENT_TYPE, FORM)
            .body(self.submit(goals))
            .send()
            .await?
            .text()
            .await
    }

    /// A request carrying the session cookie.
    ///
    /// The cookie is the whole header value, exactly as [`Trainer::cookie_header`]
    /// renders it, so the two spellings cannot drift apart.
    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        http::client().request(method, url).header(COOKIE, self.cookie.clone())
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
    use crate::http::loopback;

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

    /// The skill page, fetched with a real `reqwest` request answered by a
    /// loopback socket: no network, but the request, the cookie and the parse
    /// are real.
    #[tokio::test]
    async fn a_real_request_reads_the_skill_page() {
        let page = "<td><b class=\"normS2\">Climbing</b></td><td class=\"x\">\
                    <b class=\"normS1\">2</b>";
        let (base, server) = loopback::serve_once("200 OK", "Content-Type: text/html\r\n", page);

        let trainer = Trainer::new("acct", "Dridarn", "sid=1");
        let body = trainer
            .get_page_at(&format!("{}/gs4/play/cm/Dridarn", base))
            .await
            .unwrap();

        assert_eq!(body, page);
        assert!(landed(&body));
        assert_eq!(page::read_page(&body).unwrap().goal("Climbing"), Some(2));

        let request = server.join().unwrap();
        assert!(request.starts_with("GET /gs4/play/cm/Dridarn HTTP/1.1"), "{request}");
        assert!(request.to_lowercase().contains("cookie: sid=1"), "{request}");
    }

    /// The goal form, POSTed with a real `reqwest` request.  The loopback server
    /// hands back the request it saw, so the body can be checked from the other
    /// side of the wire.
    #[tokio::test]
    async fn a_real_request_submits_the_goal_form() {
        let (base, server) = loopback::serve_once("200 OK", "", "ok");
        let mut goals = SkillGoals::new();
        goals.aim("Climbing", 3);
        let trainer = Trainer::new("acct", "Dridarn", "sid=1");

        let answer = trainer
            .post_goals_at(&format!("{}/gs4/play/cm/Dridarn", base), &goals)
            .await
            .unwrap();
        assert_eq!(answer, "ok");

        let request = server.join().unwrap();
        assert!(request.starts_with("POST /gs4/play/cm/Dridarn HTTP/1.1"), "{request}");
        assert!(
            request
                .to_lowercase()
                .contains("content-type: application/x-www-form-urlencoded"),
            "{request}"
        );
        assert!(request.to_lowercase().contains("cookie: sid=1"), "{request}");
        assert!(request.ends_with("skill0=3&train=1"), "{request}");
    }

    /// A page that never arrives is `reqwest`'s error, not a panic.
    #[tokio::test]
    async fn a_page_that_cannot_be_reached_is_an_error() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let trainer = Trainer::new("acct", "Dridarn", "sid=1");
        let error = trainer
            .get_page_at(&format!("http://127.0.0.1:{}/gs4/play/cm/Dridarn", port))
            .await
            .unwrap_err();
        assert!(error.is_connect(), "{error}");
    }
}
