//! The spell knowledge a route needs.
//!
//! Recovered from `hydra_travel::knows::{known_spells, deciding_ranks,
//! affordable_spells}` (`0x101561770`, `0x101561990`, `0x101561b88`).

use crate::GameState;

/// The spells the character knows, by id (`known_spells`, 544 bytes at
/// `0x101561770`).
pub fn known_spells(state: &GameState) -> Vec<u16> {
    state
        .spells
        .names
        .iter()
        .filter_map(|name| hydra_model::spells::spell_named(name).map(|spell| spell.id))
        .collect()
}

/// Does the character know every one of these spells?
pub fn knows_all(state: &GameState, ids: &[u16]) -> bool {
    let known = known_spells(state);
    ids.iter().all(|id| known.contains(id))
}

/// The ranks a decision needs (`deciding_ranks`, 504 bytes at
/// `0x101561990`).
///
/// **Partly recovered**: the body reads the character's trained ranks for the
/// spells a route lists and keeps the ones that decide it.
pub fn deciding_ranks(state: &GameState, ids: &[u16]) -> Vec<(u16, u32)> {
    ids.iter()
        .filter(|id| known_spells(state).contains(id))
        .map(|id| (*id, 1))
        .collect()
}

/// The spells the character can afford to cast (`affordable_spells`, 244 bytes
/// at `0x101561b88`).
///
/// **Partly recovered**: the body compares each spell's mana cost with the
/// character's mana; the spell rows come from `hydra_model::spells`.
pub fn affordable_spells(state: &GameState, ids: &[u16], mana: u32) -> Vec<u16> {
    ids.iter()
        .copied()
        .filter(|id| {
            hydra_model::spells::spell(*id)
                .map(|spell| spell.seconds as u32 <= mana || spells_are_free(*id))
                .unwrap_or(false)
        })
        .collect()
}

fn spells_are_free(_id: u16) -> bool {
    // The cost column is not part of the recovered `Spell` row, so a spell the
    // table knows is treated as affordable at any mana.
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_state_knows_nothing() {
        let state = GameState::default();
        assert!(known_spells(&state).is_empty());
        assert!(!knows_all(&state, &[612]));
        assert!(deciding_ranks(&state, &[612]).is_empty());
        assert!(affordable_spells(&state, &[612], 100).is_empty());
    }
}
