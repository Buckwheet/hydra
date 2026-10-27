//! The game clock.
//!
//! Recovered from `state::clock` / `state::stun` (each accessor is 64–164 bytes
//! of the same shape). Field offsets inside `GameState` were read directly:
//!
//! | offset  | field                                     |
//! |---------|-------------------------------------------|
//! | `+0x1118` | roundtime flags / active bit            |
//! | `+0x111c` | roundtime duration (seconds)            |
//! | `+0x1120` | casttime flags / active bit             |
//! | `+0x1124` | casttime duration (seconds)             |
//! | `+0x1128` | stun flags / active bit                 |
//! | `+0x112c` | stun duration (seconds)                 |
//! | `+0x1130` | `armed` (used as the `Option` tag)      |
//! | `+0x1134` | `since`: game time (s) at the anchor    |
//! | `+0x1140` | `epoch`: `Option<Instant>` (16 bytes)   |
//!
//! The anchor is an `Option<Instant>` whose niche is a `nanos` field of
//! `1_000_000_000` (`mov w9, #0xca00; movk w9, #0x3b9a, lsl #16`), so the
//! `None` test is `ldr w8, [x0, #0x1148]; cmp w8, #0x3b9aca00`.
//!
//! `elapsed` is `Instant::elapsed().as_secs()` saturated at `u64::MAX - 1` and
//! truncated to `u32` (`mov w8, #-1; cmp x0, x8; csel x8, x0, x8, lo`).
//!
//! Every accessor returns `Option<..>`: when the clock is not armed the binary
//! returns the `Option` `None` tag (`mov w0, #0` for `Option<u32>`,
//! `mov w0, #2` for `Option<bool>`).

use std::time::{Duration, Instant};

/// One pending timer: `active` flag plus a duration.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deadline {
    /// `// UNKNOWN:` the binary keeps a 32-bit flags word and tests bit 0 here
    /// but `!= 0` in `*_remaining_after`; modelled as a plain flag.
    pub active: bool,
    /// Duration in seconds.
    pub seconds: u32,
}

impl Deadline {
    pub fn new(seconds: u32) -> Self {
        Deadline {
            active: true,
            seconds,
        }
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.seconds = 0;
    }
}

/// The recovered clock block of [`crate::state::GameState`].
#[derive(Default, Clone, Debug)]
pub struct Clock {
    pub roundtime: Deadline,
    pub casttime: Deadline,
    pub stun: Deadline,
    /// The game-time anchor is valid.
    pub armed: bool,
    /// Game time in seconds at the anchor.
    pub since: u32,
    /// Wall-clock anchor; `None` means "no reading yet".
    pub epoch: Option<Instant>,
}

impl Clock {
    /// Seconds since the anchor (0 when unset).
    pub fn elapsed(&self) -> u32 {
        match self.epoch {
            None => 0,
            Some(instant) => {
                let secs = instant.elapsed().as_secs();
                if secs > u64::MAX - 1 {
                    (u64::MAX - 1) as u32
                } else {
                    secs as u32
                }
            }
        }
    }

    /// Game time (seconds) now, or `None` when the clock is not armed.
    pub fn game_time_now(&self) -> Option<u32> {
        if self.armed {
            Some(self.since.saturating_add(self.elapsed()))
        } else {
            None
        }
    }

    fn remaining(&self, deadline: &Deadline) -> Option<u32> {
        if !self.armed {
            return None;
        }
        let now = self.since.saturating_add(self.elapsed());
        Some(if deadline.active {
            deadline.seconds.saturating_sub(now)
        } else {
            0
        })
    }

    fn remaining_after(&self, deadline: &Deadline, after: u32) -> Option<u32> {
        if !self.armed {
            return None;
        }
        let now = self.since.saturating_add(after);
        Some(if deadline.active {
            deadline.seconds.saturating_sub(now)
        } else {
            0
        })
    }

    fn running(&self, deadline: &Deadline) -> Option<bool> {
        if !self.armed {
            return None;
        }
        if !deadline.active {
            return Some(false);
        }
        let now = self.since.saturating_add(self.elapsed());
        Some(now < deadline.seconds)
    }

    /// Seconds until the next whole second of wall time, at least 0.01 s.
    ///
    /// Recovered arithmetic: `1.0 - fract(elapsed)` then `fmaxnm(0.01)`, fed
    /// through `Duration::from_secs_f64` (`ucvtf`/`frintz`/`fdiv 1e9`).
    pub fn until_next_second(&self) -> Option<Duration> {
        let instant = self.epoch?;
        let elapsed = instant.elapsed();
        let seconds = elapsed.as_secs_f64();
        let mut until = 1.0 - (seconds - seconds.trunc());
        if until < 0.01 {
            until = 0.01;
        }
        Some(Duration::from_secs_f64(until))
    }
}

