//! `PSM` (combat maneuver / feat) tables.
//!
//! Recovered: `read_tables` (1008 B) uses `Runs::plain` + `Runs::bold_fragments`
//! and calls `classify_header` (376 B, splitting on `", "`) then
//! `PsmLine::classify` (1676 B). `PsmLine::classify` matches the literal
//! `"Mnemonic"`. `PsmSet::replace_category` (404 B) and `has_table` (116 B)
//! are set operations; serde `expecting` texts `struct PsmSet` / `struct
//! PsmRanks` survive.

use std::collections::BTreeMap;

use super::vocabulary::PsmCategory;

/// A learned PSM with its ranks.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct PsmRanks {
    /// `name -> ranks`
    pub ranks: BTreeMap<String, u32>,
}

impl PsmRanks {
    pub fn rank(&self, name: &str) -> u32 {
        self.ranks.get(name).copied().unwrap_or(0)
    }
}

/// The per-category PSM tables.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct PsmSet {
    pub categories: BTreeMap<PsmCategory, PsmRanks>,
}

impl PsmSet {
    /// @0x0000000101b5927c (116 B)
    pub fn has_table(&self, category: PsmCategory) -> bool {
        self.categories.contains_key(&category)
    }

    /// @0x0000000101b590e8 (404 B) — install a category table, returning the
    /// replaced one.
    pub fn replace_category(
        &mut self,
        category: PsmCategory,
        ranks: PsmRanks,
    ) -> Option<PsmRanks> {
        self.categories.insert(category, ranks)
    }

    pub fn rank(&self, category: PsmCategory, name: &str) -> u32 {
        self.categories
            .get(&category)
            .map(|table| table.rank(name))
            .unwrap_or(0)
    }
}

/// One classified PSM line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PsmLine {
    pub category: Option<PsmCategory>,
    pub name: String,
    pub ranks: u32,
}

impl PsmLine {
    /// @0x0000000101b592f0 (1676 B) — `"<name>....<ranks>"`, with the literal
    /// `"Mnemonic"` marking the name column.
    pub fn classify(line: &str) -> Option<PsmLine> {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Mnemonic") {
            return None;
        }
        let digits_at = line.find(|c: char| c.is_ascii_digit())?;
        let name = line[..digits_at]
            .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
            .to_string();
        if name.is_empty() {
            return None;
        }
        let ranks = line[digits_at..]
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        Some(PsmLine {
            category: None,
            name,
            ranks,
        })
    }
}

/// @0x0000000101b58f70 (376 B) — the table heading, `"<category>, <page>"`.
pub fn classify_header(line: &str) -> Option<PsmCategory> {
    PsmCategory::parse_heading(line)
}

/// @0x0000000101b58b80 (1008 B) — read a whole PSM table.
///
/// Returns the number of rows stored.
pub fn read_tables(set: &mut PsmSet, lines: &[&str]) -> usize {
    let mut stored = 0;
    let mut category: Option<PsmCategory> = None;
    for line in lines {
        if let Some(header) = classify_header(line) {
            category = Some(header);
            set.categories.entry(header).or_default();
            continue;
        }
        let Some(parsed) = PsmLine::classify(line) else {
            continue;
        };
        if let Some(current) = category {
            let table = set.categories.entry(current).or_default();
            table.ranks.insert(parsed.name, parsed.ranks);
            stored += 1;
        }
    }
    stored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psm_line_parses_name_and_ranks() {
        let line = PsmLine::classify("cheapshots....................5").unwrap();
        assert_eq!(line.name, "cheapshots");
        assert_eq!(line.ranks, 5);
    }

    #[test]
    fn header_lines_are_not_rows() {
        assert!(PsmLine::classify("Mnemonic                     Ranks").is_none());
        assert!(PsmLine::classify("").is_none());
    }

    #[test]
    fn read_tables_groups_by_category() {
        let mut set = PsmSet::default();
        let lines = [
            "Combat Maneuvers, page 1",
            "cheapshots....................5",
            "Feats, page 1",
            "acuity........................3",
        ];
        assert_eq!(read_tables(&mut set, &lines), 2);
        assert!(set.has_table(PsmCategory::CombatManeuvers));
        assert_eq!(set.rank(PsmCategory::CombatManeuvers, "cheapshots"), 5);
        assert_eq!(set.rank(PsmCategory::Feats, "acuity"), 3);
        assert_eq!(set.rank(PsmCategory::Feats, "cheapshots"), 0);
    }

    #[test]
    fn replace_category_returns_the_old_table() {
        let mut set = PsmSet::default();
        assert!(set.replace_category(PsmCategory::Feats, PsmRanks::default()).is_none());
        let mut table = PsmRanks::default();
        table.ranks.insert("acuity".into(), 2);
        let old = set.replace_category(PsmCategory::Feats, table).unwrap();
        assert_eq!(old.ranks.len(), 0);
        assert_eq!(set.rank(PsmCategory::Feats, "acuity"), 2);
    }
}
