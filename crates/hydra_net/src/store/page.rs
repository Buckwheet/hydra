//! The play.net store's pages.
//!
//! Recovered from `hydra_net::store::page` (`0x101c5a3c4` .. `0x101c5e1d8`).
//! The store is scraped: Hydra reads the purchase page, finds the sign-in form's
//! verification token, signs in, and then looks for the claim links on the
//! account page.  Everything here works on the page text that the HTTP layer
//! (`store::http`) fetched.

use std::sync::OnceLock;

use regex::Regex;

/// The marker every claim link on the account page carries.
pub const CLAIM_LINK: &str = "/Store/ClaimReward";

/// The anti-forgery field the sign-in form carries.
pub const TOKEN_FIELD: &str = "__RequestVerificationToken";

/// The token on a form field whose attribute value may be quoted either way
/// (or, on the recovered pages, not quoted at all).
///
/// The pattern is anchored by the caller at the point just past `value=`, which
/// is why it starts with `\s*`: a quoted value wins over an unquoted one, and an
/// unquoted value stops at whitespace or the end of the tag.  Rust's `regex`
/// has no backreferences, so the two quoted shapes are spelled out and the
/// closing quote is not re-checked against the opening one — an unterminated
/// quote therefore lands in the unquoted arm and is refused there, which is what
/// the hand-written scan did.
const TOKEN_VALUE: &str = r#"(?s)^\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#;

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
/// quoted contents are the token.  The value itself is read with
/// [`TOKEN_VALUE`]; the two searches around it are literal (`find`), exactly as
/// recovered, so `value = "x"` with a space before the `=` still finds nothing
/// to read.
pub fn token(page: &str) -> Option<String> {
    let at = page.find(TOKEN_FIELD)?;
    let tail = &page[at..];
    let value_at = tail.find("value=")?;
    let rest = &tail[value_at + "value=".len()..];
    let pattern = token_value_pattern()?;
    let captures = pattern.captures(rest)?;
    if let Some(quoted) = captures.get(1).or_else(|| captures.get(2)) {
        return Some(quoted.as_str().to_string());
    }
    let bare = captures.get(3)?.as_str();
    // An opening quote with no closing one is not a value at all.
    if bare.starts_with(['"', '\'']) {
        return None;
    }
    Some(bare.to_string())
}

/// The compiled [`TOKEN_VALUE`] pattern, built once.
fn token_value_pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(TOKEN_VALUE).ok())
        .as_ref()
}

/// The "next available" date: three fields separated by `/` or `-`.
///
/// The trailing `[^0-9/-]`-or-end is what makes the year capture end the run of
/// digits and separators (Rust's `regex` has no lookahead), so the shape stays
/// "the whole run, then exactly three fields": `1/2/20245` is still one invalid
/// run rather than a valid `2024`.
const STORE_DATE: &str = r"([0-9]+)[/-]([0-9]+)[/-]([0-9]+)(?:[^0-9/-]|$)";

/// The next time the store will have something to claim, in unix seconds.
///
/// Recovered from `0x101c5a798`: the page's "next available" text (which names a
/// month and a day) is turned into a timestamp.  UNKNOWN: the exact parse is
/// spread through the inlined month/entity tables; this reconstruction only
/// recognises the plain `MM/DD/YYYY` and `Month DD, YYYY` shapes.  The scan is
/// the recovered shape spelled as a regex: past the word `available`
/// (case-insensitively), the first run of digits and `/`/`-` is read as exactly
/// three fields.
pub fn next_available(page: &str) -> Option<i64> {
    let at = available_pattern()?.find(page)?;
    let tail = &page[at.end()..];
    // UNKNOWN: the month-name/entity tables the original parses are inlined;
    // only numeric dates are recognised here.
    let start = tail.find(|c: char| c.is_ascii_digit())?;
    let captures = store_date_pattern()?.captures(&tail[start..])?;
    let field = |index: usize| -> Option<i64> { captures.get(index)?.as_str().parse().ok() };
    let (month, day, year) = (field(1)?, field(2)?, field(3)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || !(1970..=9999).contains(&year) {
        return None;
    }
    let date = crate::eastern::Date::new(year, month, day);
    Some(crate::eastern::midnight(&date))
}

/// The compiled `(?i)available` pattern, built once.
fn available_pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new("(?i)available").ok())
        .as_ref()
}

/// The compiled [`STORE_DATE`] pattern, built once.
fn store_date_pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(STORE_DATE).ok())
        .as_ref()
}

/// How long until the store has something to claim, in seconds.
///
/// Recovered from `0x101c5cd1c`: the difference between [`next_available`] and
/// now, floored at zero.
pub fn next_in(page: &str, now: i64) -> Option<i64> {
    next_available(page).map(|at| (at - now).max(0))
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
    fn the_token_value_shapes_are_the_recovered_ones() {
        let single = "<input name=\"__RequestVerificationToken\" value='abc123' />";
        assert_eq!(token(single).as_deref(), Some("abc123"));

        // An unquoted value stops at whitespace, and at the end of the tag.
        assert_eq!(token(&field_with_value("tok/")).as_deref(), Some("tok/"));
        assert_eq!(token(&field_with_value("tok next")).as_deref(), Some("tok"));

        // An opening quote with no closing one is not a value.
        assert_eq!(token(&field_with_value("\"abc />")), None);

        // The recovered scan looked for the literal `value=`, not `value =`.
        assert_eq!(
            token("<input name=\"__RequestVerificationToken\" value = \"abc\" />"),
            None
        );

        // A quoted token may contain spaces and line breaks.
        let wrapped = "<input name=\"__RequestVerificationToken\" value=\"a b\nc\" />";
        assert_eq!(token(wrapped).as_deref(), Some("a b\nc"));
    }

    /// A page whose verification field carries `value`.
    fn field_with_value(value: &str) -> String {
        format!("<input name=\"{}\" value={}", TOKEN_FIELD, value)
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

    #[test]
    fn the_next_date_run_is_read_whole() {
        let midnight = |y, m, d| {
            crate::eastern::midnight(&crate::eastern::Date::new(y, m, d))
        };
        // The word `available` is found case insensitively, and either separator
        // spelling works.
        assert_eq!(next_available("Next Available 3/10/2024"), Some(midnight(2024, 3, 10)));
        assert_eq!(next_available("NEXT AVAILABLE 12-31-2024"), Some(midnight(2024, 12, 31)));
        assert_eq!(next_available("available on 1/1/2025 at noon"), Some(midnight(2025, 1, 1)));

        // The run is taken whole: a fourth field or a five-digit year is one
        // invalid run, not a valid prefix of it.
        assert_eq!(next_available("available 3/10/2024/5"), None);
        assert_eq!(next_available("available 3/10/20245"), None);
        assert_eq!(next_available("available 0/10/2024"), None);
        assert_eq!(next_available("available 12/32/2024"), None);
        assert_eq!(next_available("available 2024"), None);

        // Nothing to read at all.
        assert_eq!(next_available(""), None);
        assert_eq!(next_available("available soon"), None);
    }
}
