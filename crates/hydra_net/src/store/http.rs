//! The play.net store's URLs and failure families.
//!
//! Recovered from `hydra_net::store::http` (`0x101c53550` .. `0x101c53724` and
//! the message pool at `0x103e232be`).  The original used `reqwest`; the URL
//! shapes and the ways the store can fail were:

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
}
