//! Retention: how much log to keep.
//!
//! Recovered from `player_log::retention::{doomed, preview, prune}` at
//! `0x101aa09f4`, `0x101aa1558`, `0x101aa06b8`.
//!
//! **ESTABLISHED**:
//! * `preview` formats exactly one sentence (literal at `0x103e7c960`,
//!   length-prefixed pieces decoded):
//!
//!   ```text
//!   The next login removes {} file{} ({} KB), the days up to {} (the oldest {}).
//!   ```
//!
//!   The four arguments are a count, a plural suffix, a size in kilobytes and
//!   two day keys — so retention works in whole days and reports the oldest
//!   day it will remove.
//! * `doomed` (`0x101aa09f4`) walks `writer::days`, parses each day key with
//!   `eastern::parse`/`days_from_civil`/`civil_from_days`/`eastern::format`,
//!   and collects the pairs that fall before the cut-off; it also looks at
//!   `archive::archives` and `archive::names`, so archived periods are counted
//!   too.
//! * `prune` (`0x101aa06b8`) calls `doomed` and `remove_file`s each entry,
//!   reporting failures with the `"{}: {}"` shape.
//!
//! **UNKNOWN**: how the number of days to keep is configured (it comes from
//! the endpoint settings, not from a literal here).  Every function takes it as
//! an argument.

use super::archive;
use super::writer;
use std::path::{Path, PathBuf};

/// One file that retention would remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doomed {
    /// the day (or archive period) the file belongs to
    pub day: String,
    /// the file
    pub path: PathBuf,
    /// its size in bytes
    pub bytes: u64,
}

impl Doomed {
    /// The size in whole kilobytes, rounded up, as the preview shows it.
    pub fn kb(&self) -> u64 {
        self.bytes.div_ceil(1024)
    }
}

/// The files `keep` days of retention would remove, oldest first.
///
/// **ESTABLISHED** shape: day files older than the cut-off plus the archives
/// whose period is older than it.
pub fn doomed(dir: &Path, keep: u64) -> Vec<Doomed> {
    let days = writer::days(dir);
    if days.is_empty() {
        return Vec::new();
    }
    let Some(latest) = days.iter().map(|(d, _)| d.as_str()).max() else {
        return Vec::new();
    };
    let Some(cutoff) = cutoff_before(latest, keep) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (day, path) in &days {
        if day.as_str() < cutoff.as_str() {
            out.push(Doomed {
                day: day.clone(),
                bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
                path: path.clone(),
            });
        }
    }
    for (period, path) in archive::archives(dir) {
        // Archive periods are named `week-<day>` or `month-<day>`.
        let day = period.rsplit('-').take(3).collect::<Vec<_>>();
        let day = day.into_iter().rev().collect::<Vec<_>>().join("-");
        if day.as_str() < cutoff.as_str() {
            out.push(Doomed {
                day: period.clone(),
                bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                path,
            });
        }
    }
    out.sort_by(|a, b| a.day.cmp(&b.day));
    out
}

/// The day `keep` days before `latest`.
fn cutoff_before(latest: &str, keep: u64) -> Option<String> {
    let (y, m, d) = archive::parse_day(latest)?;
    let days = crate::notes::days_from_civil(y, m, d) - keep as i64;
    let (y, m, d) = crate::notes::civil_from_days(days);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// The sentence shown before a prune.
///
/// **ESTABLISHED** text: `The next login removes {} file{} ({} KB), the days up
/// to {} (the oldest {}).`
pub fn preview(files: &[Doomed]) -> String {
    if files.is_empty() {
        return "The next login removes no log files.".to_string();
    }
    let kb: u64 = files.iter().map(Doomed::kb).sum();
    let newest = files.iter().map(|f| f.day.as_str()).max().unwrap_or("");
    let oldest = files.iter().map(|f| f.day.as_str()).min().unwrap_or("");
    let plural = if files.len() == 1 { "" } else { "s" };
    format!(
        "The next login removes {} file{} ({} KB), the days up to {} (the oldest {}).",
        files.len(),
        plural,
        kb,
        newest,
        oldest
    )
}

/// Remove everything [`doomed`] names.  Returns the removed files and any
/// failures as `"{}: {}"`.
///
/// **ESTABLISHED** shape (`0x101aa06b8`): `doomed`, then `remove_file` for
/// each, collecting failures.
pub fn prune(dir: &Path, keep: u64) -> (Vec<PathBuf>, Vec<String>) {
    let mut removed = Vec::new();
    let mut problems = Vec::new();
    for entry in doomed(dir, keep) {
        match std::fs::remove_file(&entry.path) {
            Ok(()) => removed.push(entry.path),
            Err(e) => problems.push(format!("{}: {e}", entry.path.display())),
        }
    }
    (removed, problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_log::writer::PlayerWriter;

    fn fixture(tag: &str, days: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("hydra-ret-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut w = PlayerWriter::new(&root, "Aelith");
        for day in days {
            w.write_at(day, "10:00", "speech", "hi").unwrap();
        }
        w.flush().unwrap();
        w.directory().to_path_buf()
    }

    #[test]
    fn the_cutoff_is_keep_days_before_the_latest() {
        assert_eq!(
            cutoff_before("2024-05-10", 3).unwrap(),
            "2024-05-07"
        );
        assert_eq!(cutoff_before("not a day", 3), None);
    }

    #[test]
    fn doomed_lists_the_old_days_only() {
        let dir = fixture("a", &["2024-05-01", "2024-05-02", "2024-05-10"]);
        let entries = doomed(&dir, 3);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].day, "2024-05-01");
        assert_eq!(entries[1].day, "2024-05-02");
        assert!(entries[0].kb() >= 1);
        assert!(entries[0].bytes > 0);
        assert!(doomed(&dir, 100).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_preview_is_the_recovered_sentence() {
        let files = vec![
            Doomed {
                day: "2024-05-01".to_string(),
                path: PathBuf::from("/a"),
                bytes: 2048,
            },
            Doomed {
                day: "2024-05-02".to_string(),
                path: PathBuf::from("/b"),
                bytes: 1024,
            },
        ];
        assert_eq!(
            preview(&files),
            "The next login removes 2 files (3 KB), the days up to 2024-05-02 (the oldest \
             2024-05-01)."
        );
        let one = vec![Doomed {
            day: "2024-05-01".to_string(),
            path: PathBuf::from("/a"),
            bytes: 1024,
        }];
        assert_eq!(
            preview(&one),
            "The next login removes 1 file (1 KB), the days up to 2024-05-01 (the oldest \
             2024-05-01)."
        );
        assert_eq!(
            preview(&[]),
            "The next login removes no log files."
        );
    }

    #[test]
    fn prune_removes_what_it_promised() {
        let dir = fixture("b", &["2024-05-01", "2024-05-02", "2024-05-10"]);
        let (removed, problems) = prune(&dir, 3);
        assert_eq!(removed.len(), 2);
        assert!(problems.is_empty());
        assert_eq!(writer::days(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
