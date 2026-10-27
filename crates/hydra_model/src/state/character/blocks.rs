//! The `INFO` report block.
//!
//! `InfoReport::read` (1960 B) splits on the recovered literals `" Race: "`,
//! `" Profession: "` and `" Age: "` and calls `StatLine::classify_with_bold`
//! for the stat rows.

use super::stats::Stat;

/// The parsed `INFO` block.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct InfoReport {
    pub race: Option<String>,
    pub profession: Option<String>,
    pub age: Option<u32>,
    pub stats: Vec<Stat>,
    /// Everything else, keyed by its label.
    pub other: std::collections::BTreeMap<String, String>,
}

impl InfoReport {
    /// @0x0000000101b63750 (1960 B)
    pub fn read(&mut self, lines: &[&str]) -> usize {
        let mut stored = 0;
        for line in lines {
            if self.read_line(line) {
                stored += 1;
            }
        }
        stored
    }

    pub fn read_line(&mut self, line: &str) -> bool {
        if let Some((_, value)) = line.split_once(" Race: ") {
            let value = value.trim();
            if self.race.as_deref() == Some(value) {
                return false;
            }
            self.race = Some(value.to_string());
            return true;
        }
        if let Some((_, value)) = line.split_once(" Profession: ") {
            let value = value.trim();
            if self.profession.as_deref() == Some(value) {
                return false;
            }
            self.profession = Some(value.to_string());
            return true;
        }
        if let Some((_, value)) = line.split_once(" Age: ") {
            let age = value
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse().ok());
            if age.is_some() && self.age == age {
                return false;
            }
            self.age = age;
            return age.is_some();
        }
        if let Some(stat) = Stat::classify_with_bold(line, false) {
            if let Some(kind) = stat.kind {
                if let Some(existing) = self.stats.iter_mut().find(|s| s.kind == Some(kind)) {
                    if existing.value == stat.value {
                        return false;
                    }
                    *existing = stat;
                } else {
                    self.stats.push(stat);
                }
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::character::stats::StatKind;

    #[test]
    fn race_profession_and_age() {
        let mut info = InfoReport::default();
        assert!(info.read_line("Kithar Race: Human"));
        assert!(info.read_line("Kithar Profession: Wizard"));
        assert!(info.read_line("Kithar Age: 42"));
        assert_eq!(info.race.as_deref(), Some("Human"));
        assert_eq!(info.profession.as_deref(), Some("Wizard"));
        assert_eq!(info.age, Some(42));
    }

    #[test]
    fn stat_rows_are_folded_in() {
        let mut info = InfoReport::default();
        assert!(info.read_line("Strength: 98 (30)"));
        assert_eq!(info.stats.len(), 1);
        assert!(!info.read_line("Strength: 98 (30)"));
        assert_eq!(info.stats[0].kind, Some(StatKind::Strength));
    }

    #[test]
    fn read_counts_changes() {
        let mut info = InfoReport::default();
        let lines = ["Kithar Race: Human", "Kithar Race: Human", "Logic: 70 (10)"];
        assert_eq!(info.read(&lines), 2);
    }

    #[test]
    fn unrelated_lines_are_ignored() {
        let mut info = InfoReport::default();
        assert!(!info.read_line("You are standing."));
    }
}
