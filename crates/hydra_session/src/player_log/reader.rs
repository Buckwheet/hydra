//! Reading the day files: days, tails, windows, searches and exports.
//!
//! Recovered from `player_log::reader::*`:
//! `0x101a5b728` (`around`), `0x101a5ac18` (`days`), `0x101a5bb94` (`export`),
//! `0x101a593b0` (`export_path`), `0x101a5e150` (`read_day`),
//! `0x101a59e2c` (`read_day_from`), `0x101a5c66c` (`search`),
//! `0x101a595a0` (`search_days`), `0x101a58ffc` (`sort_again`),
//! `0x101a5cd80` (`tagged`), `0x101a5aff0` (`tail`),
//! `0x101a5cf8c` (`window`), `0x101a5d9d0`/`0x101a5d9e4`/`0x101a5da58`
//! (`Pattern::{regex, literal, matches}`), `0x101a5dc84` (`Streams::admits`).
//!
//! **ESTABLISHED** formats:
//!
//! ```text
//! "{}/{}"          a day path / the reader's display   0x103e7c12e
//! "{}_to_{}"       an export name (from, to)           0x103e7c133
//! "{}.{}.partial"  the export's partial file           0x103e7c13b
//! "{}\n"           one exported line                   0x103e21acf
//! ```
//!
//! **ESTABLISHED** behaviour:
//! * `export` writes to `"{}.{}.partial"` and renames when done, so a failed
//!   export never leaves a half file behind.
//! * `export_path` builds `"{}_to_{}"` from the first and last day.
//! * `Pattern::literal` and `Pattern::regex` are the two ways a search term is
//!   read; `Pattern::matches` is the test.  The original compiled with the
//!   `regex` crate.
//! * `Streams::admits` filters by stream class.
//! * `tail`, `around` and `window` are all "a slice of one day's lines", the
//!   last one taking a range of line numbers.
//!
//! **UNKNOWN**: the exact argument shapes of `around`/`window` (the original
//! passes context counts and anchors that are inlined).  We take the obvious
//! ones: a day, a target line and a context radius.

use super::writer::{self, Line};
use crate::pattern::Pattern as Compiled;
use std::fs;
use std::path::{Path, PathBuf};

/// How many lines [`tail`] returns by default.
pub const TAIL: usize = 200;
/// How much context [`around`] shows by default.
pub const CONTEXT: usize = 5;

/// The day files under `dir`, oldest first.
///
/// **ESTABLISHED** shape (`0x101a5ac18`): collects the days and sorts them.
pub fn days(dir: &Path) -> Vec<(String, PathBuf)> {
    writer::days(dir)
}

/// Re-sort `(day, path)` pairs by day, then by path.
///
/// **ESTABLISHED** shape (`0x101a58ffc`): the recovered body sorts a vector of
/// pairs and joins `"{}/{}"` for display.
pub fn sort_again(pairs: &mut Vec<(String, PathBuf)>) {
    pairs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
}

/// Every line of one day file, already parsed (unparsable lines are kept as
/// text with an empty time and stream).
///
/// **ESTABLISHED** shape (`0x101a5e150`): read the file and split on `\n`.
pub fn read_day(path: &Path) -> std::io::Result<Vec<Line>> {
    let text = fs::read_to_string(path)?;
    Ok(text.lines().map(line_of).collect())
}

/// Read one day's lines starting at 1-based line `from`.
///
/// **ESTABLISHED** shape (`0x101a59e2c`): the streaming form of
/// [`read_day`], used by the paging UI.
pub fn read_day_from(path: &Path, from: u64) -> std::io::Result<Vec<Line>> {
    let lines = read_day(path)?;
    let skip = from.saturating_sub(1) as usize;
    Ok(lines.into_iter().skip(skip).collect())
}

fn line_of(raw: &str) -> Line {
    writer::parse_line(raw).unwrap_or_else(|| Line {
        time: String::new(),
        stream: String::new(),
        text: raw.to_string(),
    })
}

/// The last `count` lines of a day file.
///
/// **ESTABLISHED** shape (`0x101a5aff0`): a bounded ring over the file.
pub fn tail(path: &Path, count: usize) -> std::io::Result<Vec<Line>> {
    let lines = read_day(path)?;
    let start = lines.len().saturating_sub(count);
    Ok(lines[start..].to_vec())
}

/// The lines around 1-based line `target`, `context` either side.
///
/// **ESTABLISHED** shape (`0x101a5b728`): one slice of the day.
pub fn around(path: &Path, target: u64, context: usize) -> std::io::Result<Vec<Line>> {
    let lines = read_day(path)?;
    if lines.is_empty() {
        return Ok(Vec::new());
    }
    let target = target.clamp(1, lines.len() as u64) as usize;
    let lo = target.saturating_sub(1 + context);
    let hi = (target + context).min(lines.len());
    Ok(lines[lo..hi].to_vec())
}

