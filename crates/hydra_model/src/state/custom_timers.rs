//! Custom timers.
//!
//! Recovered: `CustomTimers::get` (228 B) and `timer` (220 B) are lookups in a
//! `BTreeMap`-shaped store; `read` (196 B) folds a line in via `List::named`
//! (1404 B, builds the name with `format!` and a single space); `GameState::
//! timer_ends_in` (208 B) uses `Instant::elapsed` / `SystemTime::now` +
//! `duration_since`.

use std::collections::BTreeMap;
use std::time::{Instant, SystemTime};

/// One custom timer.
#[derive(Clone, Debug)]
pub struct Timer {
    pub name: String,
    /// Seconds of duration the timer was started with.
    pub duration: u32,
    /// Wall-clock anchor.
    pub started: Option<Instant>,
    /// System-time anchor (used by `timer_ends_in`).
    pub started_at: Option<SystemTime>,
}

impl Default for Timer {
    fn default() -> Self {
        Timer {
            name: String::new(),
            duration: 0,
            started: None,
            started_at: None,
        }
    }
}

impl PartialEq for Timer {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.duration == other.duration
    }
}

impl Eq for Timer {}

/// The timer store.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct CustomTimers {
    pub timers: BTreeMap<String, Timer>,
}

impl CustomTimers {
    /// @0x0000000101b8344c (228 B)
    pub fn get(&self, name: &str) -> Option<&Timer> {
        self.timers.get(name)
    }

    /// @0x0000000101b835f4 (220 B)
    pub fn timer(&self, name: &str) -> Option<&Timer> {
        self.timers.get(name)
    }

    /// @0x0000000101b83530 (196 B)
    pub fn read(&mut self, line: &str) -> bool {
        let Some((name, seconds)) = List::named(line) else {
            return false;
        };
        self.insert(&name, seconds)
    }

    pub fn insert(&mut self, name: &str, seconds: u32) -> bool {
        if let Some(existing) = self.timers.get_mut(name) {
            if existing.duration == seconds {
                return false;
            }
            existing.duration = seconds;
            existing.started = Some(Instant::now());
            existing.started_at = Some(SystemTime::now());
            return true;
        }
        self.timers.insert(
            name.to_string(),
            Timer {
                name: name.to_string(),
                duration: seconds,
                started: Some(Instant::now()),
                started_at: Some(SystemTime::now()),
            },
        );
        true
    }
}

/// A named timer list (the recovered `List` type).
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct List {
    pub name: String,
}

impl List {
    /// @0x0000000101b836d0 (1404 B) — `"<timer name> <seconds>"`.
    pub fn named(line: &str) -> Option<(String, u32)> {
        let line = line.trim();
        let (name, rest) = line.rsplit_once(' ')?;
        let seconds = rest.trim().parse::<u32>().ok()?;
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        Some((name.to_string(), seconds))
    }
}

impl crate::state::GameState {
    /// @0x0000000101bccec0 (208 B) — seconds until a timer expires.
    pub fn timer_ends_in(&self, name: &str) -> Option<u32> {
        let timer = self.timers.get(name)?;
        if let Some(started) = timer.started {
            let elapsed = started.elapsed().as_secs().min(u32::MAX as u64) as u32;
            return Some(timer.duration.saturating_sub(elapsed));
        }
        if let Some(started_at) = timer.started_at {
            if let Ok(elapsed) = SystemTime::now().duration_since(started_at) {
                let elapsed = elapsed.as_secs().min(u32::MAX as u64) as u32;
                return Some(timer.duration.saturating_sub(elapsed));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_named_splits_name_and_seconds() {
        assert_eq!(
            List::named("spell haste 60"),
            Some(("spell haste".to_string(), 60))
        );
        assert_eq!(List::named("60"), None);
        assert_eq!(List::named(" 30"), None);
    }

    #[test]
    fn timers_are_stored_and_replaced() {
        let mut timers = CustomTimers::default();
        assert!(timers.read("haste 60"));
        assert_eq!(timers.get("haste").unwrap().duration, 60);
        assert!(!timers.read("haste 60"));
        assert!(timers.timer("haste").is_some());
        assert!(timers.insert("haste", 90));
        assert_eq!(timers.get("haste").unwrap().duration, 90);
    }

    #[test]
    fn timer_ends_in_counts_down() {
        let mut state = crate::state::GameState::default();
        state.timers.insert("haste", 60);
        let left = state.timer_ends_in("haste").unwrap();
        assert!(left <= 60, "{left}");
        assert_eq!(state.timer_ends_in("missing"), None);
    }

    #[test]
    fn prose_is_ignored() {
        let mut timers = CustomTimers::default();
        assert!(!timers.read("The wind blows."));
    }
}
