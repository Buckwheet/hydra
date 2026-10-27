//! Numeric parsing helpers for the wire format.
//!
//! The original parsed amounts such as `"12/34"` (current/max) and
//! countdowns such as `"1:02:03"`.  The exact recovered behaviours are
//! documented on each function.

/// Parse a signed decimal `i32`, allowing an optional leading `+`/`-`.
///
/// This is the binary's private `numbers::signed` helper: it rejects empty
/// input, garbage and overflow.
fn signed(text: &str) -> Option<i32> {
    let bytes = text.as_bytes();
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: i32 = 0;
    for c in digits.chars() {
        let d = c.to_digit(10)? as i32;
        value = value.checked_mul(10)?;
        value = if negative {
            value.checked_sub(d)?
        } else {
            value.checked_add(d)?
        };
    }
    Some(value)
}

/// Parse an unsigned decimal `u64` with an optional leading `+`.
fn unsigned(text: &str) -> Option<u64> {
    let digits = text.strip_prefix('+').unwrap_or(text);
    if digits.is_empty() {
        return None;
    }
    let mut value: u64 = 0;
    for c in digits.chars() {
        let d = c.to_digit(10)? as u64;
        value = value.checked_mul(10)?.checked_add(d)?;
    }
    Some(value)
}

/// Parse a `current/max` amount pair.
///
/// Recovered behaviour:
///   * the string is trimmed;
///   * the **last** `/` splits the two sides;
///   * the left side is the last whitespace-separated token;
///   * the right side is the first whitespace-separated token;
///   * both sides must parse through the signed `i32` helper.
///
/// Consequences (asserted in the tests because they surprise callers):
/// no slash, comma-grouped numbers (`"1,234"`) and a left side whose last
/// token is a word (`"1,234 silvers"`) all return `None`.
pub fn parse_amount(text: &str) -> Option<(i32, i32)> {
    let text = text.trim();
    let slash = text.rfind('/')?;
    let left = text[..slash].trim_end();
    let right = text[slash + 1..].trim_start();

    let left = left.split_whitespace().next_back()?;
    let right = right.split_whitespace().next()?;
    Some((signed(left)?, signed(right)?))
}

/// Parse a duration into whole seconds.
///
/// The binary's recovered implementation handles colon forms only:
///   * `"m:ss"` (two segments)
///   * `"h:mm:ss"` (three segments)
///
/// A single segment (`"30"`) is rejected, as are four or more segments.
///
/// `// UNKNOWN:` the binary did **not** contain unit-suffix handling, but the
/// task specification asks for `"5m"`, `"30s"` and `"2h 15m"`.  Unit parsing
/// is therefore implemented additively (it cannot change the result of any
/// input the binary handled, because such input contains `:`).
pub fn parse_duration_secs(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    if text.contains(':') {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 2 && parts.len() != 3 {
            return None;
        }
        let mut nums = Vec::with_capacity(parts.len());
        for part in parts {
            nums.push(unsigned(part)?);
        }
        let total = match nums.as_slice() {
            [secs] => *secs,
            [mins, secs] => mins
                .checked_mul(60)?
                .checked_add(*secs)?,
            [hours, mins, secs] => hours
                .checked_mul(3600)?
                .checked_add(mins.checked_mul(60)?)?
                .checked_add(*secs)?,
            _ => return None,
        };
        return Some(total);
    }

    // Unit-suffix form: a sequence of `<digits><unit>` tokens, optionally
    // separated by whitespace, e.g. `"2h 15m"` or `"1h2m3s"`.
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut total: u64 = 0;
    let mut saw_unit = false;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let num_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == num_start {
            return None;
        }
        let value = unsigned(&text[num_start..i])?;
        // Allow `"1 hour"` as well as `"1h"`; whitespace between the number
        // and its unit is accepted by the additive unit-suffix parser.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let unit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit = &text[unit_start..i];
        let factor = match unit {
            "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "s" | "sec" | "secs" | "second" | "seconds" => 1,
            _ => return None,
        };
        total = total.checked_add(value.checked_mul(factor)?)?;
        saw_unit = true;
        if i < bytes.len() && !bytes[i].is_ascii_whitespace() && !bytes[i].is_ascii_digit() {
            return None;
        }
    }
    if saw_unit {
        Some(total)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_amounts() {
        assert_eq!(parse_amount("12/34"), Some((12, 34)));
        assert_eq!(parse_amount("  100 / 120  "), Some((100, 120)));
        assert_eq!(parse_amount("hp 100/120"), Some((100, 120)));
        assert_eq!(parse_amount("-5/+6"), Some((-5, 6)));
    }

    #[test]
    fn amount_requires_slash_and_plain_integers() {
        // Recovered behaviour: no slash -> None.
        assert_eq!(parse_amount("1,234"), None);
        assert_eq!(parse_amount("1,234 silvers"), None);
        // Comma grouping is not stripped by the binary.
        assert_eq!(parse_amount("1,234/2,000"), None);
        // A left-side word is taken as the token and fails to parse.
        assert_eq!(parse_amount("1,234 silvers/1,500"), None);
        assert_eq!(parse_amount("12/34/56"), None);
        assert_eq!(parse_amount(""), None);
    }

    #[test]
    fn parses_colon_durations() {
        assert_eq!(parse_duration_secs("1:02"), Some(62));
        assert_eq!(parse_duration_secs("1:02:03"), Some(3723));
        assert_eq!(parse_duration_secs(" 0:30 "), Some(30));
        assert_eq!(parse_duration_secs("12:00:00"), Some(43200));
    }

    #[test]
    fn rejects_bad_colon_durations() {
        assert_eq!(parse_duration_secs(""), None);
        assert_eq!(parse_duration_secs("30"), None);
        assert_eq!(parse_duration_secs("1:2:3:4"), None);
        assert_eq!(parse_duration_secs("1::3"), None);
        assert_eq!(parse_duration_secs("-1:02"), None);
    }

    #[test]
    fn parses_unit_durations() {
        assert_eq!(parse_duration_secs("5m"), Some(300));
        assert_eq!(parse_duration_secs("30s"), Some(30));
        assert_eq!(parse_duration_secs("2h 15m"), Some(8100));
        assert_eq!(parse_duration_secs("1h2m3s"), Some(3723));
        assert_eq!(parse_duration_secs("1 hour 30 minutes"), Some(5400));
        assert_eq!(parse_duration_secs("5x"), None);
    }

    #[test]
    fn signed_helper_checks_overflow() {
        assert_eq!(signed("2147483647"), Some(i32::MAX));
        assert_eq!(signed("-2147483648"), Some(i32::MIN));
        assert_eq!(signed("2147483648"), None);
        assert_eq!(signed(""), None);
        assert_eq!(signed("12a"), None);
    }
}
