//! The `ENHANCIVE` report.
//!
//! Recovered literals from `EnhanciveLine::classify` (4840 B): `" Bonus: "`,
//! `" Ranks: "`, `" ("`, `"): "`, `": "`, `": +"`, plus the stat names and
//! `Resource::parse` (512 B) / `Section::classify` (620 B) /
//! `EnhanciveTotals::apply` (976 B).
//!
//! `// UNKNOWN:` the section titles; only the recovered ones are listed.

use std::collections::BTreeMap;

use super::stats::StatKind;

/// Which part of the report a line belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
    #[default]
    Unknown,
    Bonus,
    Ranks,
}

impl Section {
    /// @0x0000000101b67a40 (620 B)
    pub fn classify(line: &str) -> Section {
        if line.contains(" Bonus: ") || line.trim_end().ends_with("Bonus") {
            Section::Bonus
        } else if line.contains(" Ranks: ") || line.trim_end().ends_with("Ranks") {
            Section::Ranks
        } else {
            Section::Unknown
        }
    }
}

/// What an enhancive line grants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Resource {
    #[default]
    Unknown,
    Mana,
    Spirit,
    Health,
    Stamina,
    Stat(StatKind),
}

impl Resource {
    /// @0x0000000101b67cac (512 B)
    pub fn parse(text: &str) -> Resource {
        let text = text.trim();
        match text.to_ascii_lowercase().as_str() {
            "mana" => Resource::Mana,
            "spirit" => Resource::Spirit,
            "health" => Resource::Health,
            "stamina" => Resource::Stamina,
            _ => match StatKind::parse(text) {
                Some(kind) => Resource::Stat(kind),
                None => Resource::Unknown,
            },
        }
    }
}

/// One enhancive line: `Strength: +3 (Bonus: 3, Ranks: 1)`.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct EnhanciveLine {
    pub resource: Resource,
    pub amount: i32,
    pub section: Section,
}

impl EnhanciveLine {
    /// @0x0000000101b66388 (4840 B)
    pub fn classify(line: &str) -> Option<EnhanciveLine> {
        let section = Section::classify(line);
        let line = line.trim();
        let (label, rest) = line.split_once(':')?;
        let resource = Resource::parse(label);
        if resource == Resource::Unknown {
            return None;
        }
        let amount_text = rest.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '-');
        let amount = amount_text
            .trim()
            .trim_start_matches('+')
            .parse::<i32>()
            .unwrap_or(0);
        Some(EnhanciveLine {
            resource,
            amount,
            section,
        })
    }
}

/// The running enhancive totals.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct EnhanciveTotals {
    pub bonuses: BTreeMap<Resource, i32>,
    pub ranks: BTreeMap<Resource, i32>,
}

impl EnhanciveTotals {
    /// @0x0000000101b67670 (976 B)
    pub fn apply(&mut self, line: &EnhanciveLine) -> bool {
        let target = match line.section {
            Section::Ranks => &mut self.ranks,
            _ => &mut self.bonuses,
        };
        let previous = target.get(&line.resource).copied().unwrap_or(0);
        if previous == line.amount {
            return false;
        }
        target.insert(line.resource, line.amount);
        true
    }

    pub fn bonus(&self, resource: Resource) -> i32 {
        self.bonuses.get(&resource).copied().unwrap_or(0)
    }

    pub fn rank(&self, resource: Resource) -> i32 {
        self.ranks.get(&resource).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_parse_covers_stats_and_vitals() {
        assert_eq!(Resource::parse("mana"), Resource::Mana);
        assert_eq!(Resource::parse("Stamina"), Resource::Stamina);
        assert_eq!(
            Resource::parse("Strength"),
            Resource::Stat(StatKind::Strength)
        );
        assert_eq!(Resource::parse("Basketweaving"), Resource::Unknown);
    }

    #[test]
    fn section_classification() {
        assert_eq!(Section::classify("Strength Bonus: +3"), Section::Bonus);
        assert_eq!(Section::classify("Strength Ranks: 1"), Section::Ranks);
        assert_eq!(Section::classify("nothing"), Section::Unknown);
    }

    #[test]
    fn enhancive_line_parses_amount() {
        let line = EnhanciveLine::classify("Strength: +3").unwrap();
        assert_eq!(line.resource, Resource::Stat(StatKind::Strength));
        assert_eq!(line.amount, 3);
    }

    #[test]
    fn totals_accumulate_by_section() {
        let mut totals = EnhanciveTotals::default();
        assert!(totals.apply(&EnhanciveLine {
            resource: Resource::Stat(StatKind::Strength),
            amount: 3,
            section: Section::Bonus
        }));
        assert!(!totals.apply(&EnhanciveLine {
            resource: Resource::Stat(StatKind::Strength),
            amount: 3,
            section: Section::Bonus
        }));
        assert!(totals.apply(&EnhanciveLine {
            resource: Resource::Stat(StatKind::Strength),
            amount: 1,
            section: Section::Ranks
        }));
        assert_eq!(totals.bonus(Resource::Stat(StatKind::Strength)), 3);
        assert_eq!(totals.rank(Resource::Stat(StatKind::Strength)), 1);
    }
}
