//! Scraping the play.net login pages.
//!
//! Recovered from `hydra_net::gemstone::weblogin::scrape` (`0x101c44dd4` ..
//! `0x101c48c20`).  The web login follows redirects until it lands on a page it
//! recognises, collects the character codes from the account page, and finally
//! reads `host`/`port`/`key` off the launch URL.

use crate::gemstone::weblogin::failure::WebLoginFailure;
use crate::gemstone::weblogin::instance::Instance;

/// The `id="W_` prefix every character row on the account page carries.
const CHAR_ID: &str = "id=\"W_";

/// A launch the web login found in a URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebLaunch {
    pub host: String,
    pub port: u16,
    pub key: String,
}

/// Where a login redirect sends the client next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginRedirect {
    /// The instance's "you are in" page.
    Ok,
    /// The instance's login error page.
    Error,
    /// The sign-in page: the password still has to be given.
    SignIn,
    /// Somewhere that is not part of the login at all.
    Elsewhere,
}

/// Classify a redirect `Location` found during the web login.
///
/// Recovered from `0x101c4611c`: the location is compared with the instance's
/// error and ok pages (`/{}/login_error.asp`, `/{}/play/home.asp`), and
/// `signin_needed.asp` sends the flow back to the password form.
pub fn classify_login_redirect(location: &str, instance: &Instance) -> LoginRedirect {
    let location = location.trim();
    if location.ends_with(&instance.error_page()) {
        LoginRedirect::Error
    } else if location.ends_with(&instance.okay_page()) {
        LoginRedirect::Ok
    } else if location.contains("signin_needed.asp") {
        LoginRedirect::SignIn
    } else {
        LoginRedirect::Elsewhere
    }
}

/// True when the page says the account has no subscription for this instance.
///
/// Recovered from `0x101c45ea4`: the page is searched for `/play/subscription`.
pub fn is_subscription_needed(page: &str) -> bool {
    page.contains("/play/subscription")
}

/// The character codes and names on the account page.
///
/// Recovered from `0x101c458b8`: every `id="W_` in the page marks a character
/// row; the code runs to the closing quote, and the row's `<label for="CODE">`
/// carries the name.  `wanted` filters by name (case insensitively) when it is
/// not empty.  Rows without a label are skipped.
pub fn find_char_code(page: &str, wanted: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = page[from..].find(CHAR_ID) {
        let start = from + at + CHAR_ID.len() - 2; // keep the `W_`
        let rest = &page[start..];
        let end = match rest.find('"') {
            Some(end) => end,
            None => break,
        };
        let code = &rest[..end];
        from = start + end;
        let marker = format!("<label for=\"{}\">", code);
        let name = match page[from..].find(&marker) {
            Some(label_at) => {
                let text_start = from + label_at + marker.len();
                let text_end = page[text_start..]
                    .find('<')
                    .map(|e| text_start + e)
                    .unwrap_or(page.len());
                let name = page[text_start..text_end].trim();
                from = text_end;
                name
            }
            None => continue,
        };
        if wanted.is_empty() || name.eq_ignore_ascii_case(wanted) {
            found.push((code.to_string(), name.to_string()));
        }
    }
    found
}

/// The next URL to fetch, from a page or a redirect location.
///
/// Recovered from `0x101c46374`: a `https://<host>/<path>` location is followed,
/// but a redirect that leaves play.net is refused with
/// `character selection: untrusted redirect` (0x103bf9734).
pub fn next_hop(location: &str) -> Result<String, WebLoginFailure> {
    let location = location.trim();
    if let Some(rest) = location.strip_prefix("https://") {
        let host = rest.split('/').next().unwrap_or(rest);
        if host.ends_with("play.net") || host.ends_with("simutronics.net") {
            return Ok(location.to_string());
        }
        return Err(WebLoginFailure::new(
            "character selection: untrusted redirect",
        ));
    }
    if location.starts_with('/') {
        return Ok(location.to_string());
    }
    Ok(location.to_string())
}

