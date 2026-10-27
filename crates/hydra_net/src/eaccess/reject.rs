//! Classify the play.net page that rejected a launch.
//!
//! Recovered from `hydra_net::eaccess::reject` (`0x101c64830` .. `0x101c64b2c`).
//! The original upper-cases a copy of the page and looks for the markers the
//! website uses; both messages are exact literals (`__const` at `0x103bfaf27`
//! onwards).

/// The markers and messages the original matches, in the order it tests them.
///
/// `INVALID`/`REJECT`/`loader.asp` mean the launch link was already spent;
/// `NORECORD`/`PASSWORD`/`home.asp` mean the website never started a session.
const REJECTED_LINK: &str = "the link was used or old: send GOALS again for a new one";
const NO_SESSION: &str = "the website did not start its session: send GOALS again";

/// What the original says when the page carries none of the markers.
///
/// INFERRED: the fall-through text is not recoverable from the binary (the
/// second `format!` in the function builds the message from the page itself).
const UNRECOGNISED: &str = "the website rejected the launch, and the page said nothing recognisable";

/// Classify the page a rejected launch came back with.
///
/// Recovered from `0x101c64830`: the page is compared case insensitively (the
/// original builds an upper-cased and a lower-cased copy) against
/// `REJECT`/`INVALID`/`loader.asp` and `NORECORD`/`PASSWORD`/`home.asp`.
pub fn classify_a_rejection(text: &str) -> &'static str {
    let upper = text.to_ascii_uppercase();
    let has = |needle: &str| upper.contains(&needle.to_ascii_uppercase());
    if has("REJECT") || has("INVALID") || has("loader.asp") {
        REJECTED_LINK
    } else if has("NORECORD") || has("PASSWORD") || has("home.asp") {
        NO_SESSION
    } else {
        UNRECOGNISED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spent_link_says_to_ask_for_a_new_one() {
        assert_eq!(
            classify_a_rejection("ERROR: INVALID LAUNCH"),
            "the link was used or old: send GOALS again for a new one"
        );
        assert_eq!(
            classify_a_rejection("http://www.play.net/gs4/loader.asp?x=1"),
            "the link was used or old: send GOALS again for a new one"
        );
        assert_eq!(
            classify_a_rejection("the login was REJECTed"),
            "the link was used or old: send GOALS again for a new one"
        );
    }

    #[test]
    fn a_missing_session_says_to_ask_again() {
        assert_eq!(
            classify_a_rejection("NORECORD for that account"),
            "the website did not start its session: send GOALS again"
        );
        assert_eq!(
            classify_a_rejection("bad PASSWORD"),
            "the website did not start its session: send GOALS again"
        );
        assert_eq!(
            classify_a_rejection("<a href=\"home.asp\">"),
            "the website did not start its session: send GOALS again"
        );
    }

    #[test]
    fn case_does_not_matter() {
        assert_eq!(classify_a_rejection("invalid"), classify_a_rejection("INVALID"));
        assert_eq!(classify_a_rejection("Loader.Asp"), classify_a_rejection("LOADER.ASP"));
    }

    #[test]
    fn an_unknown_page_is_recognised_as_unknown() {
        assert_eq!(classify_a_rejection("<html>nothing</html>"), UNRECOGNISED);
        assert_eq!(classify_a_rejection(""), UNRECOGNISED);
    }
}
