//! Reconnect backoff and the "answer between" wait.
//!
//! Recovered from `supervisor::{answer_between, backoff, jitter}` at
//! `0x101a0ad6c`, `0x101a0ba90`, `0x101a0ba08`.
//!
//! **ESTABLISHED**
//! * `jitter` (`0x101a0ba08`) is `SystemTime::now` + `duration_since` and
//!   returns an **`f64` fraction**, not a duration: the nanosecond field is
//!   divided by 1000 (`umull` by `0x10624dd3`, `lsr #38`), reduced modulo 1000
//!   (`umull` by `0x418938`, `msub`), and divided by `999.0` (the `f64` at
//!   `0x101a0ba68`), so the result is `0.0..=1.0` at microsecond resolution; a
//!   clock error yields `0.0` (`fcsel` on the `Result` discriminant).
//! * `backoff` (`0x101a0ba90`) takes `(attempt: u32, jitter: f64)` and is pure
//!   arithmetic: `table[min(attempt, 4)] × 1000` milliseconds, scaled by
//!   `0.8 + 0.4 × clamp(jitter, 0.0, 1.0)`, then truncated to whole
//!   milliseconds (`fcvtzu`) and split into `Duration { secs, nanos }`.  The
//!   table at `0x1039c5c60` is five `u64`s — `1, 2, 5, 10, 30` seconds, *not* a
//!   doubling — and the two `f64` constants at `0x101a0bad8` / `0x101a0baec`
//!   are `0.4` and `0.8`.
//! * `SupervisedSession::wait_before_retry` calls them back to back
//!   (`0x1000089cc` `bl jitter`, `0x1000089d4` `bl backoff`, with `d0` passed
//!   straight through), so the recovered composition is
//!   `backoff(attempt, jitter())`.
//! * `answer_between` (`0x101a0ad6c`) is the whole tokio machinery: a
//!   `broadcast::Sender::subscribe`, a `oneshot::Sender<…>::send`, and drops of
//!   `command::Snapshot`, `hydra_model::state::GameState` and `command::Inbox`.
//!   It waits for the next frame between two subscriptions and answers a
//!   pending command with it.
//!
//! [`backoff`] keeps the crate's one-argument signature (it calls the recovered
//! jitter itself) and [`jitter`] keeps its `Duration`-shaped signature even
//! though the original returns the bare fraction; both use the recovered
//! bodies.
//!
//! **UNKNOWN**: the body of `answer_between` (it is a `select!` over the
//! session's own broadcast channel).  We keep the documented equivalent: a wait
//! that polls a channel until a deadline.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime};

/// The first retry delay (**ESTABLISHED**: the first table entry).
pub const BACKOFF_BASE: Duration = Duration::from_secs(1);
/// The longest retry delay (**ESTABLISHED**: the last table entry).
pub const BACKOFF_CAP: Duration = Duration::from_secs(30);
/// Unused by [`backoff`]: the recovered jitter is multiplicative (80 %–120 % of
/// the step), not an additive number of milliseconds.  Kept so the crate's
/// public surface does not change.
pub const JITTER_MS: u64 = 250;

/// The recovered retry table (`0x1039c5c60`), in seconds.
const BACKOFF_STEPS: [u64; 5] = [1, 2, 5, 10, 30];
/// The jitter share added on top of [`JITTER_FLOOR`] (`0.4` at `0x101a0bad8`).
const JITTER_SPAN: f64 = 0.4;
/// The smallest share of a step [`backoff`] waits (`0.8` at `0x101a0baec`).
const JITTER_FLOOR: f64 = 0.8;

/// The recovered `supervisor::jitter`: a wall-clock fraction in `0.0..=1.0`.
///
/// **ESTABLISHED** (`0x101a0ba08`): the sub-millisecond part of the clock,
/// `(subsec_nanos / 1000 % 1000) / 999.0`, and `0.0` when the clock cannot be
/// read.
fn jitter_fraction() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let micros = (nanos / 1000) % 1000;
    micros as f64 / 999.0
}