/// The launch parameters carried by a launch URL.
///
/// Recovered from `0x101c44dd4`: the URL needs a query, and the query needs
/// exactly one `host`, one `port` and one `key`; the port must be a number and
/// the host/port must belong to the instance.  Every failure message is a
/// recovered literal (`launch url: ...`).
pub fn parse_launch(url: &str, instance: &Instance) -> Result<WebLaunch, WebLoginFailure> {
    let query = match url.split_once('?') {
        Some((_, query)) => query,
        None => return Err(WebLoginFailure::new("launch url: no query")),
    };
    let mut host = None;
    let mut port = None;
    let mut key = None;
    for pair in query.split('&') {
        let (name, value) = match pair.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        let slot = match name {
            "host" => &mut host,
            "port" => &mut port,
            "key" => &mut key,
            _ => continue,
        };
        if slot.is_some() {
            return Err(WebLoginFailure::new(
                "launch url: duplicate host, port or key",
            ));
        }
        *slot = Some(value);
    }
    let (host, port, key) = match (host, port, key) {
        (Some(host), Some(port), Some(key)) => (host, port, key),
        _ => {
            return Err(WebLoginFailure::new(
                "launch url: missing host, port or key",
            ))
        }
    };
    if host.is_empty() || key.is_empty() {
        return Err(WebLoginFailure::new("launch url: blank host, port or key"));
    }
    let port: u16 = port
        .parse()
        .map_err(|_| WebLoginFailure::new("launch url: port is not a number"))?;
    if port == 0 {
        return Err(WebLoginFailure::new("launch url: blank host, port"));
    }
    if !instance.home.contains(instance.web_code) {
        return Err(WebLoginFailure::new(format!(
            "launch url: host or port is not this instance's {}",
            instance.code
        )));
    }
    Ok(WebLaunch {
        host: host.to_string(),
        port,
        key: key.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemstone::weblogin::instance::instance_for;

    fn gs3() -> &'static Instance {
        instance_for("GS3").unwrap()
    }

    #[test]
    fn redirects_are_classified_against_the_instance() {
        assert_eq!(
            classify_login_redirect("https://www.play.net/gs4/login_error.asp", gs3()),
            LoginRedirect::Error
        );
        assert_eq!(
            classify_login_redirect("https://www.play.net/gs4/play/home.asp", gs3()),
            LoginRedirect::Ok
        );
        assert_eq!(
            classify_login_redirect("/gs4/signin_needed.asp?x=1", gs3()),
            LoginRedirect::SignIn
        );
        assert_eq!(
            classify_login_redirect("/gs4/play/other.asp", gs3()),
            LoginRedirect::Elsewhere
        );
    }

    #[test]
    fn a_subscription_page_is_recognised() {
        assert!(is_subscription_needed("<a href=\"/gs4/play/subscription\">subscribe</a>"));
        assert!(!is_subscription_needed("<a href=\"/gs4/play/home.asp\">home</a>"));
    }

    #[test]
    fn character_codes_come_from_the_account_page() {
        let page = r#"
            <input type="radio" id="W_1_A" name="char" />
            <label for="W_1_A">Dridarn</label>
            <input type="radio" id="W_2_B" name="char" />
            <label for="W_2_B">Aelith</label>
        "#;
        assert_eq!(
            find_char_code(page, "Dridarn"),
            vec![("W_1_A".to_string(), "Dridarn".to_string())]
        );
        assert_eq!(find_char_code(page, "").len(), 2);
        assert_eq!(
            find_char_code(page, "aelith"),
            vec![("W_2_B".to_string(), "Aelith".to_string())],
            "the name match ignores case"
        );
        assert!(find_char_code(page, "Nobody").is_empty());
        assert!(find_char_code("no characters here", "").is_empty());
    }

    #[test]
    fn hops_stay_on_play_net() {
        assert_eq!(
            next_hop("https://www.play.net/gs4/x.asp").unwrap(),
            "https://www.play.net/gs4/x.asp"
        );
        assert_eq!(next_hop("/gs4/x.asp").unwrap(), "/gs4/x.asp");
        let err = next_hop("https://evil.example.com/gs4/x.asp").unwrap_err();
        assert_eq!(err.detail, "character selection: untrusted redirect");
    }

    #[test]
    fn a_launch_url_yields_host_port_and_key() {
        let url = "https://www.play.net/gs4/play/launch.asp?host=gs-play.simutronics.net&port=10121&key=abc";
        let launch = parse_launch(url, gs3()).unwrap();
        assert_eq!(launch.host, "gs-play.simutronics.net");
        assert_eq!(launch.port, 10121);
        assert_eq!(launch.key, "abc");
    }

    #[test]
    fn a_launch_url_must_say_everything() {
        assert_eq!(
            parse_launch("https://x/y", gs3()).unwrap_err().detail,
            "launch url: no query"
        );
        assert_eq!(
            parse_launch("/l?host=h&port=1", gs3()).unwrap_err().detail,
            "launch url: missing host, port or key"
        );
        assert_eq!(
            parse_launch("/l?host=h&port=1&key=k&key=j", gs3()).unwrap_err().detail,
            "launch url: duplicate host, port or key"
        );
        assert_eq!(
            parse_launch("/l?host=h&port=x&key=k", gs3()).unwrap_err().detail,
            "launch url: port is not a number"
        );
        assert_eq!(
            parse_launch("/l?host=&port=1&key=k", gs3()).unwrap_err().detail,
            "launch url: blank host, port or key"
        );
    }
}
