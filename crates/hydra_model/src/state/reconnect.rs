//! Reconnect invalidation.
//!
//! `GameState::invalidate_for_reconnect` (9596 B at `0x101bd8dc8`) clears the
//! state that a reconnect makes stale. Its `bl` targets are
//! `ship::Ship::lapse`, `BountyStatus::clear`, `StatusInfo::forget` and
//! `CombatTracker::invalidate_for_reconnect`; the recovered literals are the
//! status words (`bound`, `calmed`, `cutthroat`, `silenced`, `sleeping`,
//! `thorned`) and the tracker field names.

/// The status flags a reconnect forgets (recovered literals).
pub const STALE_STATUS: [&str; 6] = ["bound", "calmed", "cutthroat", "silenced", "sleeping", "thorned"];

impl crate::state::GameState {
    /// @0x0000000101bd8dc8 (9596 B)
    pub fn invalidate_for_reconnect(&mut self) {
        self.combat.invalidate_for_reconnect();
        for status in STALE_STATUS {
            self.status.forget(status);
        }
        self.bounty.clear();
        self.ship.lapse = None;
        self.targeting.selected = None;
        self.creatures.in_room.clear();
        self.timers.timers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_forgets_the_recovered_status_words() {
        let mut state = crate::state::GameState::default();
        for status in STALE_STATUS {
            state.status.set(status, true);
        }
        state.status.set("dead", true);
        state.invalidate_for_reconnect();
        for status in STALE_STATUS {
            assert!(!state.status.is_known(status), "{status} should be forgotten");
        }
        // flags outside the recovered list survive
        assert!(state.status.dead());
    }

    #[test]
    fn reconnect_clears_bounty_timers_and_combat() {
        let mut state = crate::state::GameState::default();
        state.combat.active_assault = Some("fury".into());
        state.timers.insert("haste", 30);
        state.bounty.read_line("You already have a task.");
        state.invalidate_for_reconnect();
        assert!(!state.combat.is_active());
        assert!(state.timers.timers.is_empty());
        assert_eq!(state.bounty.refusal, None);
    }
}
