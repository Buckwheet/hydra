//! Climbing back out of a room the walk could not cross.
//!
//! Recovered from `hydra_travel::walk::recovery::{Attempt::new, Attempt::again,
//! Attempt::react, with_verb}` (`0x101520d50`, `0x101520e50`, `0x1015210dc`,
//! `0x101522458`).

/// The ordinals `with_verb` knows, from the pool run at `0x10241c1ab`:
/// `first`, `second`, `third`, … `twelfth`, plus `stow feet` and
/// `second door`.
pub const ORDINALS: [&str; 13] = [
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
    "sixth",
    "seventh",
    "eighth",
    "ninth",
    "tenth",
    "eleventh",
    "twelfth",
    "feet",
];

/// The verbs a recovery attempt may try, from the same run: `go`, `climb`,
/// `open`, `stow`.
pub const VERBS: [&str; 4] = ["go", "climb", "open", "stow"];

/// Put a verb in front of an ordinal: `go second door`.
///
/// **ESTABLISHED (literals)** (`0x101522458`): the body joins one of
/// [`VERBS`] with one of [`ORDINALS`] and the noun `door` (`0x102080d70`).
pub fn with_verb(verb: &str, ordinal: &str) -> String {
    format!("{verb} {ordinal} door")
}

/// One attempt at getting out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attempt {
    /// Which verb of [`VERBS`] this attempt uses.
    pub verb: usize,
    /// Which ordinal of [`ORDINALS`] it names.
    pub ordinal: usize,
    /// How many times it has been retried.
    pub tries: u32,
}

impl Attempt {
    pub fn new() -> Attempt {
        Attempt::default()
    }

    /// The next attempt after a failure (`0x101520e50`).
    pub fn again(&mut self) -> Option<String> {
        self.tries += 1;
        if self.tries > 3 {
            return None;
        }
        self.ordinal += 1;
        if self.ordinal >= ORDINALS.len() {
            self.ordinal = 0;
            self.verb += 1;
        }
        let verb = VERBS.get(self.verb)?;
        Some(with_verb(verb, ORDINALS[self.ordinal]))
    }

    /// React to a line of output (`0x1015210dc`).
    ///
    /// **Partly recovered**: the body matches the room's refusal lines and
    /// reports whether another attempt is worth making.
    pub fn react(&mut self, line: &str) -> bool {
        let lower = line.to_ascii_lowercase();
        lower.contains("you can't go that way")
            || lower.contains("you cannot go that way")
            || lower.contains("is locked")
            || lower.contains("appears to be locked")
            || lower.contains("you can't do that")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ordinals_are_the_recovered_thirteen() {
        assert_eq!(ORDINALS.len(), 13);
        assert_eq!(ORDINALS[0], "first");
        assert_eq!(ORDINALS[11], "twelfth");
        assert_eq!(ORDINALS[12], "feet");
        assert_eq!(VERBS, ["go", "climb", "open", "stow"]);
        assert_eq!(with_verb("climb", "second"), "climb second door");
    }

    #[test]
    fn attempts_walk_the_table() {
        let mut attempt = Attempt::new();
        assert_eq!(attempt.again().as_deref(), Some("go second door"));
        assert_eq!(attempt.again().as_deref(), Some("go third door"));
        assert_eq!(attempt.again().as_deref(), Some("go fourth door"));
        assert_eq!(attempt.again(), None);
        assert!(attempt.react("It appears to be locked."));
        assert!(!attempt.react("The gate opens."));
    }
}
