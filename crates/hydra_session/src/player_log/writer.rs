//! Writing the day files.
//!
//! Recovered from `player_log::writer::*`:
//! `0x101a514e8` (`adopt_unscoped`), `0x101a51fe0` (`day_of`),
//! `0x101a52054` (`day_path`), `0x101a51c1c` (`days`), `0x101a519c4` (`dir`),
//! `0x101a52158` (`file_key`), `0x101a52498` (`file_name`),
//! `0x101a50dd4` (`parse_line`), `0x101a51d48` (`piece`),
//! `0x101a51d44` (`root`), `0x101a522e0` (`scrubbed`),
//! `0x101a50f00`/`0x101a50f3c` (`PlayerWriter::{flush, write}`).
//!
//! **ESTABLISHED** formats (length-prefixed format pieces decoded from
//! `__TEXT`):
//!
//! ```text
//! log line      "[{}][{}] {}\n"      time, stream, text      (0x103e7bf3a)
//! file key      "{}_{}"              day, character          (0x103e7bf48)
//! file name     "{}_{}.log"          day, character          (0x103e7bf4d)
//! key fragment  "{}_"                day                     (0x103e7bf57)
//! ```
//!
//! **ESTABLISHED** behaviour:
//! * `root` is `hydra_net::sink::config::log_dir` — the log root comes from
//!   the endpoint configuration.  We take the base directory as an argument
//!   and use `<base>/logs`.
//! * `write` (`0x101a50f3c`) computes today's key with `eastern::now` +
//!   `file_key`, makes sure `dir` exists (`DirBuilder::_create`), opens
//!   `day_path` with `OpenOptions`, applies the configured redactions
//!   (`Redactions::apply`), formats a `[time][stream] text` line and records
//!   it with `links::written`.
//! * `adopt_unscoped` moves day files out of the old shared directory into the
//!   character directory, then removes the old directory.
//! * `piece` parses a day file name back into its parts (`eastern::parse` +
//!   `eastern::stretch_start`), and `day_of` uses `piece` on a file name.
//!
//! **UNKNOWN**: the exact date format of a day key.  `eastern::parse` /
//! `eastern::format` are Hydra's Eastern-time helpers; we use `YYYY-MM-DD`,
//! which sorts correctly as text and is what the archive periods
//! (`week-<date>`) and the retention preview need.

use crate::store;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory under the configured base that holds the logs.
pub const LOGS: &str = "logs";
/// The extension of a day file.
pub const EXT: &str = ".log";
/// How many lines a [`PlayerWriter`] keeps before forcing a flush.
pub const FLUSH_EVERY: usize = 32;

/// The log root for a configured base directory.
///
/// **ESTABLISHED** shape (`0x101a51d44`): the original reads the configured
/// `log_dir`; we derive it from the base the caller passes.
pub fn root(base: &Path) -> PathBuf {
    base.join(LOGS)
}

/// The directory a character's day files live in: `<root>/<safe(character)>`.
///
/// **ESTABLISHED** shape (`0x101a519c4`): one `Path::join` with a
/// `store::safe_component`-style component.
pub fn dir(root: &Path, character: &str) -> PathBuf {
    root.join(store::safe_component(character))
}

/// The key a day is filed under: `"{}_{}"` — day then character.
///
/// **ESTABLISHED** (`0x101a52158`): `eastern::date` + `format`.
pub fn file_key(day: &str, character: &str) -> String {
    format!("{day}_{}", store::safe_component(character))
}

/// A day file's name: `"{}_{}.log"`.
///
/// **ESTABLISHED** (`0x101a52498`).
pub fn file_name(day: &str, character: &str) -> String {
    format!("{}.log", file_key(day, character))
}

/// `<dir>/<day>_<character>.log`.
///
/// **ESTABLISHED** (`0x101a52054`): `dir` then `file_name` then `Path::join`.
pub fn day_path(dir: &Path, day: &str, character: &str) -> PathBuf {
    dir.join(file_name(day, character))
}

/// Split a day file's *file name* back into `(day, character)`.
///
/// **ESTABLISHED** shape (`0x101a51d48`): the file name is taken apart on the
/// last `_` (the recovered code runs `from_utf8` over `Path::file_name` and
/// then `piece`).
pub fn piece(file_name: &str) -> Option<(String, String)> {
    let stem = file_name.strip_suffix(EXT).unwrap_or(file_name);
    let (day, character) = stem.rsplit_once('_')?;
    if day.is_empty() || character.is_empty() {
        return None;
    }
    Some((day.to_string(), character.to_string()))
}

/// The day a *path* belongs to.
///
/// **ESTABLISHED** (`0x101a51fe0`): `Path::file_name` then [`piece`].
pub fn day_of(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    piece(&name).map(|(day, _)| day)
}

/// Every day in a directory, oldest first.
///
/// **ESTABLISHED** shape (`0x101a51c1c`): `read_dir`, keep the `.log` files,
/// collect `(day, path)` and sort.
pub fn days(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some((day, _)) = piece(&entry.file_name().to_string_lossy()) {
            out.push((day, path));
        }
    }
    out.sort();
    out
}

