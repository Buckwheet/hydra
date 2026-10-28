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
//! that polls a channel until a deadline, and — Phase 1 — a tokio version
//! ([`answer_between_async`]) that uses `tokio::time::timeout` the way the
//! original's `select!` did.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime};

/// The five lifecycle states, in their recovered order.
///
/// **ESTABLISHED** words: `SupervisedSession::transition` (`0x1000089…`)
/// broadcast the state and logged `\nlifecycle {state}`; the five words are in
/// the binary's string table — `Connecting`, `Logging in`, `Connected`,
/// `Reconnecting`, `Closed`.  Which transition happens *where* is
/// **INFERRED** from the three logged lines below (a session logs `Connecting`
/// and `Logging in` before it is `Connected`, `Reconnecting` before
/// [`wait_before_retry`], and `Closed` where it logs `\nsession stopped:
/// {reason}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// about to dial
    Connecting,
    /// dialled, walking the login
    LoggingIn,
    /// the game connection is up
    Connected,
    /// the connection dropped and a retry is pending
    Reconnecting,
    /// the session is over
    Closed,
}

impl Lifecycle {
    /// Every state, in the recovered order.
    pub const ALL: [Lifecycle; 5] = [
        Lifecycle::Connecting,
        Lifecycle::LoggingIn,
        Lifecycle::Connected,
        Lifecycle::Reconnecting,
        Lifecycle::Closed,
    ];

    /// The recovered state word (`Connecting`, `Logging in`, `Connected`,
    /// `Reconnecting`, `Closed`).
    pub fn name(self) -> &'static str {
        match self {
            Lifecycle::Connecting => "Connecting",
            Lifecycle::LoggingIn => "Logging in",
            Lifecycle::Connected => "Connected",
            Lifecycle::Reconnecting => "Reconnecting",
            Lifecycle::Closed => "Closed",
        }
    }
}

/// `\nlifecycle {state}` — the recovered `transition` log line
/// (`SupervisedSession::transition`).
pub fn lifecycle_line(state: Lifecycle) -> String {
    format!("\nlifecycle {}", state.name())
}

/// `reconnecting in {ms}ms (attempt {n})` — the recovered
/// `SupervisedSession::wait_before_retry` line.
///
/// **ESTABLISHED** template.  The attempt number is 1-based here, because the
/// recovered line names the attempt it is about to make (the first retry says
/// `attempt 1`); whether the original counted from 0 is **UNKNOWN**, and it
/// does not matter to [`backoff`], which takes the 0-based *index* it clamps at
/// 4.
pub fn reconnecting_line(wait: Duration, attempt: u32) -> String {
    format!("reconnecting in {}ms (attempt {attempt})", wait.as_millis())
}

/// `\nsession stopped: {reason}` — the recovered `SupervisedSession::finish`
/// line.
pub fn stopped_line(reason: &str) -> String {
    format!("\nsession stopped: {reason}")
}

/// The wait before retry `attempt` (0-based): `backoff(attempt, jitter())`.
///
/// **ESTABLISHED** composition: `wait_before_retry` calls `jitter` and then
/// `backoff` back to back, with `d0` passed straight through — and [`backoff`]
/// already does both (see the module note).
pub fn wait_before_retry(attempt: u32) -> Duration {
    backoff(attempt)
}

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
///
/// **Phase 1**: [`answer_between_async`] is the tokio form of the same wait,
/// built on `tokio::time::timeout` instead of a polling loop.
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

/// Wait for the next value on a tokio channel, giving up after `within`.
///
/// **ESTABLISHED** shape (`0x101a0ad6c`): the binary waits on tokio channels
/// with a deadline (`tokio::select!` over the frame stream, the game-state
/// snapshot and the inbox, plus a `CancellationToken`).  This is the same
/// deadline with `tokio::time::timeout`, and it answers `None` when the
/// deadline passes or the sender goes away — what the recovered `select!`
/// falls through to.
pub async fn answer_between_async<T>(
    rx: &mut tokio::sync::mpsc::Receiver<T>,
    within: Duration,
) -> Option<T> {
    tokio::time::timeout(within, rx.recv()).await.ok().flatten()
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
    fn the_lifecycle_words_are_the_recovered_five() {
        let words: Vec<&str> = Lifecycle::ALL.iter().map(|state| state.name()).collect();
        assert_eq!(
            words,
            vec!["Connecting", "Logging in", "Connected", "Reconnecting", "Closed"]
        );
        // The recovered log line has a leading newline and the state word.
        assert_eq!(lifecycle_line(Lifecycle::Connected), "\nlifecycle Connected");
        // `finish`: `\nsession stopped: {reason}`.
        assert_eq!(stopped_line("quit"), "\nsession stopped: quit");
        // `wait_before_retry`: `reconnecting in {ms}ms (attempt {n})`, with the
        // wait in whole milliseconds and the attempt counted from 1.
        assert_eq!(
            reconnecting_line(Duration::from_millis(934), 1),
            "reconnecting in 934ms (attempt 1)"
        );
        assert_eq!(
            reconnecting_line(Duration::from_millis(1200), 3),
            "reconnecting in 1200ms (attempt 3)"
        );
        // The wait is the recovered backoff: the jitter is multiplicative, so
        // every step lands in 80 %..=120 % of its table entry.
        for attempt in 0..6u32 {
            let wait = wait_before_retry(attempt);
            let step = BACKOFF_STEPS[(attempt.min(4)) as usize] * 1000;
            assert!(wait.as_millis() >= (step * 4 / 5) as u128, "{attempt}: {wait:?}");
            assert!(wait.as_millis() <= (step * 6 / 5) as u128, "{attempt}: {wait:?}");
        }
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

    #[tokio::test]
    async fn answer_between_async_takes_the_first_value_or_times_out() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(4);
        tx.send("frame".to_string()).await.unwrap();
        assert_eq!(
            answer_between_async(&mut rx, Duration::from_millis(50)).await,
            Some("frame".to_string())
        );
        // nothing more arrives: the deadline wins
        let started = Instant::now();
        assert_eq!(
            answer_between_async(&mut rx, Duration::from_millis(20)).await,
            None
        );
        assert!(started.elapsed() >= Duration::from_millis(20));
        // a closed channel is `None` immediately, not a hang
        drop(tx);
        assert_eq!(
            answer_between_async(&mut rx, Duration::from_secs(30)).await,
            None
        );
    }
}
