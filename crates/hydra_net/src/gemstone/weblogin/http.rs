//! The `Location` and status of the play.net web login's redirects.
//!
//! Recovered from `hydra_net::gemstone::weblogin::http::location_of`
//! (`0x101c5e1d8` .. `0x101c5e594`).  The original used `reqwest` and read the
//! `Location` header off the response; both spellings are here:
//!
//! * [`location_of_response`] / [`status_of_response`] take a real
//!   `reqwest::Response`, which is what the original did, and are what the web
//!   login should use;
//! * [`location_of`] / [`status_of`] work on the response's head as text, for a
//!   caller that only kept the bytes (the std-only reconstruction's shape, kept
//!   so nothing that already calls it has to change).

use reqwest::header::LOCATION;
use reqwest::Response;

/// The `Location` a real redirect response points at, if it carried one.
///
/// Recovered from `0x101c5e1d8`: a redirect without a `Location` (or with an
/// empty one) has nowhere to go.  The header name is matched case insensitively
/// by `reqwest`'s `HeaderMap`, and a value that is not UTF-8 is dropped rather
/// than guessed at.
pub fn location_of_response(response: &Response) -> Option<String> {
    let value = response.headers().get(LOCATION)?.to_str().ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

/// The status code of a real response, as the `u16` the recovered code kept.
pub fn status_of_response(response: &Response) -> u16 {
    response.status().as_u16()
}

/// The `Location` a redirect points at, if the response carried one.
///
/// Recovered from `0x101c5e1d8`: the header block is scanned line by line for a
/// `location:` header, case insensitively, and the value is trimmed.  A response
/// without one (a 200, say) has nowhere to go.
pub fn location_of(head: &str) -> Option<&str> {
    for line in head.lines() {
        let line = line.trim_end_matches('\r');
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("location") {
                let value = value.trim();
                if value.is_empty() {
                    return None;
                }
                return Some(value);
            }
        }
    }
    None
}

/// The status code on the first line of an HTTP response, if there is one.
pub fn status_of(head: &str) -> Option<u16> {
    let first = head.lines().next()?;
    let mut parts = first.split_whitespace();
    let _version = parts.next()?;
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redirect_is_found() {
        let head = "HTTP/1.1 302 Found\r\nLocation: /gs4/play/home.asp\r\n\r\n";
        assert_eq!(location_of(head), Some("/gs4/play/home.asp"));
        assert_eq!(status_of(head), Some(302));
    }

    #[test]
    fn the_header_name_is_case_insensitive() {
        assert_eq!(location_of("HTTP/1.1 301\r\nLOCATION: https://x/y\r\n"), Some("https://x/y"));
        assert_eq!(location_of("HTTP/1.1 301\r\nlocation:   /a  \r\n"), Some("/a"));
    }

    #[test]
    fn no_location_leaves_nothing_to_follow() {
        assert_eq!(location_of("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n"), None);
        assert_eq!(location_of(""), None);
        assert_eq!(location_of("HTTP/1.1 302\r\nLocation: \r\n"), None);
        assert_eq!(status_of(""), None);
        assert_eq!(status_of("nonsense"), None);
    }

    /// A real `reqwest` response, served from a loopback socket: no network.
    #[tokio::test]
    async fn a_real_response_carries_its_location() {
        let (base, server) = crate::http::loopback::serve_once(
            "302 Found",
            "Location: /gs4/play/home.asp\r\n",
            "",
        );
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("a client");
        let response = client.get(format!("{}/gsp/login", base)).send().await.unwrap();

        assert_eq!(status_of_response(&response), 302);
        assert_eq!(
            location_of_response(&response).as_deref(),
            Some("/gs4/play/home.asp")
        );
        // The two spellings answer the same thing.
        let head = format!(
            "HTTP/1.1 302 Found\r\nLocation: {}\r\n\r\n",
            location_of_response(&response).unwrap()
        );
        assert_eq!(location_of(&head), location_of_response(&response).as_deref());
        assert_eq!(status_of(&head), Some(status_of_response(&response)));

        let request = server.join().unwrap();
        assert!(request.starts_with("GET /gsp/login HTTP/1.1"), "{request}");
    }

    /// A 200 has nowhere to go, in either spelling.
    #[tokio::test]
    async fn a_real_response_without_a_location_has_none() {
        let (base, server) = crate::http::loopback::serve_once("200 OK", "", "home");
        let response = crate::http::client().get(&base).send().await.unwrap();

        assert_eq!(status_of_response(&response), 200);
        assert_eq!(location_of_response(&response), None);
        server.join().unwrap();
    }
}
