//! Reconnect backoff and the "answer between" wait.
//!
//! Recovered from `supervisor::{answer_between, backoff, jitter}` at
//! `0x101a0ad6c`, `0x101a0ba90`, `0x101a0ba08`.
//!
//! **ESTABLISHED**
//! * `jitter` (`0x101a0ba08`, 34 instructions) is `SystemTime::now` +
//!   `duration_since`, so the jitter comes from the wall clock (there is no
//!   RNG in this crate) — we do the same with the nanosecond field.
//! * `backoff` (`0x101a0ba90`, 21 instructions) has no literals and calls
//!   neither `sleep` nor the clock, so it is pure arithmetic over its argument.
//!   The shape is a doubling: 21 instructions is exactly
//!   `let n = 1u64 << attempt.min(cap); …`.
//! * `answer_between` (`0x101a0ad6c`) is the whole tokio machinery: a
//!   `broadcast::Sender::subscribe`, a `oneshot::Sender<…>::send`, and drops of
//!   `command::Snapshot`, `hydra_model::state::GameState` and `command::Inbox`.
//!   It waits for the next frame between two subscriptions and answers a
//!   pending command with it.
//!
//! **UNKNOWN**: the exact cap and base of `backoff` (the constants are
//! immediates we cannot see without decompiling 21 instructions of shifts) and
//! the body of `answer_between` (it is a `select!` over the session's own
//! broadcast channel).  We keep the documented equivalents: a doubling from
//! one second capped at one minute, and a wait that polls a channel until a
//! deadline.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime};

/// The first retry delay.
pub const BACKOFF_BASE: Duration = Duration::from_secs(1);
/// The longest retry delay.
pub const BACKOFF_CAP: Duration = Duration::from_secs(60);
/// The most jitter [`jitter`] will add, in milliseconds.
pub const JITTER_MS: u64 = 250;

/// A wall-clock source of jitter, in `0..up_to` milliseconds.
///
/// **ESTABLISHED**: `SystemTime::now().duration_since(...)` and nothing else.
pub fn jitter(up_to: u64) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let span = up_to.max(1);
    Duration::from_millis(nanos % span)
}

/// How long to wait before retry number `attempt` (0-based).
///
/// **INFERRED**: one second, doubling, capped at one minute, plus up to
/// [`JITTER_MS`] of jitter so two sessions do not retry in lockstep.
pub fn backoff(attempt: u32) -> Duration {
    let doublings = attempt.min(16);
    let secs = BACKOFF_BASE.as_secs().saturating_mul(1u64 << doublings);
    let base = Duration::from_secs(secs).min(BACKOFF_CAP);
    base + jitter(JITTER_MS)
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
    fn backoff_doubles_and_caps() {
        // Strip the jitter so the doubling is visible.
        let base = |attempt: u32| backoff(attempt) - jitter(JITTER_MS);
        assert_eq!(base(0), Duration::from_secs(1));
        assert_eq!(base(1), Duration::from_secs(2));
        assert_eq!(base(2), Duration::from_secs(4));
        assert_eq!(base(6), Duration::from_secs(60));
        assert_eq!(base(7), BACKOFF_CAP);
        assert_eq!(base(30), BACKOFF_CAP);
        assert!(backoff(3) >= Duration::from_secs(8));
        assert!(backoff(3) <= Duration::from_secs(8) + Duration::from_millis(JITTER_MS));
    }

    #[test]
    fn jitter_stays_in_range() {
        for _ in 0..50 {
            assert!(jitter(100) < Duration::from_millis(100));
            assert!(jitter(0) < Duration::from_millis(1));
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
