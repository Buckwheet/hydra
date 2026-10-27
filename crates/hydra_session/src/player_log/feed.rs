//! The log feed: the object that decides what reaches the day file.
//!
//! Recovered from `player_log::feed::{Feed::*, Capture}` at `0x101a84dec`
//! (`close`), `0x101a870e0` (`command`), `0x101a86790` (`frame`),
//! `0x101a8737c` (`set_rules`), `0x101a84b14` (`take_noticed`),
//! `0x101a86d98` (`watch`), `0x101a87250` (`write_to`).
//!
//! **ESTABLISHED**:
//! * `Feed::frame` (`0x101a86790`) is the hot path: it takes one frame's text,
//!   runs the stream rules over it, and either logs it, redirects it to another
//!   stream, or drops it.
//! * `Feed::command` (`0x101a870e0`) records a typed command with the same
//!   `"{}/{}"` template the channel uses (`0x103e7c12e`), so a command that was
//!   typed is followed by its output.
//! * `Feed::set_rules` (`0x101a8737c`) is eight bytes of move: it swaps the
//!   compiled rules in.
//! * `Feed::take_noticed` (`0x101a84b14`) drains what the feed has already
//!   decided to show, so a caller can fold it into the session's state.
//! * `Feed::watch` (`0x101a86d98`) follows the current day file, handing new
//!   lines to a watcher.
//! * `Feed::write_to` (`0x101a87250`) points the feed at a different directory.
//! * `LogSettings` (a `Deserialize` impl of its own,
//!   `0x101a89050`) carries the feed's settings, and `Capture` (its `Default`
//!   appears at `0x101a0cff8`/`0x101a11528`) is what one frame captured.
//!
//! **UNKNOWN**: the `LogSettings` fields and the `Capture` layout.  We keep the
//! settings the rules need (`Archive`, the stream list, the redactions) and a
//! capture of `(stream, text)` pairs.

use super::archive::Archive;
use super::channel::PlayerLog;
use super::writer::{self, PlayerWriter};
use crate::stream_rules::{Compiled, StreamRules};
use std::path::{Path, PathBuf};

/// The feed's settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogSettings {
    /// which archive period the sweep folds into
    pub archive: Archive,
    /// the streams to keep; empty keeps everything
    pub streams: Vec<String>,
    /// the redactions applied to every line
    pub redactions: Vec<String>,
    /// whether the log is kept at all
    pub keep: bool,
}

impl LogSettings {
    /// Settings that keep everything.
    pub fn keeping() -> Self {
        LogSettings {
            keep: true,
            ..LogSettings::default()
        }
    }
}

/// What one frame contributed to the log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capture {
    /// the stream the line landed in
    pub stream: String,
    /// the lines
    pub lines: Vec<String>,
}

impl Capture {
    /// Is there anything to log?
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// The object that decides what reaches the day file.
#[derive(Debug)]
pub struct Feed {
    writer: PlayerWriter,
    rules: StreamRules,
    settings: LogSettings,
    log: Option<PlayerLog>,
    noticed: Vec<String>,
}

impl Feed {
    /// A feed for `character` under `root`.
    pub fn new(root: &Path, character: &str) -> Self {
        Feed {
            writer: PlayerWriter::new(root, character),
            rules: StreamRules::empty(),
            settings: LogSettings::keeping(),
            log: None,
            noticed: Vec::new(),
        }
    }

    /// Point the feed's writer at another directory.
    ///
    /// **ESTABLISHED** shape (`0x101a87250`): replaces the writer.
    pub fn write_to(&mut self, root: &Path, character: &str) {
        let _ = self.writer.flush();
        self.writer = PlayerWriter::new(root, character);
        self.writer.redactions = self.settings.redactions.clone();
    }

    /// The directory being written to.
    pub fn directory(&self) -> PathBuf {
        self.writer.directory().to_path_buf()
    }

    /// Install the compiled stream rules.
    ///
    /// **ESTABLISHED** (`0x101a8737c`, eight bytes): a plain move.
    pub fn set_rules(&mut self, rules: StreamRules) {
        self.rules = rules;
    }

    /// The rules in force.
    pub fn rules(&self) -> &StreamRules {
        &self.rules
    }

    /// The feed's settings.
    pub fn settings(&self) -> &LogSettings {
        &self.settings
    }

    /// Apply the settings (rules, streams, redactions, archive period).
    pub fn configure(&mut self, settings: LogSettings) {
        self.writer.redactions = settings.redactions.clone();
        self.settings = settings;
    }

    /// Attach the log channel so the feed also reports what it saw.
    pub fn attach(&mut self, log: PlayerLog) {
        self.log = Some(log);
    }

