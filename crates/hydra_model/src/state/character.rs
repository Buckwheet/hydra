//! The player character block.
//!
//! Recovered members (each has its own module and its own reader):
//! `identity`, `skills`, `stats`, `stance`, `standing`, `enhancive`,
//! `experience`, `currency`, `gemstones`, `psm`, `profile`, `blocks`,
//! `snapshot` and `shadow`.
//!
//! `Character::apply_info` (1160 B) merges an `InfoReport` (the `INFO` output)
//! into the character and returns whether anything changed; its literals
//! (`encum`, `blurb`, `experience point`, `your group`) are the keys it
//! matches. `Character::take_taught` (84 B) is a two-field state update.
//! `Character::exist_id` (320 B, in `identity`) returns the "exist id" used to
//! address creature links.
//!
//! `// UNKNOWN:` the exact `apply_info` field mapping.

pub mod blocks;
pub mod currency;
pub mod enhancive;
pub mod experience;
pub mod gemstones;
pub mod identity;
pub mod profile;
pub mod psm;
pub mod shadow;
pub mod skills;
pub mod snapshot;
pub mod stance;
pub mod standing;
pub mod stats;
pub mod vocabulary;

use stance::Stance;
use stats::StatSet;

/// Everything known about the logged-in character.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Character {
    pub name: String,
    pub race: String,
    pub profession: String,
    pub level: u32,
    pub age: Option<u32>,
    /// `INFO`-style free text keyed by its label (`encum`, `blurb`, …).
    pub info: std::collections::BTreeMap<String, String>,
    pub stats: StatSet,
    pub skills: skills::SkillSet,
    pub stance: Stance,
    pub standing: standing::Standing,
    pub experience: experience::Experience,
    pub currency: currency::Currency,
    pub gemstones: gemstones::Gemstones,
    pub psm: psm::PsmSet,
    pub enhancive: enhancive::EnhanciveTotals,
    /// Seconds of "taught" state (`take_taught`).
    pub taught: Option<u32>,
    /// `identity`: the id assigned to this character's links.
    pub exist_id: u32,
}

impl Character {
    /// @0x0000000101bd88ec (1160 B) — merge one line of `INFO` output.
    ///
    /// `// UNKNOWN:` the recovered function folds the whole `InfoReport` in one
    /// go (`InfoReport::read` is called from `roundtime_remaining_after`'s
    /// inlined block); this line-at-a-time form keeps the same key set.
    pub fn apply_line(&mut self, line: &str) -> bool {
        let mut changed = false;
        for (key, label) in INFO_KEYS {
            if let Some(value) = line.strip_prefix(label) {
                let value = value
                    .trim_start_matches(|c: char| c == ':' || c.is_whitespace())
                    .trim()
                    .to_string();
                if self.info.get(*key) != Some(&value) {
                    self.info.insert((*key).to_string(), value);
                    changed = true;
                }
            }
        }
        changed
    }

    /// @0x0000000101bd88ec (1160 B) — merge a whole `InfoReport`.
    ///
    /// The recovered function takes the parsed report and folds its fields in,
    /// returning whether anything changed.
    pub fn apply_info(&mut self, report: &blocks::InfoReport) -> bool {
        let mut changed = false;
        if let Some(race) = &report.race {
            if self.race != *race {
                self.race = race.clone();
                changed = true;
            }
        }
        if let Some(profession) = &report.profession {
            if self.profession != *profession {
                self.profession = profession.clone();
                changed = true;
            }
        }
        if let Some(age) = report.age {
            if self.age != Some(age) {
                self.age = Some(age);
                changed = true;
            }
        }
        for stat in &report.stats {
            if let Some(kind) = stat.kind {
                if self.stats.get(kind) != Some(stat.value) {
                    self.stats.stats.insert(kind, stat.value);
                    changed = true;
                }
            }
        }
        changed
    }

    /// @0x0000000101bd8?? (84 B) — record the "taught" expiry.
    pub fn take_taught(&mut self, seconds: Option<u32>) -> Option<u32> {
        std::mem::replace(&mut self.taught, seconds)
    }
}

/// `Encoder`/`Info` labels recovered from the string pool.
///
/// `// UNKNOWN:` the association between label and stored key beyond the ones
/// spelled out here (`encum`, `blurb`).
pub const INFO_KEYS: &[(&str, &str)] = &[
    ("encum", "Encumbrance"),
    ("blurb", "Blurb"),
    ("experience", "Experience"),
    ("level", "Level"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_lines_are_stored_under_their_key() {
        let mut character = Character::default();
        assert!(character.apply_line("Encumbrance: none"));
        assert_eq!(character.info.get("encum").map(String::as_str), Some("none"));
        // the same value again is not a change
        assert!(!character.apply_line("Encumbrance: none"));
    }

    #[test]
    fn unknown_info_lines_are_ignored() {
        let mut character = Character::default();
        assert!(!character.apply_line("Weather: clear"));
        assert!(character.info.is_empty());
    }

    #[test]
    fn apply_info_folds_the_report_in() {
        let mut character = Character::default();
        let mut report = blocks::InfoReport::default();
        // the recovered literals are `" Race: "`, `" Profession: "`, `" Age: "`
        assert!(report.read_line("Kithar Race: Human"));
        assert!(report.read_line("Kithar Profession: Wizard"));
        assert!(report.read_line("Kithar Age: 42"));
        report.read_line("Strength: 98 (30)");
        assert!(character.apply_info(&report));
        assert_eq!(character.race, "Human");
        assert_eq!(character.profession, "Wizard");
        assert_eq!(character.age, Some(42));
        assert_eq!(
            character.stats.value(stats::StatKind::Strength),
            98
        );
        assert!(!character.apply_info(&report));
    }

    #[test]
    fn take_taught_swaps_the_value() {
        let mut character = Character::default();
        assert_eq!(character.take_taught(Some(30)), None);
        assert_eq!(character.taught, Some(30));
        assert_eq!(character.take_taught(None), Some(30));
        assert_eq!(character.taught, None);
    }
}