/// An inclusive range of 1-based line numbers.
///
/// **ESTABLISHED** shape (`0x101a5cf8c`): the recovered code slices the line
/// vector and clamps the bounds.
pub fn window(path: &Path, first: u64, last: u64) -> std::io::Result<Vec<(u64, Line)>> {
    let lines = read_day(path)?;
    if lines.is_empty() || first > last {
        return Ok(Vec::new());
    }
    let lo = first.clamp(1, lines.len() as u64) as usize;
    let hi = last.clamp(lo as u64, lines.len() as u64) as usize;
    Ok((lo..=hi).map(|n| (n as u64, lines[n - 1].clone())).collect())
}

/// The lines of `path` whose text matches `pattern`.
///
/// **ESTABLISHED** shape (`0x101a5c66c`): keep the line number with the match.
pub fn search(path: &Path, pattern: &Pattern) -> std::io::Result<Vec<(u64, Line)>> {
    let lines = read_day(path)?;
    Ok(lines
        .into_iter()
        .enumerate()
        .filter(|(_, line)| {
            pattern.matches(&line.text) || pattern.matches(&line.to_log())
        })
        .map(|(i, line)| (i as u64 + 1, line))
        .collect())
}

/// Search every day in `dir`, oldest first.
///
/// **ESTABLISHED** shape (`0x101a595a0`): per-day search, then a merge.
pub fn search_days(dir: &Path, pattern: &Pattern) -> std::io::Result<Vec<(String, u64, Line)>> {
    let mut out = Vec::new();
    for (day, path) in days(dir) {
        for (n, line) in search(&path, pattern)? {
            out.push((day.clone(), n, line));
        }
    }
    Ok(out)
}

/// The lines of a day whose stream class is admitted by `streams`.
///
/// **ESTABLISHED** shape (`0x101a5cd80`): filter by class, keeping the line
/// numbers.
pub fn tagged(path: &Path, streams: &Streams) -> std::io::Result<Vec<(u64, Line)>> {
    let lines = read_day(path)?;
    Ok(lines
        .into_iter()
        .enumerate()
        .filter(|(_, line)| streams.admits(&line.stream))
        .map(|(i, line)| (i as u64 + 1, line))
        .collect())
}

/// The name an export of `first`..`last` is written under.
///
/// **ESTABLISHED** (`0x101a593b0`): `"{}_to_{}"`.
pub fn export_path(dir: &Path, first: &str, last: &str) -> PathBuf {
    dir.join(format!("{first}_to_{last}"))
}

/// Export the lines of `first`..`last` into a single file.
///
/// **ESTABLISHED** (`0x101a5bb94`): writes `"{}.{}.partial"`, then renames it
/// over `"{}.{}"` (the extension is the export's own tag, `txt` here); one
/// `"{}\n"` per line.
pub fn export(dir: &Path, first: &str, last: &str, ext: &str) -> std::io::Result<PathBuf> {
    let base = export_path(dir, first, last);
    let final_path = base.with_extension(ext);
    let partial = PathBuf::from(format!("{}.{}.partial", base.display(), ext));
    let mut out = String::new();
    let mut started = false;
    for (day, path) in days(dir) {
        if day.as_str() < first {
            continue;
        }
        if day.as_str() > last {
            break;
        }
        started = true;
        for line in read_day(&path)? {
            out.push_str(&line.to_log());
            out.push('\n');
        }
    }
    if !started {
        return Ok(final_path);
    }
    fs::create_dir_all(dir)?;
    fs::write(&partial, out)?;
    fs::rename(&partial, &final_path)?;
    Ok(final_path)
}

/// A search term: either a literal or a regular expression.
///
/// **ESTABLISHED**: `Pattern::literal` (`0x101a5d9e4`) and `Pattern::regex`
/// (`0x101a5d9d0`) are the two constructors, `matches` (`0x101a5da58`) the
/// test.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    compiled: Compiled,
}

impl Pattern {
    /// A pattern that matches `text` as a plain substring.
    pub fn literal(text: &str) -> Self {
        Pattern {
            compiled: Compiled::literal(&text.to_lowercase()),
        }
    }

    /// A pattern compiled from a regular expression.  A broken expression is
    /// kept as a literal, so a search never fails outright.
    pub fn regex(text: &str) -> Self {
        match Compiled::regex(text) {
            Ok(compiled) => Pattern { compiled },
            Err(_) => Pattern::literal(text),
        }
    }

    /// Does `text` match?  Matching is case-insensitive, which is what the
    /// log's search has always been.
    pub fn matches(&self, text: &str) -> bool {
        self.compiled.is_match(text) || self.compiled.is_match(&text.to_lowercase())
    }
}

