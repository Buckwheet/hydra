//! Critical-hit tables.
//!
//! `CritTables` owns the parsed rows plus a [`MatchIndex`] over their
//! `pattern` column. Recovered shapes:
//!
//! * `from_entries` (612 B) takes the entry vector by value and calls
//!   `MatchIndex::build`; it returns the tables through the sret pointer.
//! * `load` (144 B) has **no arguments**: it calls
//!   `parse_tsv(0x103a3ff08, 0x7dcc)` — the embedded table — and then
//!   `from_entries`, returning `Result<CritTables, BuildError>`.
//! * `parse` (408 B) calls `MatchIndex::matches` and maps the indices back to
//!   entries.

pub mod load;
pub mod match_index;
pub mod types;

use load::{parse_tsv, BuildError, CritEntry, EMBEDDED_TSV};
use match_index::MatchIndex;
use types::{Position, WoundLocation};

/// Every crit-table row plus the pattern index.
#[derive(Clone, Debug, Default)]
pub struct CritTables {
    pub entries: Vec<CritEntry>,
    pub index: MatchIndex,
}

impl CritTables {
    /// Build the index over `entries`.
    pub fn from_entries(entries: Vec<CritEntry>) -> CritTables {
        let patterns: Vec<String> = entries.iter().map(|e| e.pattern.clone()).collect();
        let index = MatchIndex::build(&patterns);
        CritTables { entries, index }
    }

    /// Parse the table that is embedded in the binary.
    pub fn load() -> Result<CritTables, BuildError> {
        let entries = parse_tsv(EMBEDDED_TSV)?;
        Ok(CritTables::from_entries(entries))
    }

    /// Every entry whose pattern matches `line`, in table order.
    pub fn parse(&self, line: &str) -> Vec<&CritEntry> {
        self.index
            .matches(line)
            .into_iter()
            .filter_map(|i| self.entries.get(i))
            .collect()
    }

    /// The strongest (highest-rank) entry matching `line`, if any.
    pub fn best(&self, line: &str) -> Option<&CritEntry> {
        self.parse(line).into_iter().max_by_key(|e| e.rank)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Rows for one damage type, in table order.
    pub fn of_type<'a>(&'a self, damage_type: &'a str) -> impl Iterator<Item = &'a CritEntry> + 'a {
        self.entries
            .iter()
            .filter(move |e| e.damage_type == damage_type)
    }

    /// Rows for one damage type and wound location, ascending by rank.
    pub fn lookup(&self, damage_type: &str, location: WoundLocation) -> Vec<&CritEntry> {
        let mut rows: Vec<&CritEntry> = self
            .entries
            .iter()
            .filter(|e| e.damage_type == damage_type && e.location == location)
            .collect();
        rows.sort_by_key(|e| e.rank);
        rows
    }

    /// The entry for an exact (type, location, rank) triple.
    pub fn get(&self, damage_type: &str, location: WoundLocation, rank: u32) -> Option<&CritEntry> {
        self.entries
            .iter()
            .find(|e| e.damage_type == damage_type && e.location == location && e.rank == rank)
    }

    /// Distinct damage types in table order of first appearance.
    pub fn damage_types(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for entry in &self.entries {
            if !out.contains(&entry.damage_type.as_str()) {
                out.push(&entry.damage_type);
            }
        }
        out
    }
}

/// Re-export so callers do not need to reach into `crit::types`.
pub use types::{Position as CritPosition, WoundLocation as CritLocation};

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> CritTables {
        CritTables::load().expect("embedded crit table")
    }

    #[test]
    fn embedded_table_loads_with_every_row() {
        let tables = tables();
        assert_eq!(tables.len(), 2394);
        assert!(!tables.is_empty());
        assert_eq!(tables.index.len(), 2394);
        assert_eq!(tables.index.compiled(), 2394);
    }

    #[test]
    fn lookup_finds_a_recovered_row() {
        let tables = tables();
        let row = tables
            .get("vacuum", WoundLocation::RightLeg, 9)
            .expect("vacuum right_leg rank 9");
        assert_eq!(row.damage, 45);
        assert_eq!(row.position, Position::Prone);
        assert!(row.amputated);
        assert_eq!(row.wound_rank, 3);
        assert_eq!(row.pattern, "^Right leg explodes into thousands of pieces.");
    }

    #[test]
    fn lookup_is_sorted_by_rank() {
        let tables = tables();
        let rows = tables.lookup("vacuum", WoundLocation::RightLeg);
        assert!(!rows.is_empty());
        let ranks: Vec<u32> = rows.iter().map(|r| r.rank).collect();
        let mut sorted = ranks.clone();
        sorted.sort_unstable();
        assert_eq!(ranks, sorted);
    }

    #[test]
    fn unknown_locations_in_the_table_parse_as_unknown() {
        // The embedded table has `left_arm` rows, but `WoundLocation::parse`
        // only accepts eight spellings; `left_arm` is one of the rest.
        let tables = tables();
        assert!(tables
            .entries
            .iter()
            .any(|e| e.location == WoundLocation::Unknown));
        let unknown = tables
            .entries
            .iter()
            .filter(|e| e.location == WoundLocation::Unknown)
            .count();
        assert!(unknown > 0);
    }

    #[test]
    fn parse_matches_a_real_crit_message() {
        let tables = tables();
        let matched = tables.parse("Elbow shatters under sudden decompression.");
        assert!(
            matched.iter().any(|e| e.damage_type == "vacuum"
                && e.location == WoundLocation::Unknown
                && e.rank == 4),
            "expected the vacuum/left_arm/4 row, got {:?}",
            matched
                .iter()
                .map(|e| (e.damage_type.as_str(), e.rank))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn parse_uses_the_wildcard_patterns() {
        let tables = tables();
        let matched = tables.parse("Too bad the kobold ducked.");
        assert!(!matched.is_empty());
        assert!(matched
            .iter()
            .any(|e| e.rank == 0 && e.damage_type == "acid"));
    }

    #[test]
    fn unknown_messages_match_nothing() {
        let tables = tables();
        assert!(tables.parse("You feel a gentle breeze.").is_empty());
        assert!(tables.best("You feel a gentle breeze.").is_none());
    }

    #[test]
    fn best_prefers_the_highest_rank() {
        let tables = tables();
        let best = tables
            .best("Right leg explodes into thousands of pieces.")
            .expect("a match");
        assert_eq!(best.rank, 9);
        assert_eq!(best.damage_type, "vacuum");
    }

    #[test]
    fn damage_types_are_recovered_in_order() {
        let tables = tables();
        let types = tables.damage_types();
        assert_eq!(types.first(), Some(&"acid"));
        assert!(types.contains(&"vacuum"));
        assert!(types.contains(&"disintegrate"));
    }

    #[test]
    fn from_entries_builds_an_index() {
        let entries = load::parse_tsv(load::EMBEDDED_TSV).unwrap();
        let first_two: Vec<CritEntry> = entries.into_iter().take(2).collect();
        let tables = CritTables::from_entries(first_two);
        assert_eq!(tables.len(), 2);
        assert_eq!(
            tables.parse("Too bad you ducked.").len(),
            1,
            "index must cover the copied rows"
        );
    }
}