    /// Take one game frame's text.
    ///
    /// **ESTABLISHED** shape (`0x101a86790`): look the line up in the rules
    /// first; a `gag`/`hide` match keeps it out of the log, a `redirect` match
    /// sends it to the named stream, anything else goes to its own stream.
    /// Returns what was captured.
    pub fn frame(&mut self, stream: &str, text: &str) -> Capture {
        let compiled: Compiled = self.rules.current();
        let (stream, text) = match compiled.find(text) {
            Some(rule) => match rule.class {
                crate::stream_rules::Class::Gag | crate::stream_rules::Class::Hide => {
                    return Capture::default();
                }
                crate::stream_rules::Class::Redirect => (
                    if rule.stream.is_empty() { stream } else { &rule.stream },
                    text,
                ),
                _ => (stream, text),
            },
            None => (stream, text),
        };
        if !self.settings.keep {
            return Capture::default();
        }
        if !self.settings.streams.is_empty()
            && !self
                .settings
                .streams
                .iter()
                .any(|s| s.eq_ignore_ascii_case(stream))
        {
            return Capture::default();
        }
        let (day, time) = writer::civil::now();
        let clean = writer::scrubbed(text, &self.settings.redactions);
        let _ = self.writer.write_at(&day, &time, stream, &clean);
        if let Some(log) = &self.log {
            log.frame(stream, &clean);
        }
        self.noticed
            .push(format!("{} [{}] {}", time, stream, clean));
        Capture {
            stream: stream.to_string(),
            lines: clean.lines().map(str::to_string).collect(),
        }
    }

    /// Record a typed command.
    ///
    /// **ESTABLISHED** shape (`0x101a870e0`): the typed line is logged as
    /// `<day>/<line>` in the `typed` stream.
    pub fn command(&mut self, line: &str) {
        if !self.settings.keep {
            return;
        }
        let (day, time) = writer::civil::now();
        let _ = self.writer.write_at(&day, &time, "typed", &format!("{day}/{line}"));
        if let Some(log) = &self.log {
            log.typed(line);
        }
    }

    /// Take everything the feed noticed since the last call.
    ///
    /// **ESTABLISHED** shape (`0x101a84b14`): drains and returns the queue.
    pub fn take_noticed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.noticed)
    }

    /// Watch the current day file: hand every complete line to `sink`.
    ///
    /// **ESTABLISHED** shape (`0x101a86d98`): follows the open day file and
    /// emits its lines (the original runs this as a task).
    pub fn watch(&mut self, sink: &mut dyn FnMut(&str)) -> std::io::Result<usize> {
        self.writer.flush()?;
        let day = self.writer.day().to_string();
        if day.is_empty() {
            return Ok(0);
        }
        let Some((_day, path)) = writer::days(self.writer.directory())
            .into_iter()
            .find(|(d, _)| *d == day)
        else {
            return Ok(0);
        };
        let text = std::fs::read_to_string(&path)?;
        let mut count = 0;
        for line in text.lines() {
            sink(line);
            count += 1;
        }
        Ok(count)
    }

    /// Flush what has been written.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }

    /// Close the feed: flush, then stop accepting frames.
    ///
    /// **ESTABLISHED** shape (`0x101a84dec`): the recovered body formats
    /// `"{}/{}"` (the day path) and finishes.
    pub fn close(&mut self) -> std::io::Result<PathBuf> {
        self.writer.flush()?;
        self.settings.keep = false;
        Ok(self.writer.directory().to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_rules::{Class, Rule, StreamRules};

    fn dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("hydra-feed-{}-{tag}", std::process::id()))
    }

    #[test]
    fn frames_land_in_their_stream_and_gagged_frames_do_not() {
        let root = dir("a");
        let _ = std::fs::remove_dir_all(&root);
        let mut feed = Feed::new(&root, "Aelith");
        assert_eq!(
            feed.frame("speech", "Aelith says hi").stream,
            "speech"
        );
        feed.set_rules(StreamRules::new(&[Rule::new("gag", "junk", "^spam", Class::Gag)]).unwrap());
        assert!(feed.frame("junk", "spam spam").is_empty());
        feed.close().unwrap();
        assert_eq!(feed.take_noticed().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn redirect_rules_move_a_line_to_another_stream() {
        let root = dir("b");
        let _ = std::fs::remove_dir_all(&root);
        let mut feed = Feed::new(&root, "Aelith");
        feed.set_rules(
            StreamRules::new(&[Rule::new("gossip", "talk", "^Gossip:", Class::Redirect)]).unwrap(),
        );
        assert_eq!(feed.frame("main", "Gossip: hi").stream, "talk");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn settings_filter_streams_and_redact() {
        let root = dir("c");
        let _ = std::fs::remove_dir_all(&root);
        let mut feed = Feed::new(&root, "Aelith");
        feed.configure(LogSettings {
            keep: true,
            streams: vec!["speech".to_string()],
            redactions: vec!["hunter2".to_string()],
            ..LogSettings::default()
        });
        assert_eq!(feed.frame("speech", "pw hunter2").lines, ["pw ***"]);
        assert!(feed.frame("combat", "a kobold attacks").is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn watch_hands_over_the_current_day() {
        let root = dir("e");
        let _ = std::fs::remove_dir_all(&root);
        let mut feed = Feed::new(&root, "Aelith");
        feed.frame("speech", "one");
        feed.frame("speech", "two");
        let mut seen = Vec::new();
        let count = feed.watch(&mut |line| seen.push(line.to_string())).unwrap();
        assert_eq!(count, 2);
        assert_eq!(seen.len(), 2);
        assert!(seen[1].contains("two"));
        // a fresh feed with an empty day watches nothing
        let mut empty = Feed::new(&dir("f"), "Aelith");
        assert_eq!(empty.watch(&mut |_| {}).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_closed_feed_stops_logging() {
        let root = dir("d");
        let _ = std::fs::remove_dir_all(&root);
        let mut feed = Feed::new(&root, "Aelith");
        feed.command("go north");
        feed.close().unwrap();
        assert!(feed.frame("speech", "after the close").is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