/// A wall-clock source of jitter, in `0..=up_to` milliseconds.
///
/// **ESTABLISHED**: the fraction comes from
/// `SystemTime::now().duration_since(...)` and nothing else.  The original
/// returns the bare `f64` (see [`backoff`]); this wrapper keeps the crate's
/// `Duration`-shaped signature.
pub fn jitter(up_to: u64) -> Duration {
    let millis = jitter_fraction() * up_to as f64;
    Duration::from_millis(millis as u64)
}

/// How long to wait before retry number `attempt` (0-based).
///
/// **ESTABLISHED** (`0x101a0ba90`): the recovered table `[1, 2, 5, 10, 30]`
/// seconds, indexed by `min(attempt, 4)`, scaled by `0.8 + 0.4 × jitter()` and
/// truncated to whole milliseconds.  The jitter is multiplicative, so a retry
/// waits between 80 % and 120 % of its step.
pub fn backoff(attempt: u32) -> Duration {
    let secs = BACKOFF_STEPS[attempt.min(4) as usize];
    let millis = secs as f64 * 1000.0 * (JITTER_FLOOR + JITTER_SPAN * jitter_fraction());
    Duration::from_millis(millis as u64)
}

/// Wait for the first message that arrives before `within` elapses.
///
/// This is the std-only stand-in for `answer_between`: the original subscribes
/// to the session's broadcast channel, waits for the next frame *and* for the
/// game state to settle, and then answers the pending command.  A synchronous
/// equivalent is "take the next value from the receiver unless the deadline
/// passes first".
///
/// **UNKNOWN**: the original's exact combination of the frame stream, the
/// game-state snapshot, the inbox and the cancellation token.
pub fn answer_between<T>(rx: &Receiver<T>, within: Duration) -> Option<T> {
    let deadline = Instant::now() + within;
    loop {
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(value) => return Some(value),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn backoff_follows_the_recovered_table() {
        // The table at 0x1039c5c60 is 1, 2, 5, 10, 30 seconds; the jitter is
        // multiplicative, so every step lands in 80 %..=120 % of its entry.
        let steps = [1u64, 2, 5, 10, 30];
        for (attempt, secs) in steps.iter().enumerate() {
            let low = Duration::from_millis(secs * 800);
            let high = Duration::from_millis(secs * 1200);
            for _ in 0..20 {
                let got = backoff(attempt as u32);
                assert!(got >= low, "backoff({attempt}) = {got:?} < {low:?}");
                assert!(got <= high, "backoff({attempt}) = {got:?} > {high:?}");
            }
        }
        // The index is clamped at 4: everything past the table is 30 s.
        for attempt in [5u32, 9, 30] {
            let got = backoff(attempt);
            assert!(got >= Duration::from_millis(24_000), "{got:?}");
            assert!(got <= Duration::from_millis(36_000), "{got:?}");
        }
        assert_eq!(BACKOFF_STEPS, steps);
        assert_eq!(BACKOFF_BASE, Duration::from_secs(1));
        assert_eq!(BACKOFF_CAP, Duration::from_secs(30));
        assert_eq!((JITTER_FLOOR, JITTER_SPAN), (0.8, 0.4));
    }

    #[test]
    fn jitter_stays_in_range() {
        for _ in 0..50 {
            let fraction = jitter_fraction();
            assert!((0.0..=1.0).contains(&fraction), "{fraction}");
            assert!(jitter(100) <= Duration::from_millis(100));
            assert_eq!(jitter(0), Duration::ZERO);
        }
    }

    #[test]
    fn answer_between_returns_what_arrives_first() {
        let (tx, rx) = channel();
        tx.send("frame".to_string()).unwrap();
        assert_eq!(answer_between(&rx, Duration::from_millis(50)), Some("frame".to_string()));
        let (tx2, rx2) = channel::<String>();
        drop(tx2);
        assert_eq!(answer_between(&rx2, Duration::from_millis(50)), None);
    }

    #[test]
    fn answer_between_gives_up_at_the_deadline() {
        let (_tx, rx) = channel::<String>();
        let started = Instant::now();
        assert_eq!(answer_between(&rx, Duration::from_millis(20)), None);
        assert!(started.elapsed() >= Duration::from_millis(20));
    }
}
