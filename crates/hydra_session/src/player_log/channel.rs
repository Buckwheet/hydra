//! The channel between the session and the log writer.
//!
//! Recovered from `player_log::channel::{LogSink, PlayerLog}` at `0x101a873c8`,
//! `0x101a874b8`, `0x101a87834`, `0x101a87a94`, `0x101a87a9c`, `0x101a87aa4`,
//! `0x101a87d3c`, `0x101a87f20`.
//!
//! **ESTABLISHED**:
//! * the channel is **bounded** (`with_capacity`, `0x101a87834`, allocates a
//!   `Semaphore` like the mpsc in `hydra_lich` with capacity 64) and
//!   `new`/`close` are two instructions — `PlayerLog` is one sender plus a
//!   counter;
//! * `dropped` (`0x101a873c8`) is *shared* between `LogSink` and `PlayerLog`
//!   (both addresses are the same function), so both count the same drops;
//! * `try_recv` (`0x101a874b8`) takes one queued line if there is one;
//! * `typed` (`0x101a87aa4`) formats `"{}/{}"` (`0x103e7c12e`) — the typed line
//!   is logged as `<day>/<something>`;
//! * `notice` (`0x101a87d3c`) writes a notice's lines;
//! * `redact` (`0x101a87f20`) installs the redaction list.
//!
//! **std-only departure**: `tokio::sync::mpsc::channel(cap)` becomes
//! `std::sync::mpsc::sync_channel(cap)`, so a full channel blocks the session
//! instead of applying backpressure asynchronously.  `dropped` therefore
//! counts the lines that could not be queued when the channel was closed,
//! which is the only drop a synchronous channel has.

use super::writer::Line;
use crate::notice::Notice;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;

/// How many lines the channel holds by default.  **ESTABLISHED**: the
/// `Semaphore::new(0x40)` in `channel::with_capacity`.
pub const CAPACITY: usize = 64;

/// The counter of lines that never made it into the log.
#[derive(Debug, Clone, Default)]
pub struct Drops(Arc<AtomicU64>);

