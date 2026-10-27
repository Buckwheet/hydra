//! Numeric attribute readers used by the inventory panel.
//!
//! Recovered from `crates/hydra-protocol/src/parser/inventory.rs`:
//!
//!   * `num` — two monomorphisations, `0x101c29e38` (644 B) and `0x101c2a0bc`
//!     (692 B).  They are the same function over two different attribute
//!     names; both call `text::attribute` (`0x101c26a7c`) first and then
//!     `str::trim_matches` (`0x101c2eb54`).
//!   * `inventory_manager` (7872 B) — not reconstructed, see `NOTES.md`.
//!
//! [`num`] is the crate's `attribute_u32` twin, and it differs from it in two
//! ways that the disassembly makes unambiguous: it decodes nothing (the value
//! comes back raw from `text::attribute`), and it `trim`s first.

use crate::text;

/// Read `name` out of `text` and parse it as a `u32`.
///
/// Established control flow (`0x101c29e38`):
///
/// ```text
/// let raw = text::attribute(text, name)?;          // None -> None
/// let raw = raw.trim();
/// if raw.is_empty() { return None }                // "" -> None
/// let digits = raw.strip_prefix('+').unwrap_or(raw);
/// // decimal accumulate, u32 checked
/// // "" (i.e. raw == "+") -> Some(0)
/// // any non-digit (including '-') -> None
/// ```
///
/// The digit loop is fully unrolled for 1..=8 digits and becomes a
/// `umull`/`adds` loop from 9 digits up; both forms answer `None` on `u32`
/// overflow (`tst x10, #0xffff_ffff_0000_0000` → `0x101c2a078`).
///
/// The leading-`+` handling is a `cset`/`cinc` pair, **not** a sign: a `-` is
/// simply a non-digit and yields `None`.  `text::attribute_u32` accepts the
/// same inputs but does not trim, so the two are deliberately distinct.
pub fn num(text: &str, name: &str) -> Option<u32> {
    let raw = text::attribute(text, name)?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let digits = raw.strip_prefix('+').unwrap_or(raw);
    let mut value: u32 = 0;
    for byte in digits.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value
            .checked_mul(10)?
            .checked_add(u32::from(byte - b'0'))?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_plain_number() {
        assert_eq!(num("<item count=\"7\"/>", "count"), Some(7));
        assert_eq!(num("<item count=\"0\"/>", "count"), Some(0));
        assert_eq!(num("<item count=\"4294967295\"/>", "count"), Some(u32::MAX));
    }

    #[test]
    fn the_value_is_trimmed_before_parsing() {
        assert_eq!(num("<item count=\" 12 \"/>", "count"), Some(12));
        assert_eq!(num("<item count=\"  +12\"/>", "count"), Some(12));
    }

    #[test]
    fn a_leading_plus_is_stripped_and_nothing_else_is_a_sign() {
        assert_eq!(num("<item count=\"+5\"/>", "count"), Some(5));
        // `-` is not a digit; the binary has no negative branch here.
        assert_eq!(num("<item count=\"-5\"/>", "count"), None);
    }

    #[test]
    fn an_empty_value_is_none_but_a_lone_plus_is_zero() {
        assert_eq!(num("<item count=\"\"/>", "count"), None);
        assert_eq!(num("<item count=\"   \"/>", "count"), None);
        // `"" .strip_prefix('+')` leaves an empty digit run, which the
        // unrolled loop answers with a zero value (`0x101c2a038`).
        assert_eq!(num("<item count=\"+\"/>", "count"), Some(0));
    }

    #[test]
    fn overflow_is_none() {
        assert_eq!(num("<item count=\"4294967296\"/>", "count"), None);
        assert_eq!(
            num("<item n=\"99999999999999999999999\"/>", "n"),
            None,
            "the `umull` overflow branch answers None"
        );
    }

    #[test]
    fn a_non_numeric_value_or_a_missing_attribute_is_none() {
        assert_eq!(num("<item count=\"many\"/>", "count"), None);
        assert_eq!(num("<item count=\"1.5\"/>", "count"), None);
        assert_eq!(num("<item count=\"1a\"/>", "count"), None);
        assert_eq!(num("<item other=\"1\"/>", "count"), None);
        assert_eq!(num("", "count"), None);
    }

    #[test]
    fn the_first_matching_attribute_in_the_text_wins() {
        // `text::attribute` scans the whole string (it only strips a trailing
        // `>`), so a missing attribute on the first tag falls through to a
        // later one, and two tags answer with the first.
        assert_eq!(num("<item/><item count=\"3\"/>", "count"), Some(3));
        assert_eq!(num("<item count=\"3\"/><item count=\"4\"/>", "count"), Some(3));
    }
}
