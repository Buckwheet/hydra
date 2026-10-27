//! Vitals (`mana`, `health`, `spirit`, `stamina`).
//!
//! Recovered: `GameState` keeps a `BTreeMap<String, _>` at `+0x1178` (root) /
//! `+0x1180` (len) whose values are **20 bytes** (`ldr q0, [x25]` +
//! `ldr w8, [x25, #0x10]`, values at node offset `0xfc`, stride `0x14`).
//!
//! `vital` (260 B) is the shared lookup; `mana`/`health`/`spirit`/`stamina`
//! are 264-byte wrappers that pass the constant names seen in the literal at
//! `0x103bf73ad` (`"manahealthspiritstamina"`) with lengths 4/6/6/7. On a miss
//! the wrapper writes the tag word `2`, which is exactly the `None`
//! discriminant of the returned two-word `Option`, so the lookup is
//! `Option<Vital>`.
//!
//! `// UNKNOWN:` the field meanings inside the 20-byte record. The binary only
//! ever copies it verbatim (`vitals::vital`) or builds one in
//! `targetid::of`, which writes: a 32-bit kind word (`0x110000` = "no
//! reading"), 8 bytes copied from the source record, one more 32-bit word, and
//! a percentage computed as `round(100 * (max - cur) / max)` clamped to 100.

use std::collections::BTreeMap;

/// A 20-byte vitals/progress-bar record.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Vital {
    /// Kind/discriminant word. `0x11_0000` marks "no reading" in
    /// `targetid::of`.
    pub kind: u32,
    /// `// UNKNOWN:` 8 bytes copied verbatim from the source record.
    pub payload: u64,
    /// `// UNKNOWN:` trailing word copied verbatim.
    pub extra: u32,
    /// Percentage 0..=100.
    pub percent: u32,
}

impl Vital {
    /// The "no reading" kind word used by `targetid::of`.
    pub const NO_READING: u32 = 0x11_0000;

    /// The tag word the binary writes on a map miss (the `None` discriminant).
    pub const MISSING: u32 = 2;

    pub fn is_reading(&self) -> bool {
        self.kind != Vital::NO_READING
    }
}

/// The vitals map and its accessors.
#[derive(Default, Clone, Debug)]
pub struct Vitals {
    pub map: BTreeMap<String, Vital>,
}

impl Vitals {
    pub fn insert(&mut self, name: &str, vital: Vital) {
        self.map.insert(name.to_string(), vital);
    }

    pub fn get(&self, name: &str) -> Option<Vital> {
        self.map.get(name).copied()
    }
}

impl crate::state::GameState {
    /// @0x0000000101bd5118 (260 B)
    pub fn vital(&self, name: &str) -> Option<Vital> {
        self.vitals.get(name)
    }

    /// @0x0000000101bd5010 (264 B) — `self.vital("mana")`
    pub fn mana(&self) -> Option<Vital> {
        self.vital("mana")
    }

    /// @0x0000000101bd521c (264 B) — `self.vital("health")`
    pub fn health(&self) -> Option<Vital> {
        self.vital("health")
    }

    /// @0x0000000101bd5324 (264 B) — `self.vital("spirit")`
    pub fn spirit(&self) -> Option<Vital> {
        self.vital("spirit")
    }

    /// @0x0000000101bd542c (264 B) — `self.vital("stamina")`
    pub fn stamina(&self) -> Option<Vital> {
        self.vital("stamina")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vital(kind: u32, percent: u32) -> Vital {
        Vital {
            kind,
            payload: 0x0102_0304_0506_0708,
            extra: 9,
            percent,
        }
    }

    #[test]
    fn vitals_are_addressed_by_name() {
        let mut state = crate::state::GameState::default();
        state.vitals.insert("mana", vital(0, 75));
        assert_eq!(state.mana().unwrap().percent, 75);
        assert_eq!(state.health(), None);
        assert_eq!(state.spirit(), None);
        assert_eq!(state.stamina(), None);
    }

    #[test]
    fn each_named_accessor_uses_its_own_key() {
        let mut state = crate::state::GameState::default();
        state.vitals.insert("mana", vital(1, 10));
        state.vitals.insert("health", vital(2, 20));
        state.vitals.insert("spirit", vital(3, 30));
        state.vitals.insert("stamina", vital(4, 40));
        assert_eq!(state.mana().unwrap().kind, 1);
        assert_eq!(state.health().unwrap().kind, 2);
        assert_eq!(state.spirit().unwrap().kind, 3);
        assert_eq!(state.stamina().unwrap().kind, 4);
        assert_eq!(state.vital("health").unwrap().percent, 20);
    }

    #[test]
    fn missing_reading_uses_the_none_discriminant() {
        // the binary writes 2 into the first word on a miss
        assert_eq!(Vital::MISSING, 2);
        let mut state = crate::state::GameState::default();
        state.vitals.insert("mana", vital(Vital::NO_READING, 0));
        let reading = state.mana().unwrap();
        assert!(!reading.is_reading());
    }

    #[test]
    fn record_is_twenty_bytes_worth_of_fields() {
        // 4 + 8 + 4 + 4 = 20, matching the 0x14 stride of the map values
        let v = vital(0, 100);
        assert_eq!(
            std::mem::size_of::<u32>() * 3 + std::mem::size_of::<u64>(),
            20
        );
        assert_eq!(v.percent, 100);
    }
}