impl Drops {
    /// How many lines were dropped so far.
    pub fn count(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    fn bump(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// The reading end of the log channel.
#[derive(Debug)]
pub struct LogSink {
    rx: Receiver<LogLine>,
    drops: Drops,
}

/// One queued log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    /// a line as it came off the game stream
    Frame {
        stream: String,
        text: String,
    },
    /// something the player typed
    Typed {
        line: String,
    },
    /// a notice Hydra raised itself
    Notice(Notice),
}

impl LogSink {
    /// How many lines were dropped.
    ///
    /// **ESTABLISHED**: the same function as [`PlayerLog::dropped`].
    pub fn dropped(&self) -> u64 {
        self.drops.count()
    }

    /// Take one queued line, if there is one.
    ///
    /// **ESTABLISHED** (`0x101a874b8`): a non-blocking receive.
    pub fn try_recv(&self) -> Option<LogLine> {
        self.rx.try_recv().ok()
    }

    /// Wait for the next queued line.
    pub fn recv(&self) -> Option<LogLine> {
        self.rx.recv().ok()
    }

    /// Turn a queued line into the `[time][stream] text` form.
    pub fn render(line: &LogLine, day: &str, time: &str) -> Vec<Line> {
        match line {
            LogLine::Frame { stream, text } => text
                .lines()
                .map(|t| Line {
                    time: time.to_string(),
                    stream: stream.clone(),
                    text: t.to_string(),
                })
                .collect(),
            LogLine::Typed { line } => vec![Line {
                time: time.to_string(),
                stream: "typed".to_string(),
                text: format!("{day}/{line}"),
            }],
            LogLine::Notice(notice) => notice
                .lines
                .iter()
                .map(|t| Line {
                    time: time.to_string(),
                    stream: "hydra".to_string(),
                    text: t.clone(),
                })
                .collect(),
        }
    }
}

/// The writing end of the log channel.
#[derive(Debug, Clone)]
pub struct PlayerLog {
    tx: SyncSender<LogLine>,
    drops: Drops,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl PlayerLog {
    /// A channel with the default capacity.
    pub fn new() -> (PlayerLog, LogSink) {
        PlayerLog::with_capacity(CAPACITY)
    }

    /// A channel with `capacity` slots.
    ///
    /// **ESTABLISHED** (`0x101a87834`): the capacity is a real bound.
    pub fn with_capacity(capacity: usize) -> (PlayerLog, LogSink) {
        let (tx, rx) = sync_channel(capacity.max(1));
        let drops = Drops::default();
        (
            PlayerLog {
                tx,
                drops: drops.clone(),
                closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
            LogSink { rx, drops },
        )
    }

    /// How many lines were dropped.
    pub fn dropped(&self) -> u64 {
        self.drops.count()
    }

    /// Close the channel: later sends are dropped and counted.
    ///
    /// **ESTABLISHED** (`0x101a87a9c`, two instructions): flips a flag.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
    }

    /// Queue one line, counting a drop when the channel is closed.
    pub fn send(&self, line: LogLine) -> bool {
        if self.closed.load(Ordering::Relaxed) {
            self.drops.bump();
            return false;
        }
        match self.tx.try_send(line) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.drops.bump();
                false
            }
        }
    }

    /// Log something the player typed.
    ///
    /// **ESTABLISHED** (`0x101a87aa4`): the typed line is recorded with the
    /// `"{}/{}"` template, so every typed line carries the day it was typed on.
    pub fn typed(&self, line: &str) -> bool {
        self.send(LogLine::Typed {
            line: line.to_string(),
        })
    }

    /// Log a game frame's text under `stream`.
    pub fn frame(&self, stream: &str, text: &str) -> bool {
        self.send(LogLine::Frame {
            stream: stream.to_string(),
            text: text.to_string(),
        })
    }

    /// Log a notice's lines.
    ///
    /// **ESTABLISHED** (`0x101a87d3c`): a quiet notice is not logged.
    pub fn notice(&self, notice: &Notice) -> bool {
        if notice.is_quiet() {
            return false;
        }
        self.send(LogLine::Notice(notice.clone()))
    }

    /// Install the redaction list on the writer.
    ///
    /// **ESTABLISHED** shape (`0x101a87f20`): the redactions are applied by the
    /// writer (`Redactions::apply`), so this is the hand-off.
    pub fn redact(&self, writer: &mut super::writer::PlayerWriter, redactions: &[String]) {
        writer.redactions = redactions.to_vec();
    }
}

impl Default for PlayerLog {
    fn default() -> Self {
        PlayerLog::new().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notice::Notice;

    #[test]
    fn a_queued_line_comes_back_out() {
        let (log, sink) = PlayerLog::new();
        assert_eq!(sink.dropped(), 0);
        assert!(log.typed("go north"));
        assert_eq!(
            sink.try_recv(),
            Some(LogLine::Typed {
                line: "go north".to_string()
            })
        );
        assert_eq!(sink.try_recv(), None);
    }

    #[test]
    fn closing_counts_drops_on_both_ends() {
        let (log, sink) = PlayerLog::new();
        log.close();
        assert!(!log.typed("ignored"));
        assert_eq!(log.dropped(), 1);
        assert_eq!(sink.dropped(), 1, "the counter is shared");
    }

    #[test]
    fn a_full_channel_drops_rather_than_blocking() {
        let (log, sink) = PlayerLog::with_capacity(2);
        assert!(log.frame("speech", "one"));
        assert!(log.frame("speech", "two"));
        assert!(!log.frame("speech", "three"));
        assert_eq!(log.dropped(), 1);
        assert!(sink.try_recv().is_some());
        assert!(sink.try_recv().is_some());
    }

    #[test]
    fn quiet_notices_are_not_logged() {
        let (log, sink) = PlayerLog::new();
        assert!(!log.notice(&Notice::line(crate::notice::LEVEL_QUIET, "hush")));
        assert!(log.notice(&Notice::line(crate::notice::LEVEL_INFO, "hi")));
        assert!(matches!(sink.try_recv(), Some(LogLine::Notice(_))));
    }

    #[test]
    fn rendering_uses_the_recovered_templates() {
        let day = "2024-05-01";
        let time = "10:04";
        let typed = LogLine::Typed {
            line: "go north".to_string(),
        };
        let rendered = LogSink::render(&typed, day, time);
        assert_eq!(rendered[0].to_log(), "[10:04][typed] 2024-05-01/go north");

        let frame = LogLine::Frame {
            stream: "speech".to_string(),
            text: "a\nb".to_string(),
        };
        let rendered = LogSink::render(&frame, day, time);
        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[1].to_log(), "[10:04][speech] b");

        let notice = LogLine::Notice(Notice::line(2, "careful"));
        let rendered = LogSink::render(&notice, day, time);
        assert_eq!(rendered[0].stream, "hydra");
    }
}
