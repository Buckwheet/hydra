//! Stun timer accessor.
//!
//! `stun_remaining` (164 B at `0x101bccf90`) is the same shape as the other
//! clock readers and is fully recovered:
//!
//! ```text
//! if !self.armed { return None }            // Option tag 0
//! let now = self.since + elapsed();
//! let left = self.stun.seconds.saturating_sub(now);
//! Some(if self.stun.active { left } else { 0 })
//! ```
//!
//! Note the value is `Some(0)` — not `None` — when no stun is recorded but the
//! clock is armed, so callers must test the value, not the `Option`.

use crate::state::clock::Deadline;

impl crate::state::GameState {
    /// @0x0000000101bccf90 (164 B)
    pub fn stun_remaining(&self) -> Option<u32> {
        if !self.clock.armed {
            return None;
        }
        let now = self.clock.since.saturating_add(self.clock.elapsed());
        let left = self.clock.stun.seconds.saturating_sub(now);
        Some(if self.clock.stun.active { left } else { 0 })
    }

    /// Record a stun of `seconds`. Not a recovered symbol: the writer side of
    /// the clock lives in the unnamed `apply` arms; this is the inverse of the
    /// recovered reader.
    pub fn set_stun(&mut self, seconds: u32) {
        self.clock.stun = Deadline::new(seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn state(stun: u32) -> crate::state::GameState {
        let mut state = crate::state::GameState::default();
        state.clock.armed = true;
        state.clock.since = 0;
        state.clock.epoch = Some(Instant::now());
        if stun > 0 {
            state.clock.stun = Deadline::new(stun);
        }
        state
    }

    #[test]
    fn armed_clock_without_stun_reports_zero() {
        assert_eq!(state(0).stun_remaining(), Some(0));
    }

    #[test]
    fn stun_counts_down_from_its_duration() {
        assert_eq!(state(12).stun_remaining(), Some(12));
    }

    #[test]
    fn stun_of_zero_seconds_is_not_active() {
        let mut s = state(0);
        s.clock.stun.active = false;
        assert_eq!(s.stun_remaining(), Some(0));
    }

    #[test]
    fn unarmed_clock_is_none() {
        let s = crate::state::GameState::default();
        assert_eq!(s.stun_remaining(), None);
    }

    #[test]
    fn set_stun_arms_the_timer() {
        let mut s = state(0);
        s.set_stun(6);
        assert_eq!(s.stun_remaining(), Some(6));
    }
}
