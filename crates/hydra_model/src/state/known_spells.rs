//! Known spells (`SPELLS` / `known spells`).
//!
//! `classify` (3804 B) folds the spell list in (it lowercases, de-duplicates and
//! removes entries), `KnownSpells::knows` (800 B) is the membership test.

use std::collections::BTreeSet;

/// Spell names the character knows (lowercased).
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct KnownSpells {
    pub names: BTreeSet<String>,
}

impl KnownSpells {
    /// @0x0000000101c0e1bc (800 B)
    pub fn knows(&self, name: &str) -> bool {
        self.names.contains(&name.trim().to_lowercase())
    }
}

/// @0x0000000101c0e4dc (3804 B)
pub fn classify(state: &mut KnownSpells, line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return false;
    }
    // A spell row is "<name>....<circle>" or a bare name.
    let name = line
        .split(|c: char| c.is_ascii_digit())
        .next()
        .unwrap_or(line)
        .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
        .to_lowercase();
    if name.is_empty() {
        return false;
    }
    state.names.insert(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_lowercased_and_deduplicated() {
        let mut spells = KnownSpells::default();
        assert!(classify(&mut spells, "Minor Shock...................1"));
        assert!(spells.knows("minor shock"));
        assert!(!classify(&mut spells, "Minor Shock...................1"));
        assert_eq!(spells.names.len(), 1);
    }

    #[test]
    fn knows_is_case_insensitive() {
        let mut spells = KnownSpells::default();
        classify(&mut spells, "Fireball");
        assert!(spells.knows("  FIREBALL "));
        assert!(!spells.knows("Iceball"));
    }

    #[test]
    fn empty_lines_are_ignored() {
        let mut spells = KnownSpells::default();
        assert!(!classify(&mut spells, "   "));
    }
}
