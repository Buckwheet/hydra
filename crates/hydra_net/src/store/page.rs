//! The play.net store's pages.
//!
//! Recovered from `hydra_net::store::page` (`0x101c5a3c4` .. `0x101c5e1d8`).
//! The store is scraped: Hydra reads the purchase page, finds the sign-in form's
//! verification token, signs in, and then looks for the claim links on the
//! account page.  Everything here works on the page text that the HTTP layer
//! (`store::http`) fetched.

/// The marker every claim link on the account page carries.
pub const CLAIM_LINK: &str = "/Store/ClaimReward";

/// The anti-forgery field the sign-in form carries.
pub const TOKEN_FIELD: &str = "__RequestVerificationToken";

/// The inner text of the first `<tag ...> ... </tag>` in `page`.
///
/// Recovered from `0x101c5bfac`: the opening `<tag` is located, then the first
/// `</tag` after it, and the text between them is trimmed.  The two literals are
/// `<{}` and `</{}` (0x103e7a269, 0x103e7fd5c).
pub fn read<'a>(page: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}", tag);
    let close = format!("</{}", tag);
    let start = page.find(&open)?;
    let after_open = start + page[start..].find('>')? + 1;
    let end = after_open + page[after_open..].find(&close)?;
    Some(page[after_open..end].trim())
}

/// True when the page offers a claim at all.
///
/// Recovered from `0x101c5a780`, which forwards to the shared `contains` helper
/// with the 18-byte needle `/Store/ClaimReward` (0x103bfa748).
pub fn offers_claim(page: &str) -> bool {
    page.contains(CLAIM_LINK)
}

/// True when the page says the reward was already taken today.
///
/// Recovered from `0x101c5a3c4`: the page is searched (case insensitively) for
/// the `claimed` markers.
pub fn is_claimed(page: &str) -> bool {
    let lower = page.to_ascii_lowercase();
    lower.contains("claimed") || lower.contains("already redeemed")
}

/// True when the page says the reward is on a timer.
///
/// Recovered from `0x101c5a590`: the case-folded page is searched for
/// `next sub` and `on bonus` (the two eight-byte needles in that function).
pub fn is_waiting(page: &str) -> bool {
    let lower = page.to_ascii_lowercase();
    lower.contains("next sub") || lower.contains("on bonus")
}

/// True when the page is the sign-in form rather than the store.
///
/// Recovered from `0x101c5bdd4`: the login form's field names are looked for.
pub fn asks_to_sign_in(page: &str) -> bool {
    page.contains("login") && page.contains("Password")
}

/// True when the page shows a signed-in account.
///
/// Recovered from `0x101c5de70`: the page is scanned for the account chrome the
/// signed-in pages carry.
pub fn signed_in(page: &str) -> bool {
    page.contains("SignOut") || page.contains("sign out") || page.contains("Account/SignOut")
}

/// The anti-forgery token the sign-in form carries.
///
/// Recovered from `0x101c5c934`: the page is searched for
/// `__RequestVerificationToken`, then the nearest `value=` after it, whose
/// quoted contents are the token.
pub fn token(page: &str) -> Option<String> {
    let at = page.find(TOKEN_FIELD)?;
    let tail = &page[at..];
    let value_at = tail.find("value=")?;
    let rest = &tail[value_at + "value=".len()..];
    let rest = rest.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        let end = rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
        return Some(rest[..end].to_string());
    }
    let body = &rest[1..];
    let end = body.find(quote)?;
    Some(body[..end].to_string())
}

/// The next time the store will have something to claim, in unix seconds.
///
/// Recovered from `0x101c5a798`: the page's "next available" text (which names a
/// month and a day) is turned into a timestamp.  UNKNOWN: the exact parse is
/// spread through the inlined month/entity tables; this reconstruction only
/// recognises the plain `MM/DD/YYYY` and `Month DD, YYYY` shapes.
pub fn next_available(page: &str) -> Option<i64> {
    let lower = page.to_ascii_lowercase();
    // UNKNOWN: the month-name/entity tables the original parses are inlined;
    // only numeric dates are recognised here.
    let at = lower.find("available")?;
    let tail = &page[at..];
    let start = tail.find(|c: char| c.is_ascii_digit())?;
    let tail = &tail[start..];
    let end = tail
        .find(|c: char| !(c.is_ascii_digit() || c == '/' || c == '-'))
        .unwrap_or(tail.len());
    parse_stamp(&tail[..end])
}

/// How long until the store has something to claim, in seconds.
///
/// Recovered from `0x101c5cd1c`: the difference between [`next_available`] and
/// now, floored at zero.
pub fn next_in(page: &str, now: i64) -> Option<i64> {
    next_available(page).map(|at| (at - now).max(0))
}

/// Parse the small subset of date spellings the store uses.
///
/// ESTABLISHED shape: `M/D/YYYY` and `M-D-YYYY` (the page's own spelling).
fn parse_stamp(text: &str) -> Option<i64> {
    let (a, rest) = text.split_once(['/', '-'])?;
    let (b, c) = rest.split_once(['/', '-'])?;
    let month: i64 = a.parse().ok()?;
    let day: i64 = b.parse().ok()?;
    let year: i64 = c.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || !(1970..=9999).contains(&year) {
        return None;
    }
    let date = crate::eastern::Date::new(year, month, day);
    Some(crate::eastern::midnight(&date))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_returns_the_inner_text() {
        let page = "<html><div class=\"x\">  hello  </div></html>";
        assert_eq!(read(page, "div"), Some("hello"));
        assert_eq!(read(page, "span"), None);
    }

    #[test]
    fn claims_are_spotted_by_the_link() {
        assert!(offers_claim("<a href=\"/Store/ClaimReward?id=3\">claim</a>"));
        assert!(!offers_claim("<a href=\"/store/purchase/3\">buy</a>"));
    }

    #[test]
    fn claim_state_is_read_from_the_page() {
        assert!(is_claimed("You have already CLAIMED today's reward"));
        assert!(!is_claimed("claim your reward"));
        assert!(is_waiting("your next sub is due"));
        assert!(is_waiting("You are on bonus time"));
        assert!(!is_waiting("claim your reward"));
    }

    #[test]
    fn the_token_is_read_out_of_the_form() {
        let page = r#"<form><input name="__RequestVerificationToken" type="hidden" value="CfDJ8abc" /></form>"#;
        assert_eq!(token(page).as_deref(), Some("CfDJ8abc"));
        let unquoted = r#"<input name="__RequestVerificationToken" value=tok123 />"#;
        assert_eq!(token(unquoted).as_deref(), Some("tok123"));
        assert_eq!(token("<form></form>"), None);
    }

    #[test]
    fn sign_in_pages_are_recognised() {
        assert!(asks_to_sign_in("<input name=\"Password\" type=\"password\">login"));
        assert!(!asks_to_sign_in("<a href=\"/Store/ClaimReward\">"));
        assert!(signed_in("<a href=\"/Account/SignOut\">sign out</a>"));
        assert!(!signed_in("<form>"));
    }

    #[test]
    fn the_next_date_is_a_timestamp() {
        let now = crate::eastern::now();
        let page = "Next available 3/10/2024 for you";
        let at = next_available(page).unwrap();
        assert_eq!(
            at,
            crate::eastern::midnight(&crate::eastern::Date::new(2024, 3, 10))
        );
        assert_eq!(next_in(page, at).unwrap(), 0);
        assert_eq!(next_in(page, at - 60).unwrap(), 60);
        assert_eq!(next_available("nothing to see"), None);
        assert_eq!(next_available("available 99/99/2024"), None);
        let _ = now;
    }
}
