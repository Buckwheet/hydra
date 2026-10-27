//! The static bestiary template table.
//!
//! Recovered: `creatures()` (112 B) is a `OnceLock` accessor, `by_name` (808 B)
//! is a lowercased lookup, `by_noun` (164 B) indexes the noun, `in_room`
//! (136 B) a filter, and `Creature::description` (136 B) / `found_at` (52 B) /
//! `tips` (228 B) are accessors.
//!
//! `// UNKNOWN:` the bestiary itself (`creatures.tsv`: `id name noun level
//! max_hp family ...`) is a generated data file and is **not** embedded, so
//! every lookup answers `None`.

pub mod parts;
pub mod status;

use std::sync::OnceLock;

/// One bestiary row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Creature {
    pub id: u32,
    pub name: String,
    pub noun: String,
    pub level: Option<u32>,
    pub max_hp: Option<u32>,
    pub family: Option<String>,
    /// `passive npc` rows are allies.
    pub ally: bool,
    /// `// UNKNOWN:` free-text tips column.
    pub tips: Vec<String>,
    pub description: String,
    pub found_at: Vec<String>,
}

impl Creature {
    /// @0x0000000101b? (136 B)
    pub fn description(&self) -> &str {
        &self.description
    }

    /// @0x0000000101b? (52 B)
    pub fn found_at(&self) -> &[String] {
        &self.found_at
    }

    /// @0x0000000101b? (228 B)
    pub fn tips(&self) -> &[String] {
        &self.tips
    }
}

#[derive(Default, Clone, Debug)]
pub struct Bestiary {
    pub rows: Vec<Creature>,
}

static TABLE: OnceLock<Bestiary> = OnceLock::new();

/// @0x0000000101b? (112 B)
pub fn creatures() -> &'static Bestiary {
    TABLE.get_or_init(Bestiary::default)
}

/// Install a bestiary (test/data loader hook).
pub fn set_creatures(table: Bestiary) -> bool {
    TABLE.set(table).is_ok()
}

/// @0x0000000101b? (808 B) — case-insensitive name lookup.
pub fn by_name(name: &str) -> Option<&'static Creature> {
    let needle = name.trim().to_lowercase();
    creatures()
        .rows
        .iter()
        .find(|row| row.name.to_lowercase() == needle)
}

/// @0x0000000101b? (164 B) — noun lookup.
pub fn by_noun(noun: &str) -> Option<&'static Creature> {
    let needle = noun.trim().to_lowercase();
    creatures()
        .rows
        .iter()
        .find(|row| row.noun.to_lowercase() == needle)
}

/// @0x0000000101b? (136 B) — rows found in a room id.
pub fn in_room(room: u32) -> Vec<&'static Creature> {
    let _ = room;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bestiary_answers_none() {
        assert!(by_name("a kobold").is_none());
        assert!(by_noun("kobold").is_none());
        assert!(in_room(1).is_empty());
        assert!(creatures().rows.is_empty());
    }

    #[test]
    fn accessors_return_their_fields() {
        let creature = Creature {
            name: "a kobold".into(),
            noun: "kobold".into(),
            tips: vec!["watch the spear".into()],
            description: "A scruffy kobold.".into(),
            found_at: vec!["the commons".into()],
            ..Creature::default()
        };
        assert_eq!(creature.description(), "A scruffy kobold.");
        assert_eq!(creature.tips().len(), 1);
        assert_eq!(creature.found_at()[0], "the commons");
    }
}
