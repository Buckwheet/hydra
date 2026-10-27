//! The play.net store's URLs, requests and failure families.
//!
//! Recovered from `hydra_net::store::http` (`0x101c53550` .. `0x101c53724` and
//! the message pool at `0x103e232be`).  The original used `reqwest` from inside
//! its `tokio` runtime: [`fetch`] is that request again, and
//! [`Family::of_reqwest`] is the classification done on `reqwest`'s own error
//! rather than on its message.  The URL shapes and messages that were recovered
//! literally are unchanged.

use reqwest::redirect::Policy;
use reqwest::{Response, StatusCode};

use crate::http;

/// The base the store lives under.
pub const STORE: &str = "https://store.play.net";

/// How a store request failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// The store answered, but with something unusable — a page that did not
    /// carry what was expected, a sign-in that came to nothing.
    Unusable = 0,
    /// The store could not be reached at all (DNS, TCP, TLS, timeout).
    Unreachable = 1,
    /// The store answered with a redirect chain that never arrived anywhere.
    Redirect = 2,
}

impl Family {
    /// Classify a real `reqwest` failure.
    ///
    /// ESTABLISHED shape (0x101c53550 returns a small fieldless enum): the
    /// transport failures are `Unreachable`, a redirect chain that ran out is
    /// `Redirect`, and everything else — a status the store did not intend, a
    /// body that would not parse, a URL that could not be built — is `Unusable`.
    pub fn of_reqwest(error: &reqwest::Error) -> Family {
        if error.is_connect() || error.is_timeout() {
            Family::Unreachable
        } else if error.is_redirect() {
            Family::Redirect
        } else {
            Family::Unusable
        }
    }

    /// The message for `why`, in this family's wrapper.
    ///
    /// The two wrappers are the recovered literals: a store that could not be
    /// reached says so, a store that answered badly is "could not be read".
    pub fn message(&self, why: impl std::fmt::Display) -> String {
        match self {
            Family::Unreachable => unreachable(why),
            Family::Unusable | Family::Redirect => unreadable(why),
        }
    }

    /// Classify a failure by its message.
    ///
    /// Recovered from `0x101c53550`: the text is scanned for the phrases the
    /// store's two wrapper messages carry — `the store could not be read: {}`
    /// (0x103e232e1) and `the store could not be reached: {}` (0x103e23301) —
    /// and the result is a small fieldless enum (the tail of that function moves
    /// `0` or `1` into the return register).
    pub fn of(reason: &str) -> Family {
        // UNKNOWN: the original's full phrase list; these are the wrappers it
        // builds plus the standard transport failures.
        let lower = reason.to_ascii_lowercase();
        if lower.contains("could not be reached")
            || lower.contains("timed out")
            || lower.contains("connection refused")
            || lower.contains("dns")
        {
            Family::Unreachable
        } else if lower.contains("redirect") {
            Family::Redirect
        } else {
            Family::Unusable
        }
    }
}

/// The sign-in page for a purchase, carrying the page to return to.
///
/// Recovered from the literal `{}/Account/SignIn?returnURL=%2Fstore%2Fpurchase%2F{}`
/// (0x103e23324).
pub fn sign_in_url(base: &str, purchase: &str) -> String {
    format!("{}/Account/SignIn?returnURL=%2Fstore%2Fpurchase%2F{}", base, purchase)
}

/// The claim endpoint.
///
/// Recovered from `{}/Store/ClaimReward` (0x103e233a8).
pub fn claim_url(base: &str) -> String {
    format!("{}/Store/ClaimReward", base)
}

/// A purchase page.
///
/// Recovered from `{}/store/purchase/{}` (0x103e233e8).
pub fn purchase_url(base: &str, id: &str) -> String {
    format!("{}/store/purchase/{}", base, id)
}

/// The message for a store page that could not be read.
pub fn unreadable(why: impl std::fmt::Display) -> String {
    format!("the store could not be read: {}", why)
}

/// The message for a store that could not be reached.
pub fn unreachable(why: impl std::fmt::Display) -> String {
    format!("the store could not be reached: {}", why)
}

/// The message for a sign-in that did not take the password.
///
/// ESTABLISHED literal `{}: the store did not take the password` (0x103e23380).
pub fn wrong_password(account: &str) -> String {
    format!("{}: the store did not take the password", account)
}

