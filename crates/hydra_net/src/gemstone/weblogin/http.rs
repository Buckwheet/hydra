//! Minimal HTTP response header handling for the play.net web login.
//!
//! Recovered from `hydra_net::gemstone::weblogin::http::location_of`
//! (`0x101c5e1d8` .. `0x101c5e594`).  The original used `reqwest` and read the
//! `Location` header off the response; this is the header line spelled out.

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
}
