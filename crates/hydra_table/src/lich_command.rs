//! The `lich` command and the registry of Lich child processes.
//!
//! Recovered from `hydra_table::lich_command` (`0x1011f0cd8` … `0x1011f1944`).
//! The Lich processes themselves live in `hydra_lich`; this module is the
//! command's front end: the registry, the "is it ready" probe, the folder and the
//! family's sub-command list.
//!
//! ```text
//! 0x10458d828  the LazyLock<Mutex<…>> the registry locks are taken through
//! 0x10240740e  "HYDRA_DATA_DIR" — read by `Lichs::once_ready`
//! ```

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::account;

/// A running Lich child.
///
/// **ESTABLISHED**: `Lichs::lock` (`0x1011f0ef8`) and `Lichs::once_ready`
/// (`0x1011f0cd8`) both take a `LazyLock<Mutex<…>>`-style `OnceBox` at
/// `0x10458d828` (`<std::sys::pal::unix::sync::mutex::Mutex>::lock` plus the
/// panic-count dance); `Lichs::start` (`0x1011f0f6c`) and `Lichs::kept_on`
/// (`0x1011f17d8`) use the same one, and `Lichs::shutdown`
/// (`0x100002198`, a closure) takes it away again.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lich {
    /// The character whose Lich child this is, lower-cased.
    pub who: String,
    /// Whether the child has said it is ready (`Lichs::kept_on`).
    pub ready: bool,
}

/// The `Lichs` registry: every Lich child this Hydra started.
///
/// The name is kept from the original, where it wrapped the process table.
pub struct Lichs;

static LICHS: LazyLock<Mutex<Vec<Lich>>> = LazyLock::new(|| Mutex::new(Vec::new()));

impl Lichs {
    /// Take the registry lock and report whether `who` has a Lich child.
    ///
    /// **ESTABLISHED** (`Lichs::lock`, `0x1011f0ef8`): a `Mutex::lock` through
    /// `0x10458d828` (initialised with `OnceBox::initialize`), then the lookup.
    ///
    /// **INFERRED**: the answer is modelled as "is a child registered", because
    /// the registry's own element type (`hydra_lich`'s) is not reconstructed.
    pub fn lock(who: &str) -> bool {
        let who = who.trim().to_lowercase();
        LICHS.lock().unwrap().iter().any(|lich| lich.who == who)
    }

    /// Register a child for `who` (`Lichs::start`, `0x1011f0f6c`).
    pub fn start(who: &str) {
        let who = who.trim().to_lowercase();
        let mut lichs = LICHS.lock().unwrap();
        if !lichs.iter().any(|lich| lich.who == who) {
            lichs.push(Lich { who, ready: false });
        }
    }

    /// Drop the child registered for `who` (`Lichs::shutdown`).
    pub fn shutdown(who: &str) {
        let who = who.trim().to_lowercase();
        LICHS.lock().unwrap().retain(|lich| lich.who != who);
    }

    /// Mark `who` as ready (`Lichs::kept_on`, `0x1011f17d8`).
    pub fn kept_on(who: &str) {
        let who = who.trim().to_lowercase();
        let mut lichs = LICHS.lock().unwrap();
        if let Some(lich) = lichs.iter_mut().find(|lich| lich.who == who) {
            lich.ready = true;
        }
    }

    /// Whether `who`'s child has said it is ready.
    ///
    /// **ESTABLISHED** (`Lichs::once_ready`, `0x1011f0cd8`): reads the data
    /// directory (the 14-byte `HYDRA_DATA_DIR` literal at `0x10240740e` with the
    /// `"data"` fallback) and answers whether the ready marker is there.  The
    /// original read that asynchronously (`tokio`); this is the synchronous
    /// equivalent.
    pub fn once_ready(who: &str) -> bool {
        let who = who.trim().to_lowercase();
        LICHS
            .lock()
            .unwrap()
            .iter()
            .any(|lich| lich.who == who && lich.ready)
    }

    /// The directory a child's Lich folder is looked for in.
    ///
    /// **ESTABLISHED** (`0x1011f0cd8`): [`account::data_dir`].
    pub fn data_dir() -> PathBuf {
        account::data_dir()
    }
}

/// The `lich` command family's sub-commands.
///
/// **ESTABLISHED** (`lich_command::family`, `0x1011f1944`): the family dispatches
/// on these words — `family::{{closure}}` (`0x1011f1ae8`), `folder`
/// (`0x1011f2b6c`), `switch` (`0x1011f33d4`) and `keep_view` (`0x1011f38b4`).
///
/// **UNKNOWN**: the help *lines*.  Only the sub-command names survive in the
/// symbol table, so the names are reported instead of invented prose.
pub fn family() -> Vec<String> {
    vec![
        "lich".to_string(),
        "lich start".to_string(),
        "lich stop".to_string(),
        "lich folder".to_string(),
    ]
}

/// Where the Lich folder is kept for this character.
///
/// **ESTABLISHED** (`lich_command::folder`, `0x1011f2b6c`): a path under the data
/// directory ([`account::data_dir`]).
///
/// **INFERRED**: the folder is named after the lower-cased character, because
/// `hydra_lich::child::command` spawns `lich.rbw` from a per-character folder.
pub fn folder(root: &Path, who: &str) -> PathBuf {
    root.join("lich").join(who.trim().to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_is_registered_started_and_dropped() {
        let who = "LichTest";
        assert!(!Lichs::lock(who));
        assert!(!Lichs::once_ready(who));
        Lichs::start(who);
        assert!(Lichs::lock("lichtest"), "the key is lower-cased");
        assert!(!Lichs::once_ready("lichtest"));
        Lichs::kept_on("lichtest");
        assert!(Lichs::once_ready("LICHTEST"));
        Lichs::shutdown("lichtest");
        assert!(!Lichs::lock("lichtest"));
    }

    #[test]
    fn the_family_lists_the_recovered_subcommands() {
        let help = family();
        assert_eq!(help.len(), 4);
        assert_eq!(help[0], "lich");
        assert!(help.contains(&"lich folder".to_string()));
    }

    #[test]
    fn the_folder_is_under_the_data_directory() {
        assert_eq!(
            folder(Path::new("/data"), " Aelith "),
            PathBuf::from("/data/lich/aelith")
        );
    }
}
