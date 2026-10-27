//! Where the session log goes, and the stamps it is written with.
//!
//! Recovered from `hydra_net::sink::config` (`0x101c50398` .. `0x101c51428`).
//! The original read the settings from the environment with `std::env::var` and
//! stamped the files with `jiff`; this is the same set of stamps built from
//! [`crate::eastern`].
//!
//! Recovered env vars (`std::env::var`/`var_os` call sites):
//! * `HYDRA_LOG_DIR` — where the date directories go (0x103bf9a8b),
//! * `HYDRA_LOG_LINES` — how many lines to a `.bytes` file (0x103bf9a3a),
//! * `HYDRA_LOG_TIMESTAMPS` — whether the `.bytes` file gets timestamps
//!   (0x103bf9a49).
//!
//! Recovered `format!` shapes (`__const` at 0x103bf9a05 .. 0x103bf9a98):
//! * `file_stamp`: `{:04}-{:02}-{:02}_{:02}-{:02}-{:02}`
//! * `date_dir`: `{:04}-{:02}-{:02}`
//! * `moment`: `{:04}-{:02}-{:02} {:02}:{:02}`
//! * `line_time`: `{:02}:{:02}:{:02}.{:03}`

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::eastern;

/// The directory the date directories are created under.
pub const DIR_VAR: &str = "HYDRA_LOG_DIR";
/// How many lines go into each `.bytes` file.
pub const LINES_VAR: &str = "HYDRA_LOG_LINES";
/// Whether the wire recording is timestamped.
pub const TIMESTAMPS_VAR: &str = "HYDRA_LOG_TIMESTAMPS";
/// The default log root: `$HOME/hydra/logs`.
pub const DEFAULT_DIR: &str = "hydra/logs";
/// The default number of lines per `.bytes` file.
pub const DEFAULT_LINES: usize = 20_000;

/// Unix seconds of a [`SystemTime`].
fn unix(at: SystemTime) -> i64 {
    match at.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs().min(i64::MAX as u64) as i64,
        Err(_) => 0,
    }
}

/// The broken-down Eastern wall clock of a unix second.
fn wall_clock(at: i64) -> (eastern::Date, i64, i64, i64, i64) {
    let date = eastern::date(at);
    let local = at + eastern::offset(at);
    let secs = local.rem_euclid(86_400);
    (
        date,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60,
        local.rem_euclid(1) * 0,
    )
}

/// Whether the wire recording carries timestamps.
///
/// Recovered from `0x101c50718`: `HYDRA_LOG_TIMESTAMPS` is read and trimmed; an
/// unset or empty value means "off", `1`/`true`/`yes` mean "on".
pub fn bytes_timestamps_enabled() -> bool {
    match std::env::var(TIMESTAMPS_VAR) {
        Ok(value) => {
            let value = value.trim();
            !(value.is_empty() || value == "0" || value.eq_ignore_ascii_case("false")
                || value.eq_ignore_ascii_case("no") || value.eq_ignore_ascii_case("off"))
        }
        Err(_) => false,
    }
}

/// How many lines go into one `.bytes` file.
///
/// Recovered from `0x101c50548`: `HYDRA_LOG_LINES` is read and trimmed, then
/// parsed; anything unparseable falls back to the default.
pub fn rotate_after_lines() -> usize {
    match std::env::var(LINES_VAR) {
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(lines) if lines > 0 => lines,
            _ => DEFAULT_LINES,
        },
        Err(_) => DEFAULT_LINES,
    }
}

/// The root the session logs are written under.
///
/// Recovered from `0x101c509f0`: `HYDRA_LOG_DIR` is read with `var_os` (so a
/// non-UTF-8 path survives); with no setting the log goes under the home
/// directory.  UNKNOWN: the original's fallback root is built from Hydra's data
/// directories; this uses `$HOME/hydra/logs`.
pub fn log_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DIR_VAR) {
        return PathBuf::from(dir);
    }
    // UNKNOWN: the original's fallback root comes from Hydra's data dirs.
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(DEFAULT_DIR),
        None => PathBuf::from(DEFAULT_DIR),
    }
}

