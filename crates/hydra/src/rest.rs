//! The resting state: what `hydra serve` is doing, and the notice it says when
//! the game drops.
//!
//! Recovered from `hydra::rest::doing` (`0x10021e474`) and `hydra::rest::watch`
//! (the 5 s interval that logs `[hydra] memory: {doing}, {n} MB in RAM` when
//! things change or after about a minute).  The reference build's `serve` is the
//! GUI/resting window, so these are the words it used while it rested; the
//! headless `serve` prints them instead of drawing them.
//!
//! # What is recovered and what is not
//!
//! **ESTABLISHED** (`rest::doing`): the status line is composed from three
//! pieces, and all three are literals in the binary's string table —
//!
//! | piece | one | many / none |
//! | --- | --- | --- |
//! | the characters playing | `1 character playing` | `{n} characters playing`, `no character playing` |
//! | the map | `, the map laid out {n} times` | — |
//! | the windows | `the windows open` | `no window open` |
//!
//! **ESTABLISHED** (`rest::watch`): the log line is
//! `[hydra] memory: {doing}, {n} MB in RAM`, said on a 5 s interval when the
//! composition changes, and again about every 60 s so a resting run leaves a
//! heartbeat.  **UNKNOWN / not reconstructed:** the `{n}` — the reference build
//! reads it from a `memory_stats` dependency that is not in this workspace, so
//! [`memory_line`] takes the number as an argument and this crate passes `None`,
//! which says `RAM unknown` rather than inventing a figure.
//!
//! **ESTABLISHED** ([`DROPPED`]): the exact notice a reconnecting session says,
//! `\n\n[hydra] the connection dropped; reconnecting\n\n`.
//!
//! **INFERRED:** what counts as "a character playing" here.  The headless build
//! plays exactly the one character the command named, so `serve` passes `1`
//! while the game connection is up and `0` while it is not; there is no map and
//! no window in this build, so those two are always `0`.

use std::time::{Duration, Instant};

/// The notice a resting run says when the game connection drops.
///
/// **RECOVERED** literal, byte for byte (the leading and trailing blank lines
/// are part of it).
pub const DROPPED: &str = "\n\n[hydra] the connection dropped; reconnecting\n\n";

/// How often `rest::watch` looks at the session (`rest::watch`'s interval).
pub const WATCH_INTERVAL: Duration = Duration::from_secs(5);

/// How long a resting run may go without saying anything: after this, the
/// status is logged even when nothing changed.
///
/// **ESTABLISHED** shape ("or after ~60 s"): the reference build's own bound was
/// not decoded to the second, so this is its round number.
pub const WATCH_REPEAT: Duration = Duration::from_secs(60);

/// The recovered `rest::doing`: how many characters play, how many times the map
/// is laid out, and how many windows are open.
///
/// The three pieces are joined with `, ` in the recovered order.
pub fn doing(playing: usize, laid_out: usize, windows: usize) -> String {
    format!(
        "{}, the map laid out {laid_out} times, {}",
        playing_words(playing),
        windows_words(windows)
    )
}

/// The first piece: `no character playing`, `1 character playing`,
/// `{n} characters playing`.
fn playing_words(playing: usize) -> String {
    match playing {
        0 => "no character playing".to_string(),
        1 => "1 character playing".to_string(),
        n => format!("{n} characters playing"),
    }
}

/// The last piece: `no window open` or `the windows open`.
///
/// **ESTABLISHED** words; whether the recovered composition counted windows
/// (rather than saying "some window is open") is **UNKNOWN** — the two literals
/// are all the build settles, so the count is not printed.
fn windows_words(windows: usize) -> &'static str {
    if windows == 0 {
        "no window open"
    } else {
        "the windows open"
    }
}

/// The recovered `rest::watch` line, `[hydra] memory: {doing}, {n} MB in RAM`.
///
/// `megabytes` is `None` here: the memory figure needs a `memory_stats`
/// dependency this workspace does not have, so it is reported as unknown rather
/// than guessed ([`MEMORY_UNKNOWN`]).
pub fn memory_line(doing: &str, megabytes: Option<u64>) -> String {
    match megabytes {
        Some(mb) => format!("[hydra] memory: {doing}, {mb} MB in RAM"),
        None => format!("[hydra] memory: {doing}, {MEMORY_UNKNOWN}"),
    }
}

/// What [`memory_line`] says in place of the figure it cannot measure.
pub const MEMORY_UNKNOWN: &str = "RAM unknown";

/// The recovered `rest::watch`: a timer that decides when the resting status is
/// worth saying again.
///
/// It never blocks and never sleeps: a relay loop calls [`Watch::poll`] on each
/// of its own passes, and only every [`WATCH_INTERVAL`] does that cost anything.
#[derive(Debug, Clone)]
pub struct Watch {
    /// the last line this watch said
    last: Option<String>,
    /// when it said it
    said_at: Option<Instant>,
    /// the next moment it may say anything
    next: Option<Instant>,
}

impl Watch {
    /// A watch that says the first status it is asked for.
    pub fn new() -> Watch {
        Watch {
            last: None,
            said_at: None,
            next: None,
        }
    }

    /// One tick: the line to log, if one is due now.
    pub fn poll(&mut self, doing: &str) -> Option<String> {
        self.poll_at(Instant::now(), doing)
    }

