//! Experience absorption.
//!
//! Recovered literals from `Experience::read_line` (2252 B): `" experience
//! point"`, `" in your brooch"`; `apply_bar` (1436 B) folds one bar reading.
//! `// UNKNOWN:` the exact bar field set; the recovered body copies a numeric
//! triple.

use serde::Deserialize;

/// Experience state.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Experience {
    /// Total absorbed experience.
    pub total: u64,
    /// Experience in the brooch (the `"in your brooch"` figure).
    pub brooch: u64,
    /// Absorbed this pulse.
    pub absorbed: u64,
    /// Field experience earned since the last absorption.
    pub field: u64,
}

impl Experience {
    /// @0x0000000101bc3e9c (2252 B) — `"You absorb 123 experience points."`,
    /// `"123 experience points are in your brooch."`.
    pub fn read_line(&mut self, line: &str) -> bool {
        let mut changed = false;
        if line.contains("in your brooch") {
            if let Some(number) = first_number(line) {
                changed |= self.brooch != number;
                self.brooch = number;
            }
        } else if line.contains(" experience point") {
            if let Some(number) = first_number(line) {
                if line.contains("absorb") {
                    changed |= self.absorbed != number;
                    self.absorbed = number;
                    self.total = self.total.saturating_add(number);
                } else {
                    changed |= self.field != number;
                    self.field = number;
                }
            }
        }
        changed
    }

    /// @0x0000000101bc3900 (1436 B) — fold a bar reading
    /// `(current, maximum)` into the state.
    pub fn apply_bar(&mut self, current: u64, maximum: u64) -> bool {
        if self.field == current && self.total == maximum {
            return false;
        }
        self.field = current;
        self.total = maximum;
        true
    }
}

fn first_number(line: &str) -> Option<u64> {
    let mut digits = String::new();
    for c in line.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if !digits.is_empty() {
            break;
        }
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absorbed_experience_is_added_to_the_total() {
        let mut xp = Experience::default();
        assert!(xp.read_line("You absorb 120 experience points."));
        assert_eq!(xp.absorbed, 120);
        assert_eq!(xp.total, 120);
        assert!(xp.read_line("You absorb 30 experience points."));
        assert_eq!(xp.total, 150);
    }

    #[test]
    fn brooch_figure_is_separate() {
        let mut xp = Experience::default();
        assert!(xp.read_line("4180 experience points are in your brooch."));
        assert_eq!(xp.brooch, 4180);
        assert_eq!(xp.total, 0);
        assert!(!xp.read_line("4180 experience points are in your brooch."));
    }

    #[test]
    fn bar_updates_only_on_change() {
        let mut xp = Experience::default();
        assert!(xp.apply_bar(10, 100));
        assert!(!xp.apply_bar(10, 100));
        assert!(xp.apply_bar(20, 100));
        assert_eq!(xp.field, 20);
    }

    #[test]
    fn unrelated_lines_are_ignored() {
        let mut xp = Experience::default();
        assert!(!xp.read_line("You feel a sense of accomplishment."));
    }
}
