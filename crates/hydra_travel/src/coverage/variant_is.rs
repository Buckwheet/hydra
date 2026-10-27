//! `coverage::variant_is::Matching` — a `fmt::Write` that consumes a pattern.

use std::fmt;

/// A progressive matcher: writing text into it walks a pattern and stops at the
/// first character that does not continue it.
///
/// **ESTABLISHED** (`<Matching as fmt::Write>::write_str`, `0x10154e20c`, 1464
/// bytes): the receiver is two words — a cursor `(ptr @0, len @8)` — and for
/// every written char the body:
///
/// 1. tests it for alphanumeric (`sub w12, w11, #0x30` / `and w13, w11, #0xdf`
///    / `sub w13, w13, #0x41` at `0x10154e234`, i.e. a digit or a letter);
/// 2. if it is not alphanumeric, stops the whole match
///    (`b 0x10154e324`, which returns `Err`);
/// 3. otherwise compares it with the byte before the cursor
///    (`ldurb w12, [x10, #-0x1]` / `cmp w12, w11, uxtb` at `0x10154e2f8`) and
///    steps the cursor when it matches.
///
/// So `write!(matching, "abc")` succeeds only while `abc` is still a
/// subsequence of the pattern, and the whole thing answers "did the pattern run
/// out exactly at the end of the text".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matching {
    pattern: &'static str,
    at: usize,
}

impl Matching {
    pub fn new(pattern: &'static str) -> Matching {
        Matching { pattern, at: 0 }
    }

    /// Did the pattern run out exactly?
    pub fn matched(&self) -> bool {
        self.at == self.pattern.len()
    }

    /// The pattern.
    pub fn pattern(&self) -> &'static str {
        self.pattern
    }
}

impl fmt::Write for Matching {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if !c.is_ascii_alphanumeric() {
                return Err(fmt::Error);
            }
            let rest = &self.pattern[self.at..];
            let mut chars = rest.chars();
            if chars.next() != Some(c) {
                return Err(fmt::Error);
            }
            self.at += c.len_utf8();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn matching_walks_the_pattern() {
        let mut m = Matching::new("WanderWhile");
        write!(m, "Wander").unwrap();
        assert!(!m.matched());
        write!(m, "While").unwrap();
        assert!(m.matched());
    }

    #[test]
    fn a_wrong_character_stops_the_match() {
        let mut m = Matching::new("OrderByName");
        assert!(write!(m, "Ord").is_ok());
        assert!(write!(m, "ex").is_err());
    }

    #[test]
    fn a_non_alphanumeric_stops_the_match() {
        let mut m = Matching::new("OrderByName");
        assert!(write!(m, "Or der").is_err());
    }
}