/// One parsed log line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    /// `HH:MM`, the time the line was written
    pub time: String,
    /// the stream class, e.g. `speech`
    pub stream: String,
    /// the rest of the line
    pub text: String,
}

impl Line {
    /// Render the recovered `[{}][{}] {}\n` form (without the newline).
    pub fn to_log(&self) -> String {
        format!("[{}][{}] {}", self.time, self.stream, self.text)
    }
}

/// Parse a `[time][stream] text` line.
///
/// **ESTABLISHED** (`0x101a50dd4`): two bracket groups then the text.
pub fn parse_line(line: &str) -> Option<Line> {
    let rest = line.strip_prefix('[')?;
    let (time, rest) = rest.split_once("][")?;
    let (stream, text) = rest.split_once("] ").or_else(|| rest.split_once(']'))?;
    if time.is_empty() {
        return None;
    }
    Some(Line {
        time: time.to_string(),
        stream: stream.to_string(),
        text: text.to_string(),
    })
}

/// Apply the configured redactions to `text`.
///
/// **ESTABLISHED** shape (`0x101a522e0`): walks the redaction list and
/// replaces each entry; the original calls `Redactions::apply` from
/// `hydra_net`.  Redactions are applied longest-first so a shorter entry cannot
/// eat part of a longer one.
pub fn scrubbed(text: &str, redactions: &[String]) -> String {
    let mut sorted: Vec<&String> = redactions.iter().filter(|r| !r.is_empty()).collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.len()));
    let mut out = text.to_string();
    for secret in sorted {
        if out.contains(secret.as_str()) {
            out = out.replace(secret.as_str(), "***");
        }
    }
    out
}

/// Move day files out of the unscoped directory into the character directory.
///
/// **ESTABLISHED** shape (`0x101a514e8`): create the target, `read_dir` the
/// source, `rename` each file that is not a directory, then `remove_dir` the
/// (now empty) source.
pub fn adopt_unscoped(root: &Path, character: &str) -> std::io::Result<usize> {
    let target = dir(root, character);
    fs::create_dir_all(&target)?;
    let source = root.to_path_buf();
    let mut moved = 0;
    let entries = match fs::read_dir(&source) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if !name.to_string_lossy().ends_with(EXT) {
            continue;
        }
        fs::rename(&path, target.join(&name))?;
        moved += 1;
    }
    let _ = fs::remove_dir(&source);
    Ok(moved)
}

/// Appends lines to the right day file.
///
/// **ESTABLISHED** layout shape: `write` (`0x101a50f3c`) keeps the open file for
/// the current day and reopens it when the day rolls over.
#[derive(Debug)]
pub struct PlayerWriter {
    directory: PathBuf,
    character: String,
    /// the day the open file is for
    day: String,
    /// buffered lines for the open day
    pending: VecDeque<String>,
    /// redactions applied to every line
    pub redactions: Vec<String>,
    /// how many lines have been written
    pub written: u64,
}

impl PlayerWriter {
    /// A writer for `character` under `root`.
    pub fn new(root: &Path, character: &str) -> Self {
        PlayerWriter {
            directory: dir(root, character),
            character: character.to_string(),
            day: String::new(),
            pending: VecDeque::new(),
            redactions: Vec::new(),
            written: 0,
        }
    }

    /// The directory this writer appends to.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The day the writer is currently on.
    pub fn day(&self) -> &str {
        &self.day
    }

    /// Write one line, stamped with `day` and `time`.
    ///
    /// **ESTABLISHED** shape: the day file is opened (creating the directory)
    /// when the day changes; the line is redacted and formatted with the
    /// recovered template, then appended.
    pub fn write_at(&mut self, day: &str, time: &str, stream: &str, text: &str) -> std::io::Result<()> {
        if self.day != day {
            self.flush()?;
            self.day = day.to_string();
        }
        let line = Line {
            time: time.to_string(),
            stream: stream.to_string(),
            text: scrubbed(text, &self.redactions),
        };
        self.pending.push_back(line.to_log());
        self.written += 1;
        if self.pending.len() >= FLUSH_EVERY {
            self.flush()?;
        }
        Ok(())
    }

    /// Write one already formatted line (or a raw line, which is stamped with
    /// the empty time and stream).
    pub fn write(&mut self, line: &str) -> std::io::Result<()> {
        let (day, time, stream, text) = match parse_line(line) {
            Some(parsed) => (
                self.day.clone(),
                parsed.time,
                parsed.stream,
                parsed.text,
            ),
            None => (self.day.clone(), String::new(), String::new(), line.to_string()),
        };
        let day = if day.is_empty() {
            "1970-01-01".to_string()
        } else {
            day
        };
        self.write_at(&day, &time, &stream, &text)
    }

    /// Append everything buffered so far.
    ///
    /// **ESTABLISHED** shape (`0x101a50f00`): `DirBuilder::_create`,
    /// `OpenOptions::_open` (append), `write_all` for each buffered line.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if self.pending.is_empty() || self.day.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.directory)?;
        let path = day_path(&self.directory, &self.day, &self.character);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let mut buf = String::new();
        while let Some(line) = self.pending.pop_front() {
            let _ = writeln!(buf, "{line}");
        }
        file.write_all(buf.as_bytes())?;
        file.sync_data()
    }
}

