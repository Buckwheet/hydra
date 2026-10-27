//! Character stats (`INFO` / `STATS` rows).
//!
//! Recovered: `StatLine::classify` (2208 B) matches the stat labels
//! `Strength/Dexterity/Agility/Discipline/Logic/Intuition/Wisdom/Influence`
//! (recovered as one concatenated run) and the `": "` separator;
//! `StatLine::classify_with_bold` (592 B) adds the bold-run handling.
//! serde `expecting` texts survive for `Stat`, `Identity`, `StatValue` and
//! `StatKind`, so all four derive `Deserialize` (see [`Identity`]).
//!
//! `// UNKNOWN:` the exact `(value)`/`(bonus)` decomposition; the recovered
//! parser reads `"label:"` then the integer after `" ("`.

use std::collections::BTreeMap;
use serde::Deserialize;

/// The eight trainable stats, in the recovered order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub enum StatKind {
    Strength,
    Constitution,
    Dexterity,
    Agility,
    Discipline,
    Logic,
    Intuition,
    Wisdom,
    Influence,
}

impl StatKind {
    pub const ALL: [StatKind; 9] = [
        StatKind::Strength,
        StatKind::Constitution,
        StatKind::Dexterity,
        StatKind::Agility,
        StatKind::Discipline,
        StatKind::Logic,
        StatKind::Intuition,
        StatKind::Wisdom,
        StatKind::Influence,
    ];

    pub fn key(self) -> &'static str {
        match self {
            StatKind::Strength => "Strength",
            StatKind::Constitution => "Constitution",
            StatKind::Dexterity => "Dexterity",
            StatKind::Agility => "Agility",
            StatKind::Discipline => "Discipline",
            StatKind::Logic => "Logic",
            StatKind::Intuition => "Intuition",
            StatKind::Wisdom => "Wisdom",
            StatKind::Influence => "Influence",
        }
    }

    pub fn parse(text: &str) -> Option<StatKind> {
        let text = text.trim();
        StatKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.key().eq_ignore_ascii_case(text))
    }
}

/// One stat reading: the value and its bonus.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct StatValue {
    pub value: i32,
    pub bonus: i32,
}

/// The character-identity block of the `INFO`/`PROFILE` output.
///
/// Only the serde `expecting` text
/// `<impl Deserialize for stats::Identity>::__Visitor` at `0x101c11bf8`
/// survives; the type lives in `stats` next to `Stat`/`StatValue`, which is why
/// it is declared here rather than in `identity.rs`.
/// `// UNKNOWN:` the field set — these are the identity fields the recovered
/// readers fill (`Character::apply_info`, `set_identity`, the `INFO` labels).
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Identity {
    pub name: String,
    pub race: String,
    pub profession: String,
    pub level: u32,
}

/// A `Stat` row.
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Stat {
    pub kind: Option<StatKind>,
    pub value: StatValue,
}

/// The whole stat block.
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct StatSet {
    pub stats: BTreeMap<StatKind, StatValue>,
}

impl StatSet {
    pub fn get(&self, kind: StatKind) -> Option<StatValue> {
        self.stats.get(&kind).copied()
    }

    pub fn value(&self, kind: StatKind) -> i32 {
        self.get(kind).map(|v| v.value).unwrap_or(0)
    }
}

/// One classified stat line.
pub type StatLine = Stat;

impl Stat {
    /// @0x0000000101c10660 (2208 B)
    ///
    /// Accepts `"Strength: 98 (30)"`: the label before `":"`, then the value and
    /// the bonus in parentheses.
    pub fn classify(line: &str) -> Option<Stat> {
        let line = line.trim();
        let (label, rest) = line.split_once(':')?;
        let kind = StatKind::parse(label.trim());
        let rest = rest.trim();
        let (value_text, bonus_text) = match rest.split_once('(') {
            Some((v, b)) => (v.trim(), b.trim_end_matches(')').trim()),
            None => (rest, ""),
        };
        let value = value_text
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<i32>().ok())?;
        let bonus = bonus_text.parse::<i32>().unwrap_or(0);
        Some(Stat {
            kind,
            value: StatValue { value, bonus },
        })
    }

    /// @0x0000000101c10410 (592 B) — the bold-run variant; styling is not
    /// modelled, so this forwards to [`Stat::classify`].
    pub fn classify_with_bold(line: &str, _bold: bool) -> Option<Stat> {
        Stat::classify(line)
    }
}

impl crate::state::character::Character {
    /// The identity block this character carries.
    pub fn identity(&self) -> Identity {
        Identity {
            name: self.name.clone(),
            race: self.race.clone(),
            profession: self.profession.clone(),
            level: self.level,
        }
    }

    /// Fold one stat line into the character.
    pub fn apply_stat_line(&mut self, line: &str) -> bool {
        let Some(stat) = Stat::classify(line) else {
            return false;
        };
        let Some(kind) = stat.kind else {
            return false;
        };
        if self.stats.get(kind) == Some(stat.value) {
            return false;
        }
        self.stats.stats.insert(kind, stat.value);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_line_parses_value_and_bonus() {
        let stat = Stat::classify("Strength: 98 (30)").unwrap();
        assert_eq!(stat.kind, Some(StatKind::Strength));
        assert_eq!(stat.value.value, 98);
        assert_eq!(stat.value.bonus, 30);
    }

    #[test]
    fn stat_line_without_bonus() {
        let stat = Stat::classify("Influence: 55").unwrap();
        assert_eq!(stat.kind, Some(StatKind::Influence));
        assert_eq!(stat.value.value, 55);
        assert_eq!(stat.value.bonus, 0);
    }

    #[test]
    fn non_stat_lines_are_rejected() {
        assert!(Stat::classify("You feel better.").is_none());
        assert!(Stat::classify("Strength: plenty").is_none());
    }

    #[test]
    fn every_stat_kind_round_trips() {
        for kind in StatKind::ALL {
            assert_eq!(StatKind::parse(kind.key()), Some(kind));
        }
        assert_eq!(StatKind::parse("strength"), Some(StatKind::Strength));
        assert_eq!(StatKind::parse("Luck"), None);
    }

    #[test]
    fn identity_reads_the_character_block() {
        let mut character = crate::state::character::Character::default();
        character.set_identity("Kithar", "Human", "Wizard");
        character.level = 42;
        let identity = character.identity();
        assert_eq!(identity.name, "Kithar");
        assert_eq!(identity.race, "Human");
        assert_eq!(identity.profession, "Wizard");
        assert_eq!(identity.level, 42);
    }

    #[test]
    fn character_absorbs_stat_lines_once() {
        let mut c = crate::state::character::Character::default();
        assert!(c.apply_stat_line("Logic: 70 (10)"));
        assert!(!c.apply_stat_line("Logic: 70 (10)"));
        assert_eq!(c.stats.value(StatKind::Logic), 70);
        assert_eq!(c.stats.value(StatKind::Wisdom), 0);
    }
}