/// A set of stream classes to admit.
///
/// **ESTABLISHED** shape (`0x101a5dc84`): the empty set admits everything, a
/// non-empty set admits only its members.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Streams {
    /// the admitted classes; empty means "all"
    pub classes: Vec<String>,
}

impl Streams {
    /// All streams.
    pub fn all() -> Self {
        Streams::default()
    }

    /// Only these streams.
    pub fn only(classes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Streams {
            classes: classes.into_iter().map(Into::into).collect(),
        }
    }

    /// Is `class` admitted?
    pub fn admits(&self, class: &str) -> bool {
        self.classes.is_empty()
            || self
                .classes
                .iter()
                .any(|c| c.eq_ignore_ascii_case(class))
    }
}

#[cfg(test)]
mod tests {
    use super::super::writer::{day_path, dir as char_dir, PlayerWriter};
    use super::*;

    fn fixture() -> (PathBuf, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "hydra-reader-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let mut w = PlayerWriter::new(&root, "Aelith");
        w.write_at("2024-05-01", "10:00", "speech", "Aelith says, \"one\"")
            .unwrap();
        w.write_at("2024-05-01", "10:01", "combat", "a kobold attacks")
            .unwrap();
        w.write_at("2024-05-01", "10:02", "speech", "Aelith says, \"two\"")
            .unwrap();
        w.write_at("2024-05-02", "11:00", "combat", "a kobold dies")
            .unwrap();
        w.flush().unwrap();
        let d = char_dir(&root, "Aelith");
        (root, d)
    }

    #[test]
    fn days_and_reading_one_day() {
        let (root, d) = fixture();
        let all = days(&d);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, "2024-05-01");
        let lines = read_day(&all[0].1).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].stream, "combat");
        assert_eq!(lines[1].text, "a kobold attacks");
        assert_eq!(
            read_day_from(&all[0].1, 3).unwrap().len(),
            1,
            "read_day_from skips the first two"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tail_around_and_window() {
        let (root, d) = fixture();
        let day = day_path(&d, "2024-05-01", "Aelith");
        assert_eq!(tail(&day, 2).unwrap().len(), 2);
        assert_eq!(tail(&day, 0).unwrap().len(), 0);
        assert_eq!(around(&day, 2, 1).unwrap().len(), 3);
        assert_eq!(around(&day, 1, 1).unwrap().len(), 2);
        assert_eq!(around(&day, 3, 5).unwrap().len(), 3);
        let w = window(&day, 2, 3).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].0, 2);
        assert_eq!(w[0].1.stream, "combat");
        assert_eq!(window(&day, 3, 2).unwrap().len(), 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn literal_and_regex_search() {
        let (root, d) = fixture();
        let day = day_path(&d, "2024-05-01", "Aelith");
        let hits = search(&day, &Pattern::literal("kobold")).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 2);
        // case-insensitive
        assert_eq!(search(&day, &Pattern::literal("AELITH")).unwrap().len(), 2);
        let hits = search(&day, &Pattern::regex("^a .* attacks")).unwrap();
        assert_eq!(hits.len(), 1);
        let many = search_days(&d, &Pattern::literal("kobold")).unwrap();
        assert_eq!(many.len(), 2);
        assert_eq!(many[0].0, "2024-05-01");
        assert_eq!(many[1].0, "2024-05-02");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn streams_filter() {
        let (root, d) = fixture();
        let day = day_path(&d, "2024-05-01", "Aelith");
        assert_eq!(tagged(&day, &Streams::all()).unwrap().len(), 3);
        let only = Streams::only(["combat"]);
        let hits = tagged(&day, &only).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 2);
        assert!(only.admits("COMBAT"));
        assert!(!only.admits("speech"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn export_writes_one_file_and_leaves_no_partial() {
        let (root, d) = fixture();
        assert_eq!(
            export_path(&d, "2024-05-01", "2024-05-02"),
            d.join("2024-05-01_to_2024-05-02")
        );
        let path = export(&d, "2024-05-01", "2024-05-02", "txt").unwrap();
        assert_eq!(path, d.join("2024-05-01_to_2024-05-02.txt"));
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 4);
        assert!(text.starts_with("[10:00][speech] Aelith says, \"one\"\n"));
        assert!(!d.join("2024-05-01_to_2024-05-02.txt.partial").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sort_again_is_stable_on_day_then_path() {
        let mut pairs = vec![
            ("b".to_string(), PathBuf::from("/b")),
            ("a".to_string(), PathBuf::from("/z")),
            ("a".to_string(), PathBuf::from("/a")),
        ];
        sort_again(&mut pairs);
        assert_eq!(pairs[0], ("a".to_string(), PathBuf::from("/a")));
        assert_eq!(pairs[1], ("a".to_string(), PathBuf::from("/z")));
        assert_eq!(pairs[2].0, "b");
    }
}
