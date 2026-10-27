//! Target/DOM ids for indicators.
//!
//! `of` (152 B at `0x101be7ce4`) builds the 20-byte vitals/indicator record
//! from a source struct:
//!
//! ```text
//! kind = src[0x70];  if kind == 0x110000 { return {kind} }        // no reading
//! (a, b) = src[0x58..0x60]
//! if b == 0 { a = 0 }
//! if a == 1 && src[0x50] & 1 { cur = max(src[0x54], 0) }
//! else {
//!     max = if src[0x48] != 0 { src[0x4c] } else { 400 }
//!     if a == 0 { b = max }
//!     cur = (b - src[0x104]).saturating_sub(0)
//! }
//! percent = round(cur * 100 / b) clamped to 100
//! out = { kind, src[0x74..0x7c], src[0x7c], percent }
//! ```
//!
//! `tag` (876 B) builds a `String` with `FromIterator<char>` and clamps a
//! percentage to 100 (`w8 = 100; if x3 < 100 { w8 = x3 }`).

use crate::state::vitals::Vital;

/// A source record that [`of`] reads (the offsets above).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Source {
    /// `+0x70` — the kind word; `0x110000` means "no reading".
    pub kind: u32,
    /// `+0x74` — 8 bytes copied verbatim.
    pub payload: u64,
    /// `+0x7c` — one word copied verbatim.
    pub extra: u32,
    /// `+0x48`/`+0x4c` — the flag and the fallback maximum (`400`).
    pub scaled: u32,
    pub max: u32,
    /// `+0x50`/`+0x54` — the "flat value" flag and its value.
    pub flat: bool,
    pub flat_value: u32,
    /// `+0x58`/`+0x5c` — the current/max pair.
    pub current: u32,
    pub maximum: u32,
    /// `+0x104` — the damage taken, subtracted before the percentage.
    pub lost: u32,
}

impl Source {
    /// The recovered default maximum when `+0x48` is zero.
    pub const DEFAULT_MAX: u32 = 400;
}

/// @0x0000000101be7ce4 (152 B)
pub fn of(source: &Source) -> Vital {
    if source.kind == Vital::NO_READING {
        return Vital {
            kind: source.kind,
            ..Vital::default()
        };
    }
    let mut current = source.current;
    let mut maximum = source.maximum;
    if maximum == 0 && current != 1 {
        // `// UNKNOWN:` the 152-byte body zeroes the "current" reading when the
        // max is missing, so the percentage falls back to the 400 default.
        current = 0;
    }
    let numerator;
    if current == 1 && source.flat {
        numerator = source.flat_value;
    } else {
        let fallback = if source.scaled != 0 {
            source.max
        } else {
            Source::DEFAULT_MAX
        };
        if current == 0 {
            maximum = fallback;
        }
        numerator = maximum.saturating_sub(source.lost);
    }
    let percent = if maximum == 0 {
        0
    } else {
        let value = (numerator as f64) * 100.0 / (maximum as f64);
        value.round().min(100.0) as u32
    };
    Vital {
        kind: source.kind,
        payload: source.payload,
        extra: source.extra,
        percent,
    }
}

/// @0x0000000101be7d7c (876 B) — the DOM id for an indicator.
///
/// `// UNKNOWN:` the exact id grammar. The recovered function collects chars
/// into a `String` (from an alphabet that includes `A-Z0-9`) and clamps a
/// percentage to 100, so the reconstruction emits
/// `"<name>-<kind>-<percent>"` with the clamp applied.
pub fn tag(name: &str, kind: u32, percent: u32) -> String {
    let percent = percent.min(100);
    let mut out = String::with_capacity(name.len() + 8);
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("target");
    }
    format!("{out}-{kind}-{percent}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reading_is_passed_through() {
        let source = Source {
            kind: Vital::NO_READING,
            current: 5,
            maximum: 10,
            ..Source::default()
        };
        let vital = of(&source);
        assert_eq!(vital.kind, Vital::NO_READING);
        assert_eq!(vital.percent, 0);
        assert!(!vital.is_reading());
    }

    #[test]
    fn percentage_is_remaining_over_max() {
        let source = Source {
            kind: 1,
            current: 5,
            maximum: 100,
            lost: 25,
            ..Source::default()
        };
        // (100 - 25) * 100 / 100 = 75
        assert_eq!(of(&source).percent, 75);
    }

    #[test]
    fn percentage_is_clamped_at_one_hundred() {
        let source = Source {
            kind: 1,
            current: 5,
            maximum: 10,
            lost: 0,
            ..Source::default()
        };
        assert_eq!(of(&source).percent, 100);
    }

    #[test]
    fn flat_value_wins_over_the_subtraction() {
        let source = Source {
            kind: 7,
            current: 1,
            maximum: 50,
            flat: true,
            flat_value: 20,
            ..Source::default()
        };
        assert_eq!(of(&source).percent, 40);
    }

    #[test]
    fn zero_maximum_avoids_the_division() {
        let source = Source {
            kind: 3,
            current: 0,
            maximum: 0,
            lost: 400,
            ..Source::default()
        };
        // no max -> the recovered default of 400 is used
        assert_eq!(of(&source).percent, 0);
    }

    #[test]
    fn payload_and_extra_are_copied_verbatim() {
        let source = Source {
            kind: 2,
            payload: 0x1122_3344_5566_7788,
            extra: 99,
            current: 4,
            maximum: 8,
            lost: 0,
            ..Source::default()
        };
        let vital = of(&source);
        assert_eq!(vital.payload, 0x1122_3344_5566_7788);
        assert_eq!(vital.extra, 99);
        assert_eq!(vital.kind, 2);
    }

    #[test]
    fn tag_is_ascii_and_clamped() {
        assert_eq!(tag("Health", 1, 42), "health-1-42");
        assert_eq!(tag("Health", 1, 500), "health-1-100");
        assert_eq!(tag("a b", 2, 0), "a-b-2-0");
        assert_eq!(tag("", 0, 1), "target-0-1");
    }
}