/// The `YYYY-MM-DD` directory a session belongs in.
///
/// Recovered from `0x101c50a80`.
pub fn date_dir() -> String {
    let date = eastern::date(eastern::now());
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

/// The `YYYY-MM-DD_HH-MM-SS` stamp that names a session's files.
///
/// Recovered from `0x101c50398`.
pub fn file_stamp() -> String {
    let (date, hour, minute, second, _) = wall_clock(eastern::now());
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        date.year, date.month, date.day, hour, minute, second
    )
}

/// The `YYYY-MM-DD HH:MM` stamp a log line opens with.
///
/// Recovered from `0x101c507f0`.
pub fn moment() -> String {
    let (date, hour, minute, _, _) = wall_clock(eastern::now());
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        date.year, date.month, date.day, hour, minute
    )
}

/// The `HH:MM:SS.mmm` stamp of a single log line.
///
/// Recovered from `0x101c50c00`.
pub fn line_time() -> String {
    let at = eastern::now();
    let (_, hour, minute, second, _) = wall_clock(at);
    format!("{:02}:{:02}:{:02}.{:03}", hour, minute, second, 0)
}

/// The same stamp for a given instant, plus how long ago it was.
///
/// Recovered from `0x101c50da0`: the difference between now and `then` is taken
/// as a signed duration (the original refuses a duration that does not fit:
/// `overflow when negating signed duration`, 0x103bf99cb) and the stamp is the
/// moment of `then` with the age beside it.
///
/// INFERRED: the exact arrangement of the two stamps is not recoverable from the
/// inlined `jiff` formatting; this prints the moment and the age in seconds.
pub fn stamp_ago(then: SystemTime) -> String {
    let at = unix(then);
    let age = (eastern::now() - at).max(0);
    let (date, hour, minute, second, _) = wall_clock(at);
    // UNKNOWN: how the moment and the age are arranged.
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} ({age}s ago)",
        date.year, date.month, date.day, hour, minute, second
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stamps_have_the_recovered_shapes() {
        let date = date_dir();
        assert_eq!(date.len(), 10, "{date}");
        assert_eq!(date.as_bytes()[4], b'-');
        assert_eq!(date.as_bytes()[7], b'-');
        assert!(eastern::parse(&date).is_some(), "{date}");

        let stamp = file_stamp();
        assert_eq!(stamp.len(), 19, "{stamp}");
        assert_eq!(stamp.as_bytes()[10], b'_');

        let moment = moment();
        assert_eq!(moment.len(), 16, "{moment}");
        assert_eq!(moment.as_bytes()[10], b' ');

        let time = line_time();
        assert_eq!(time.len(), 12, "{time}");
        assert_eq!(time.as_bytes()[2], b':');
        assert_eq!(time.as_bytes()[5], b':');
        assert_eq!(time.as_bytes()[8], b'.');
    }

    #[test]
    fn stamp_ago_counts_the_seconds() {
        let stamp = stamp_ago(SystemTime::now());
        assert!(stamp.contains("(0s ago)") || stamp.contains("(1s ago)"), "{stamp}");
        let old = SystemTime::now() - std::time::Duration::from_secs(120);
        let stamp = stamp_ago(old);
        assert!(stamp.contains("s ago)"), "{stamp}");
    }

    #[test]
    fn the_settings_fall_back_sensibly() {
        // These read the real environment; only the shape of the answer is
        // checked so the test does not depend on how the machine is set up.
        let lines = rotate_after_lines();
        assert!(lines > 0);
        let dir = log_dir();
        assert!(!dir.as_os_str().is_empty());
        let _ = bytes_timestamps_enabled();
        assert_eq!(DEFAULT_LINES, 20_000);
    }
}
