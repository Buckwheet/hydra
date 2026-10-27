//! The "lookout seed": the first lines a fresh session is fed from the log.
//!
//! Recovered from `lookout_seed::seed` at `0x101aaa614` (8 KB, mostly inlined
//! `serde` from the archive reader).  The literals it formats were decoded from
//! the length-prefixed pieces in `__TEXT`:
//!
//! ```text
//! {} {}: {} lines        0x103e7cb6d    a stream and a day with a line count
//! {} on {}: {}           0x103e7cb8a    a line count on a day
//! {}'s days: {}          0x103e7cb7d    a character and the days it has
//! {}: {}                 0x103e21615    the shared error shape
//! "{} {} on {}: {}"      the same pieces inlined again
//! ```
//!
//! **ESTABLISHED** shape: `seed` walks a character's day files (and their
//! archives) and returns the most recent lines, summarising what it found with
//! those templates.  It calls `writer`/`reader`/`archive` helpers only, so the
//! summary is a pure function of the log directory.
//!
//! **UNKNOWN**: how many lines the seed takes.  We seed the last
//! [`SEED_LINES`] lines of the newest day, which is what a lookout needs to know
//! where the character was.

use crate::player_log::{archive, reader, writer};

/// How many recent lines [`seed`] returns.
pub const SEED_LINES: usize = 200;

/// The lines a fresh session is fed, newest last.
///
/// Returns the lines and a one-line summary in the recovered shapes, e.g.
/// `"Aelith's days: 3"`.
pub fn seed(dir: &std::path::Path, character: &str) -> (Vec<String>, String) {
    let days = writer::days(dir);
    if days.is_empty() {
        return (Vec::new(), format!("{}'s days: 0", character));
    }
    let (day, path) = match days.last() {
        Some(last) => last.clone(),
        None => return (Vec::new(), format!("{}'s days: 0", character)),
    };
    let lines = reader::tail(&path, SEED_LINES).unwrap_or_default();
    let text: Vec<String> = lines.iter().map(|l| l.to_log()).collect();
    let summary = format!(
        "{} {}: {} lines",
        character,
        day,
        text.len()
    );
    (text, summary)
}

/// The summary for a whole log directory.
///
/// **ESTABLISHED** templates: `{}'s days: {}` and `{} on {}: {}`.
pub fn summary(dir: &std::path::Path, character: &str) -> Vec<String> {
    let days = writer::days(dir);
    let mut out = vec![format!("{}'s days: {}", character, days.len())];
    for (day, path) in &days {
        let count = std::fs::read_to_string(path)
            .map(|t| t.lines().count())
            .unwrap_or(0);
        out.push(format!("{} on {}: {}", character, day, count));
    }
    for (period, path) in archive::archives(dir) {
        let count = archive::names(&path).map(|n| n.len()).unwrap_or(0);
        out.push(format!("{} {}: {} lines", period, character, count));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_log::writer::{day_path, dir as char_dir, PlayerWriter};

    fn fixture(tag: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("hydra-seed-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut w = PlayerWriter::new(&root, "Aelith");
        w.write_at("2024-05-01", "10:00", "speech", "one").unwrap();
        w.write_at("2024-05-02", "10:00", "speech", "two").unwrap();
        w.flush().unwrap();
        char_dir(&root, "Aelith")
    }

    #[test]
    fn an_empty_directory_seeds_nothing() {
        let dir = std::env::temp_dir().join(format!("hydra-seed-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (lines, summary) = seed(&dir, "Aelith");
        assert!(lines.is_empty());
        assert_eq!(summary, "Aelith's days: 0");
    }

    #[test]
    fn the_seed_is_the_newest_day() {
        let dir = fixture("a");
        let (lines, summary) = seed(&dir, "Aelith");
        assert_eq!(lines, vec!["[10:00][speech] two"]);
        assert_eq!(summary, "Aelith 2024-05-02: 1 lines");
        assert!(day_path(&dir, "2024-05-02", "Aelith").exists());
    }

    #[test]
    fn the_summary_counts_days_and_lines() {
        let dir = fixture("b");
        let summary = summary(&dir, "Aelith");
        assert_eq!(summary[0], "Aelith's days: 2");
        assert_eq!(summary[1], "Aelith on 2024-05-01: 1");
        assert_eq!(summary[2], "Aelith on 2024-05-02: 1");
        assert_eq!(summary.len(), 3);
    }
}