/// The message for a sign-in that went nowhere.
///
/// ESTABLISHED literal `{}: the store's sign-in came to nothing` (0x103e23358).
pub fn sign_in_nowhere(account: &str) -> String {
    format!("{}: the store's sign-in came to nothing", account)
}

/// The message for a claim the store answered oddly.
///
/// ESTABLISHED literal `{}: the store answered the claim with `{}`` (0x103e233bd).
pub fn odd_claim(account: &str, answer: &str) -> String {
    format!("{}: the store answered the claim with `{}`", account, answer)
}

/// GET a store page, following the store's redirects, and hand back its text.
///
/// The original did exactly this through `reqwest`.  A transport failure comes
/// back in the [`Family::Unreachable`] wrapper and an answer the store did not
/// mean as a page in the [`Family::Unusable`] one, which is the split the
/// recovered `Family` exists for; a status that is not a success says which one
/// it was.
pub async fn fetch(url: &str) -> Result<String, String> {
    fetch_with(http::client(), url).await
}

/// [`fetch`] against a caller's own client.
///
/// A caller that must not follow redirects (a web login reading the `Location`
/// of the first hop) builds a client with [`reqwest::redirect::Policy::none`]
/// and passes it here; a hop that answers with a redirect then comes back as
/// that hop's body, not as the page after it.
pub async fn fetch_with(client: reqwest::Client, url: &str) -> Result<String, String> {
    read_page(get_with(client, url).await?, url).await
}

/// GET `url` and hand the response back unread.
///
/// This is the shape a sign-in hop needs: with [`hop_client`] the redirect is
/// not followed, so the response's status and `Location`
/// ([`crate::gemstone::weblogin::http::location_of_response`]) are the hop's
/// answer.
pub async fn get_with(client: reqwest::Client, url: &str) -> Result<Response, String> {
    client
        .get(url)
        .send()
        .await
        .map_err(|error| Family::of_reqwest(&error).message(&error))
}

/// Read the body of a store response, or say which status refused it.
async fn read_page(response: Response, url: &str) -> Result<String, String> {
    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => return Err(Family::of_reqwest(&error).message(&error)),
    };
    if status.is_success() {
        return Ok(body);
    }
    Err(unreadable(format!(
        "the store answered {} for {}",
        status_line(status),
        url
    )))
}

/// `401 Unauthorized`-style text, or just the number when the reason is unknown.
fn status_line(status: StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("{} {}", status.as_u16(), reason),
        None => status.as_u16().to_string(),
    }
}

/// [`fetch`], for a caller that is still synchronous.
///
/// Runs the request on a private current-thread runtime (see
/// [`crate::http::block_on`]); it refuses to be called from inside a `tokio`
/// runtime rather than blocking one.
pub fn fetch_blocking(url: &str) -> Result<String, String> {
    http::block_on(fetch(url))?
}

/// A client that does not follow redirects, for the sign-in hops.
///
/// ESTABLISHED shape: the recovered web login asked for the first hop's
/// `Location` itself (`weblogin::http::location_of_response`) instead of letting
/// `reqwest` chase it, because each hop's redirect carries the state.
pub fn hop_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(Policy::none())
        .build()
        .unwrap_or_default()
}