impl Drop for PlayerWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// `civil` — the day key and clock time for a moment, as the day files use
/// them.
///
/// **ESTABLISHED** role: `write` asks `hydra_net::eastern::now` for the current
/// Eastern date and time and feeds them to `file_key` and the line template.
/// `eastern` is a sibling crate in the original; here the same civil-date
/// arithmetic lives in [`crate::notes::civil_from_days`] so the crate stays
/// std-only.
pub mod civil {
    use crate::notes::civil_from_days;

    /// `(day, time)` for `secs` seconds since the Unix epoch, UTC.
    pub fn of_secs(secs: u64) -> (String, String) {
        let days = (secs / 86_400) as i64;
        let rest = secs % 86_400;
        let (y, m, d) = civil_from_days(days);
        (
            format!("{y:04}-{m:02}-{d:02}"),
            format!("{:02}:{:02}", rest / 3600, (rest % 3600) / 60),
        )
    }

    /// `(day, time)` for now.
    pub fn now() -> (String, String) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        of_secs(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_key_and_name_use_the_recovered_templates() {
        assert_eq!(file_key("2024-05-01", "Aelith"), "2024-05-01_aelith");
        assert_eq!(file_name("2024-05-01", "Aelith"), "2024-05-01_aelith.log");
        assert_eq!(
            day_path(Path::new("/l"), "2024-05-01", "Aelith"),
            PathBuf::from("/l/2024-05-01_aelith.log")
        );
        assert_eq!(root(Path::new("/b")), PathBuf::from("/b/logs"));
        assert_eq!(dir(Path::new("/l"), "Aelith"), PathBuf::from("/l/aelith"));
    }

    #[test]
    fn a_day_file_name_takes_apart_on_the_last_underscore() {
        assert_eq!(
            piece("2024-05-01_aelith.log"),
            Some(("2024-05-01".to_string(), "aelith".to_string()))
        );
        assert_eq!(
            piece("2024-05-01_a_b.log"),
            Some(("2024-05-01_a".to_string(), "b".to_string()))
        );
        assert_eq!(piece("nonsense.log"), None);
        assert_eq!(piece("_x.log"), None);
        assert_eq!(piece("x_.log"), None);
        assert_eq!(
            day_of(Path::new("/l/2024-05-01_aelith.log")),
            Some("2024-05-01".to_string())
        );
    }

    #[test]
    fn parse_line_reads_the_recovered_brackets() {
        assert_eq!(
            parse_line("[10:04][speech] Aelith says, \"hi\""),
            Some(Line {
                time: "10:04".into(),
                stream: "speech".into(),
                text: "Aelith says, \"hi\"".into()
            })
        );
        // a line with no text at all still parses
        assert_eq!(
            parse_line("[23:59][combat] "),
            Some(Line {
                time: "23:59".into(),
                stream: "combat".into(),
                text: "".into()
            })
        );
        assert_eq!(parse_line("plain"), None);
        assert_eq!(parse_line("[] [x] y"), None);
        let line = Line {
            time: "10:04".into(),
            stream: "speech".into(),
            text: "hi".into(),
        };
        assert_eq!(line.to_log(), "[10:04][speech] hi");
    }

    #[test]
    fn redactions_run_longest_first() {
        let redactions = vec!["hunter2".to_string(), "hunter2extra".to_string()];
        assert_eq!(
            scrubbed("pw=hunter2extra and hunter2", &redactions),
            "pw=*** and ***"
        );
        assert_eq!(scrubbed("nothing", &[]), "nothing");
        assert_eq!(scrubbed("x", &["".to_string()]), "x");
    }

    #[test]
    fn the_writer_appends_to_the_right_day_file() {
        let root = std::env::temp_dir().join(format!("hydra-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut w = PlayerWriter::new(&root, "Aelith");
        w.write_at("2024-05-01", "10:04", "speech", "one").unwrap();
        w.write_at("2024-05-01", "10:05", "combat", "two").unwrap();
        w.write_at("2024-05-02", "08:00", "speech", "three").unwrap();
        w.flush().unwrap();
        let first = fs::read_to_string(day_path(&dir(&root, "Aelith"), "2024-05-01", "Aelith"))
            .unwrap();
        assert_eq!(first, "[10:04][speech] one\n[10:05][combat] two\n");
        let second = fs::read_to_string(day_path(&dir(&root, "Aelith"), "2024-05-02", "Aelith"))
            .unwrap();
        assert_eq!(second, "[08:00][speech] three\n");
        let days = days(&dir(&root, "Aelith"));
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].0, "2024-05-01");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn civil_dates() {
        assert_eq!(
            civil::of_secs(0),
            ("1970-01-01".to_string(), "00:00".to_string())
        );
        assert_eq!(
            civil::of_secs(1_714_557_600),
            ("2024-05-01".to_string(), "10:00".to_string())
        );
    }
}
