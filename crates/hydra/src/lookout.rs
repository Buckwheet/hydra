//! `hydra lookout seed`: the player logs taught to the lookout, with Hydra
//! stopped.
//!
//! The recovered handler is `hydra::lookout_seed` (`0x10021d088`):
//!
//! 1. [`hydra_table::account::data_dir`];
//! 2. `hydra::instance::claim` — a lock on the data folder.  If another Hydra
//!    owns it the recovered sentence is printed and the command refuses:
//!    `{data}, Hydra is running on this data folder: shut it down, then seed the
//!    lookout.` (`__const` 0x10206f84c, leading comma included);
//! 3. the player-log root ([`hydra_net::sink::config::log_dir`]) and today's
//!    directory ([`hydra_net::sink::config::date_dir`]);
//! 4. `hydra_session::lookout_seed::seed` for the logs it finds.
//!
//! This crate's `seed` returns the newest day's lines and a summary; a new
//! session is never actually started by this headless command, so the lines are
//! read and summarised, not replayed.  The recovered summary shapes are
//! `{}'s days: {}`, `{} on {}: {}`, and `{} {}: {} lines`
//! ([`hydra_session::lookout_seed`]), and `No player logs to teach the lookout
//! from.` (`__const` 0x10206f7f8) is printed when the log root is empty.
//!
//! **UNKNOWN:** the original's per-character final line combines a date range,
//! the learned/still-learning counts and the file size (`{}: {}\n lines of {}
//! ({}); {}\n learned, {} still learning, {} bytes.\n` at 0x103e25384).  The
//! counts live in a `Seeded` record this reconstruction does not have; only the
//! recovered summary lines are printed.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use hydra_session::lookout_seed;

/// The lock file `hydra::instance::claim` uses (`0x10206bcb1`).
pub const LOCK_FILE: &str = "hydra.lock";

/// The recovered "no logs" sentence.
pub const NO_LOGS: &str = "No player logs to teach the lookout from.";

/// Why a seed could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Another Hydra owns the data folder.
    Running(PathBuf),
    /// The data folder could not be made or locked.
    DataFolder(PathBuf, String),
}

impl Refusal {
    /// The recovered sentence for this refusal.
    pub fn message(&self) -> String {
        match self {
            Refusal::Running(data) => format!(
                "{}, Hydra is running on this data folder: shut it down, then seed the lookout.",
                data.display()
            ),
            Refusal::DataFolder(path, error) => {
                format!("{} cannot be made: {error}", path.display())
            }
        }
    }
}

/// `hydra lookout seed`.
pub fn seed() -> Result<(), String> {
    let data = hydra_table::account::data_dir();
    let _claim = claim(&data).map_err(|refusal| refusal.message())?;
    seed_logs(&hydra_net::sink::config::log_dir()).map_err(|error| error.to_string())
}

/// Take the data folder's lock without waiting: `hydra::instance::claim`'s first
/// two steps.  The returned guard keeps the lock until the command ends.
fn claim(data: &Path) -> Result<File, Refusal> {
    fs::create_dir_all(data)
        .map_err(|error| Refusal::DataFolder(data.to_path_buf(), error.to_string()))?;
    let path = data.join(LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|error| Refusal::DataFolder(path.clone(), error.to_string()))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err(Refusal::Running(data.to_path_buf())),
        Err(std::fs::TryLockError::Error(error)) => {
            Err(Refusal::DataFolder(path, error.to_string()))
        }
    }
}

/// Teach the lookout from every character directory under `root`.
///
/// The original's final report is one line per character; the recovered summary
/// shapes are used here ([`lookout_seed::summary`]).
pub fn seed_logs(root: &Path) -> std::io::Result<()> {
    let mut characters: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                characters.push(path);
            }
        }
    }
    characters.sort();

    if characters.is_empty() {
        println!("{NO_LOGS}");
        return Ok(());
    }

    for dir in characters {
        let character = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (lines, first) = lookout_seed::seed(&dir, &character);
        println!("{first}");
        if lines.is_empty() {
            println!("{character}: no new days");
            continue;
        }
        for line in lookout_seed::summary(&dir, &character).iter().skip(1) {
            println!("{line}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "hydra-lookout-{tag}-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_held_lock_is_the_recovered_running_sentence() {
        let dir = TempDir::new("lock");
        let held = claim(&dir.0).expect("the first claim");
        let refusal = claim(&dir.0).unwrap_err();
        assert!(matches!(refusal, Refusal::Running(_)));
        assert_eq!(
            refusal.message(),
            format!(
                "{}, Hydra is running on this data folder: shut it down, then seed the lookout.",
                dir.0.display()
            )
        );
        drop(held);
        assert!(claim(&dir.0).is_ok());
    }

    #[test]
    fn no_character_directories_prints_the_recovered_sentence() {
        let dir = TempDir::new("empty");
        seed_logs(&dir.0).unwrap();
        assert_eq!(NO_LOGS, "No player logs to teach the lookout from.");
    }
}