    /// [`Watch::poll`] at a given instant, so the cadence is testable without
    /// waiting for it.
    pub fn poll_at(&mut self, now: Instant, doing: &str) -> Option<String> {
        if let Some(next) = self.next {
            if now < next {
                return None;
            }
        }
        self.next = Some(now + WATCH_INTERVAL);
        let line = memory_line(doing, None);
        let changed = self.last.as_deref() != Some(line.as_str());
        let stale = match self.said_at {
            Some(at) => now.duration_since(at) >= WATCH_REPEAT,
            None => true,
        };
        if !changed && !stale {
            return None;
        }
        self.last = Some(line.clone());
        self.said_at = Some(now);
        Some(line)
    }
}

impl Default for Watch {
    fn default() -> Self {
        Watch::new()
    }
}

/// A watch together with the composition it repeats: what a relay loop holds
/// when the session it is relaying is a resting one.
#[derive(Debug, Clone)]
pub struct Status {
    watch: Watch,
    doing: String,
}

impl Status {
    /// A status for the composition `doing` (built with [`doing`]).
    pub fn new(doing: impl Into<String>) -> Status {
        Status {
            watch: Watch::new(),
            doing: doing.into(),
        }
    }

    /// Change what the status says (the game connection came or went).
    pub fn set(&mut self, doing: impl Into<String>) {
        self.doing = doing.into();
    }

    /// The composition this status repeats.
    pub fn doing(&self) -> &str {
        &self.doing
    }

    /// One tick of the relay loop: the line to log, if one is due.
    ///
    /// Called on every pass of the relay loop, so it does no work — and no
    /// allocation — until the watch says a line is due.
    pub fn poll(&mut self) -> Option<String> {
        let Status { watch, doing } = self;
        watch.poll(doing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_composition_is_the_recovered_one() {
        assert_eq!(
            doing(0, 0, 0),
            "no character playing, the map laid out 0 times, no window open"
        );
        assert_eq!(
            doing(1, 0, 1),
            "1 character playing, the map laid out 0 times, the windows open"
        );
        assert_eq!(
            doing(2, 3, 0),
            "2 characters playing, the map laid out 3 times, no window open"
        );
        // The recovered words themselves.
        assert_eq!(playing_words(0), "no character playing");
        assert_eq!(playing_words(1), "1 character playing");
        assert_eq!(playing_words(7), "7 characters playing");
        assert_eq!(windows_words(0), "no window open");
        assert_eq!(windows_words(2), "the windows open");
    }

    #[test]
    fn the_memory_line_says_the_figure_or_that_it_is_unknown() {
        // The recovered template, with a figure.
        assert_eq!(
            memory_line("1 character playing", Some(211)),
            "[hydra] memory: 1 character playing, 211 MB in RAM"
        );
        // Without one: no invented number.
        let line = memory_line("1 character playing", None);
        assert_eq!(line, "[hydra] memory: 1 character playing, RAM unknown");
        assert!(line.contains(MEMORY_UNKNOWN));
    }

    #[test]
    fn a_status_carries_its_composition_and_says_it_once() {
        let mut status = Status::new(doing(1, 0, 0));
        assert!(status.doing().contains("1 character playing"));
        let line = status.poll().expect("the first status");
        assert!(line.contains("1 character playing"), "{line}");
        // The next tick is too soon: a `Status` is polled on every pass of the
        // relay loop, so it must be quiet almost always.
        assert_eq!(status.poll(), None);
        status.set(doing(0, 0, 0));
        assert!(status.doing().contains("no character playing"));
    }

    #[test]
    fn the_dropped_notice_is_the_recovered_literal() {
        assert_eq!(DROPPED, "\n\n[hydra] the connection dropped; reconnecting\n\n");
    }

    #[test]
    fn a_watch_says_the_first_line_then_only_changes_or_the_heartbeat() {
        let start = Instant::now();
        let mut watch = Watch::new();
        // The first tick always says something: a resting run has just started.
        let first = watch
            .poll_at(start, &doing(1, 0, 0))
            .expect("the first status");
        assert!(first.contains("1 character playing"), "{first}");

        // Too soon: nothing, even though the composition changed.
        assert_eq!(watch.poll_at(start + Duration::from_secs(1), &doing(0, 0, 0)), None);
        // A whole interval later, with a different composition: the new line.
        let changed = watch
            .poll_at(start + WATCH_INTERVAL, &doing(0, 0, 0))
            .expect("the changed status");
        assert!(changed.contains("no character playing"), "{changed}");
        // Unchanged, and still inside the next interval: silence.
        assert_eq!(watch.poll_at(start + WATCH_INTERVAL * 2, &doing(0, 0, 0)), None);

        // Unchanged: silence until the heartbeat interval is up.
        let mut watch = Watch::new();
        let base = Instant::now();
        assert!(watch.poll_at(base, "same").is_some());
        let mut at = base;
        loop {
            at += WATCH_INTERVAL;
            match watch.poll_at(at, "same") {
                Some(line) => {
                    assert!(line.contains("same"), "{line}");
                    assert!(
                        at.duration_since(base) >= WATCH_REPEAT,
                        "the heartbeat came after {:?}",
                        at.duration_since(base)
                    );
                    break;
                }
                None => assert!(
                    at.duration_since(base) < WATCH_REPEAT + WATCH_INTERVAL,
                    "no heartbeat within {:?}",
                    WATCH_REPEAT + WATCH_INTERVAL
                ),
            }
        }
        assert_eq!(WATCH_INTERVAL, Duration::from_secs(5));
        assert_eq!(WATCH_REPEAT, Duration::from_secs(60));
    }
}