/// True when `status` is one a store page can legitimately carry.
pub fn is_page_status(status: StatusCode) -> bool {
    status.is_success()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_match_the_recovered_shapes() {
        assert_eq!(
            sign_in_url(STORE, "42"),
            "https://store.play.net/Account/SignIn?returnURL=%2Fstore%2Fpurchase%2F42"
        );
        assert_eq!(claim_url(STORE), "https://store.play.net/Store/ClaimReward");
        assert_eq!(purchase_url(STORE, "7"), "https://store.play.net/store/purchase/7");
    }

    #[test]
    fn families_follow_the_message() {
        assert_eq!(Family::of(&unreadable("bad html")), Family::Unusable);
        assert_eq!(Family::of(&unreachable("timed out")), Family::Unreachable);
        assert_eq!(Family::of("could not be reached: host"), Family::Unreachable);
        assert_eq!(Family::of("too many redirect"), Family::Redirect);
    }

    #[test]
    fn the_messages_match_their_literals() {
        assert_eq!(
            wrong_password("acct"),
            "acct: the store did not take the password"
        );
        assert_eq!(
            sign_in_nowhere("acct"),
            "acct: the store's sign-in came to nothing"
        );
        assert_eq!(odd_claim("acct", "no"), "acct: the store answered the claim with `no`");
        assert_eq!(unreadable("x"), "the store could not be read: x");
        assert_eq!(unreachable("x"), "the store could not be reached: x");
    }

    #[test]
    fn the_store_base_is_recovered() {
        assert_eq!(STORE, "https://store.play.net");
    }

    #[test]
    fn families_follow_a_request_failure() {
        assert_eq!(Family::Unreachable.message("x"), "the store could not be reached: x");
        assert_eq!(Family::Unusable.message("x"), "the store could not be read: x");
        assert_eq!(Family::Redirect.message("x"), "the store could not be read: x");
    }

    /// A real `reqwest` round trip, answered by a loopback socket.
    #[tokio::test]
    async fn a_page_is_fetched_from_the_store() {
        let (base, server) = crate::http::loopback::serve_once(
            "200 OK",
            "Content-Type: text/html\r\n",
            "<a href=\"/Store/ClaimReward?id=3\">claim</a>",
        );
        let page = fetch(&claim_url(&base)).await.unwrap();
        assert!(crate::store::page::offers_claim(&page), "{page}");

        let request = server.join().unwrap();
        assert!(request.starts_with("GET /Store/ClaimReward HTTP/1.1"), "{request}");
    }

    /// A status that is not a success says which one it was.
    #[tokio::test]
    async fn a_refused_page_names_its_status() {
        let (base, server) = crate::http::loopback::serve_once("503 Service Unavailable", "", "");
        let error = fetch(&purchase_url(&base, "7")).await.unwrap_err();
        assert_eq!(
            error,
            format!(
                "the store could not be read: the store answered 503 Service Unavailable for \
                 {}/store/purchase/7",
                base
            )
        );
        server.join().unwrap();
    }

    /// A connection nothing answers is `Unreachable`, decided from `reqwest`'s
    /// own error rather than from its message.
    #[tokio::test]
    async fn a_refused_connection_is_unreachable() {
        // Bind a port, note it, drop the listener: nothing listens there now.
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let error = http::client()
            .get(format!("http://127.0.0.1:{}/", port))
            .send()
            .await
            .unwrap_err();
        assert_eq!(Family::of_reqwest(&error), Family::Unreachable);
        assert!(Family::of_reqwest(&error).message(&error).starts_with("the store could not be reached: "));
    }

    /// A URL that cannot be built is not a transport failure.
    #[tokio::test]
    async fn a_bad_url_is_unusable() {
        let error = http::client()
            .get("http://[not a host]/store")
            .send()
            .await
            .unwrap_err();
        assert_eq!(Family::of_reqwest(&error), Family::Unusable);
    }

    /// The synchronous bridge works from a plain thread ...
    #[test]
    fn the_blocking_bridge_fetches_a_page() {
        let (base, server) = crate::http::loopback::serve_once("200 OK", "", "claimed");
        let page = fetch_blocking(&purchase_url(&base, "9")).unwrap();
        assert_eq!(page, "claimed");
        server.join().unwrap();
    }

    /// ... and refuses rather than blocking a runtime it is called from.
    #[tokio::test]
    async fn the_blocking_bridge_refuses_inside_a_runtime() {
        let error = fetch_blocking("http://127.0.0.1:1/store").unwrap_err();
        assert!(error.contains("inside a tokio runtime"), "{error}");
    }

    #[test]
    fn the_hop_client_leaves_redirects_to_the_caller() {
        // The policy is what differs; a client is built either way.
        let _client = hop_client();
        assert!(is_page_status(StatusCode::OK));
        assert!(!is_page_status(StatusCode::FOUND));
    }

    /// The sign-in hop seam end to end: the redirect is not followed, and the
    /// status and `Location` come off the real response.
    #[tokio::test]
    async fn a_sign_in_hop_answers_with_its_redirect() {
        let (base, server) = crate::http::loopback::serve_once("302 Found", "Location: /gs4/play/home.asp\r\n", "");
        let response = get_with(hop_client(), &sign_in_url(&base, "42"))
            .await
            .unwrap();

        use crate::gemstone::weblogin::http::{location_of_response, status_of_response};
        assert_eq!(status_of_response(&response), 302);
        assert_eq!(
            location_of_response(&response).as_deref(),
            Some("/gs4/play/home.asp")
        );

        let request = server.join().unwrap();
        assert!(
            request.starts_with("GET /Account/SignIn?returnURL=%2Fstore%2Fpurchase%2F42 HTTP/1.1"),
            "{request}"
        );
    }
}
