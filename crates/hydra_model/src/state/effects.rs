//! Timed effects/buffs.
//!
//! Recovered: `active` (1432 B), `active_in` (824 B), `active_named` (840 B),
//! `remaining` (1312 B, lowercases with `str::to_lowercase`),
//! `insert_lasting` (444 B), `insert_by_message` (1216 B), `saw_category`
//! (216 B) and `clear_category` (660 B).

use std::collections::BTreeMap;

/// One running effect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effect {
    pub name: String,
    /// Seconds remaining, as last observed.
    pub remaining: u32,
    /// Category (`Buffs`, `Debuffs`, `Cooldowns`, …).
    pub category: String,
    /// `true` for effects with no expiry (`Indefinite`).
    pub lasting: bool,
}

/// The effect table.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Effects {
    pub effects: BTreeMap<String, Effect>,
}

impl Effects {
    /// @0x0000000101b86ec0 (1432 B)
    pub fn active(&self) -> Vec<&Effect> {
        self.effects.values().collect()
    }

    /// @0x0000000101b87458 (824 B) — active effects in one category.
    pub fn active_in(&self, category: &str) -> Vec<&Effect> {
        let category = category.to_lowercase();
        self.effects
            .values()
            .filter(|effect| effect.category.to_lowercase() == category)
            .collect()
    }

    /// @0x0000000101b86190 (840 B) — one effect by name.
    pub fn active_named(&self, name: &str) -> Option<&Effect> {
        let name = name.trim().to_lowercase();
        self.effects
            .iter()
            .find(|(key, _)| key.to_lowercase() == name)
            .map(|(_, effect)| effect)
    }

    /// @0x0000000101b87790 (1312 B) — seconds left, lowercasing the name.
    pub fn remaining(&self, name: &str) -> Option<u32> {
        self.active_named(name).map(|effect| effect.remaining)
    }

    /// @0x0000000101b86844 (444 B)
    pub fn insert_lasting(&mut self, name: &str, category: &str) -> bool {
        let key = name.trim().to_string();
        if self.effects.contains_key(&key) {
            return false;
        }
        self.effects.insert(
            key.clone(),
            Effect {
                name: key,
                remaining: 0,
                category: category.to_string(),
                lasting: true,
            },
        );
        true
    }

    /// @0x0000000101b86a00 (1216 B) — insert from an effect message.
    pub fn insert_by_message(&mut self, message: &str) -> bool {
        let lower = message.to_lowercase();
        let mut inserted = false;
        for (needle, category) in [
            ("You feel more calm", "Buffs"),
            ("You feel the urge to flee", "Debuffs"),
            ("You are enraged", "Debuffs"),
            ("Your thoughts slowly come back", "Debuffs"),
        ] {
            if lower.contains(&needle.to_lowercase()) {
                inserted |= self.insert_lasting(needle, category);
            }
        }
        inserted
    }

    /// @0x0000000101b864d8 (216 B) — was this category observed?
    pub fn saw_category(&self, category: &str) -> bool {
        self.effects
            .values()
            .any(|effect| effect.category.eq_ignore_ascii_case(category))
    }

    /// @0x0000000101b865b0 (660 B)
    pub fn clear_category(&mut self, category: &str) -> usize {
        let before = self.effects.len();
        self.effects
            .retain(|_, effect| !effect.category.eq_ignore_ascii_case(category));
        before - self.effects.len()
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.effects.remove(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lasting_effects_are_inserted_once() {
        let mut effects = Effects::default();
        assert!(effects.insert_lasting("Haste", "Buffs"));
        assert!(!effects.insert_lasting("Haste", "Buffs"));
        assert!(effects.active_named("HASTE").unwrap().lasting);
        assert_eq!(effects.active().len(), 1);
    }

    #[test]
    fn categories_filter_and_clear() {
        let mut effects = Effects::default();
        effects.insert_lasting("Haste", "Buffs");
        effects.insert_lasting("Slow", "Debuffs");
        assert_eq!(effects.active_in("buffs").len(), 1);
        assert!(effects.saw_category("Debuffs"));
        assert_eq!(effects.clear_category("Debuffs"), 1);
        assert!(!effects.saw_category("Debuffs"));
    }

    #[test]
    fn remaining_reads_the_stored_value() {
        let mut effects = Effects::default();
        effects.effects.insert(
            "Haste".into(),
            Effect {
                name: "Haste".into(),
                remaining: 42,
                category: "Buffs".into(),
                lasting: false,
            },
        );
        assert_eq!(effects.remaining("haste"), Some(42));
        assert_eq!(effects.remaining("missing"), None);
    }

    #[test]
    fn message_driven_insertion() {
        let mut effects = Effects::default();
        assert!(effects.insert_by_message("You feel more calm."));
        assert!(effects.active_named("You feel more calm").is_some());
        assert!(!effects.insert_by_message("You feel more calm."));
        assert!(!effects.insert_by_message("Nothing happens."));
    }
}