impl crate::state::GameState {
    /// @0x0000000101bcd3e0 (164 B)
    pub fn casttime_remaining(&self) -> Option<u32> {
        self.clock.remaining(&self.clock.casttime)
    }

    /// @0x0000000101bcd528 (64 B, recovered exactly):
    /// `armed ? Some(active ? dur - (since + after) : 0) : None`
    pub fn casttime_remaining_after(&self, after: u32) -> Option<u32> {
        self.clock.remaining_after(&self.clock.casttime, after)
    }

    /// @0x0000000101bcd17c (112 B): `armed ? Some(since + elapsed) : None`
    pub fn game_time_now(&self) -> Option<u32> {
        self.clock.game_time_now()
    }

    /// @0x0000000101bcd034 (164 B) — `Option<bool>`: `2` is the `None` tag.
    pub fn in_casttime(&self) -> Option<bool> {
        self.clock.running(&self.clock.casttime)
    }

    /// @0x0000000101bcd0d8 (164 B)
    pub fn in_roundtime(&self) -> Option<bool> {
        self.clock.running(&self.clock.roundtime)
    }

    /// @0x0000000101bcd484 (164 B)
    pub fn roundtime_remaining(&self) -> Option<u32> {
        self.clock.remaining(&self.clock.roundtime)
    }

    /// @0x0000000101bcd568 (64 B)
    pub fn roundtime_remaining_after(&self, after: u32) -> Option<u32> {
        self.clock.remaining_after(&self.clock.roundtime, after)
    }

    /// @0x0000000101bcd1ec (500 B)
    pub fn until_next_second(&self) -> Option<Duration> {
        self.clock.until_next_second()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed() -> crate::state::GameState {
        let mut state = crate::state::GameState::default();
        state.clock.armed = true;
        state.clock.since = 0;
        state.clock.epoch = Some(Instant::now());
        state
    }

    #[test]
    fn unarmed_clock_has_no_readings() {
        let state = crate::state::GameState::default();
        assert_eq!(state.game_time_now(), None);
        assert_eq!(state.in_roundtime(), None);
        assert_eq!(state.roundtime_remaining(), None);
        assert_eq!(state.in_casttime(), None);
        assert_eq!(state.casttime_remaining(), None);
        assert_eq!(state.stun_remaining(), None);
        assert_eq!(state.until_next_second(), None);
    }

    #[test]
    fn game_time_advances_with_wall_clock() {
        let state = armed();
        let now = state.game_time_now().unwrap();
        assert!(now <= 1, "{now}");
    }

    #[test]
    fn roundtime_is_active_until_its_duration() {
        let mut state = armed();
        state.clock.roundtime = Deadline::new(5);
        assert_eq!(state.in_roundtime(), Some(true));
        assert!(state.roundtime_remaining().unwrap() <= 5);
        assert_eq!(state.roundtime_remaining_after(10), Some(0));
        assert_eq!(state.roundtime_remaining_after(1), Some(4));
    }

    #[test]
    fn inactive_deadline_reads_zero_not_active() {
        let mut state = armed();
        state.clock.casttime = Deadline::default();
        assert_eq!(state.in_casttime(), Some(false));
        assert_eq!(state.casttime_remaining(), Some(0));
        assert_eq!(state.casttime_remaining_after(7), Some(0));
    }

    #[test]
    fn stun_uses_its_own_duration() {
        let mut state = armed();
        state.clock.stun = Deadline::new(9);
        assert_eq!(state.stun_remaining(), Some(9));
        state.clock.stun.clear();
        assert_eq!(state.stun_remaining(), Some(0));
    }

    #[test]
    fn until_next_second_is_at_least_ten_milliseconds() {
        let state = armed();
        let wait = state.until_next_second().unwrap();
        assert!(wait >= Duration::from_millis(10), "{wait:?}");
        assert!(wait <= Duration::from_secs(1), "{wait:?}");
    }

    #[test]
    fn deadlines_saturate() {
        let mut state = armed();
        state.clock.roundtime = Deadline::new(3);
        // a far-future "after" consumes the whole timer
        assert_eq!(state.roundtime_remaining_after(u32::MAX), Some(0));
    }
}
