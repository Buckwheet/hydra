//! `coverage::variant::Name` — a `fmt::Write` that builds a variant name.

use std::fmt;

/// A name being built by `write!`.
///
/// **ESTABLISHED** (`<Name as fmt::Write>::write_str`, `0x10154e0d4`, 312
/// bytes): the body walks the written text char by char (the standard four-byte
/// UTF-8 ladder is at `0x10154e0f4`..`0x10154e15c`) and computes
///
/// ```text
/// let is_digit = c >= 0x30 && (c & !0x20) < 0x41;   // `sub w8, w23, #0x3a`
///                                                   // `and w9, w23, #0xdf`
///                                                   // `sub w9, w9, #0x5b`
///                                                   // `ccmn w9, #0x1a, #0x2, lo`
/// ```
///
/// and **returns `Err`** as soon as that holds (`cmp w19, #0x1` / `b.eq` at
/// `0x10154e178`), appending every other char to the string it holds (the
/// `RawVecInner::reserve` call at `0x10154e1c4` is the `String::push_str`
/// growth path). So a name may not contain a digit — the `coverage` report
/// uses it to reject an input it cannot name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Name(pub String);

impl Name {
    pub fn new() -> Name {
        Name(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Write for Name {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            // The recovered predicate: ASCII digit (and, by the same arithmetic,
            // 0x3a..=0x40 — see NOTES.md).
            let is_digit = c >= '0' && (c as u32 & !0x20) < 0x41;
            if is_digit {
                return Err(fmt::Error);
            }
            self.0.push(c);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn accepts_letters_and_appends_them() {
        let mut name = Name::new();
        write!(name, "{}{}", "Or", "der").unwrap();
        assert_eq!(name.as_str(), "Order");
    }

    #[test]
    fn refuses_a_digit() {
        let mut name = Name::new();
        assert!(write!(name, "or2er").is_err());
        // Everything before the digit was appended.
        assert_eq!(name.as_str(), "or");
    }
}
