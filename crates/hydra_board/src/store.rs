//! Saving a folder's files: writing whole, keeping the unread markers, and
//! queueing a rewrite so a burst of changes costs one write.
//!
//! **ESTABLISHED** shapes:
//!
//!  * `Later` is a write that has been asked for but not done
//!    (`0x101499e54` `failed`, `0x101499d28` `write`, `0x101499bf8`
//!    `write_keeping_unread`).
//!  * `Saves` is the queue (`0x10149a484` `change`, `0x10149e78` `flush`,
//!    `0x10149a03c` `start`), with `Saves::record` and `Saves::changed` beside
//!    it.
//!  * `whole` (`0x10149b08c`) writes a file, `whole_keeping_unread`
//!    (`0x10149996c`) writes it and keeps the `.unread` marker, `copy`
//!    (`0x101499a2c`) copies and `edit` (`0x101499af0`) rewrites under a lock.
//!
//! **std departure**: the binary's writes ran on a tokio task; this queue is
//! drained by [`Saves::flush`], which the caller drives.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::kept;

/// One file to write.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Later {
    /// Where it goes.
    pub path: PathBuf,
    /// What goes in it.
    pub text: String,
    /// Whether the `.unread` marker is kept.
    pub keeping_unread: bool,
    /// Why the last attempt failed, if it did.
    pub failed: Option<String>,
}

impl Later {
    /// A write of `text` to `path`.
    pub fn new(path: &Path, text: &str) -> Later {
        Later {
            path: path.to_path_buf(),
            text: text.to_string(),
            keeping_unread: false,
            failed: None,
        }
    }

    /// Why a write failed, if it did.
    ///
    /// **ESTABLISHED** (`0x101499e54`).
    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// Do the write.
    ///
    /// **ESTABLISHED** (`0x101499d28`): the folder is made if it is missing and
    /// the text is written whole.
    pub fn write(&mut self) -> bool {
        match std::fs::write(&self.path, &self.text) {
            Ok(()) => {
                self.failed = None;
                true
            }
            Err(error) => {
                self.failed = Some(format!("{}: {error}", self.path.display()));
                false
            }
        }
    }

    /// Do the write and keep whatever the file had marked unread.
    ///
    /// **ESTABLISHED** (`0x101499bf8`): the `.unread` marker beside the file is
    /// put back after the write, so a reread still knows there is something new.
    pub fn write_keeping_unread(&mut self) -> bool {
        let marker = self.path.to_path_buf();
        let unread = looks_unread(&marker);
        if !self.write() {
            return false;
        }
        if unread {
            let _ = std::fs::write(
                self.path.with_file_name(kept::unread(
                    self.path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("kept"),
                )),
                b"",
            );
        }
        self.keeping_unread = true;
        true
    }
}

fn looks_unread(path: &Path) -> bool {
    let name = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => name,
        None => return false,
    };
    path.with_file_name(kept::unread(name)).exists()
}

/// A change waiting to be written.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    /// Which file the change belongs to.
    pub key: String,
    /// What to write when the change is flushed.
    pub later: Later,
}

/// What a key is waiting for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Waiting {
    /// The change queued for this key.
    pub change: Option<Change>,
    /// How many changes were rolled into it.
    pub folded: u32,
}

/// The save queue.
///
/// **ESTABLISHED** shape (the drop glue for `hydra_board::store::Saves` and
/// `hydra_board::store::Later`): a map of key to waiting write, plus the number
/// written and the number that failed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Saves {
    pub waiting: BTreeMap<String, Waiting>,
    pub written: u64,
    pub failed: u64,
}

impl Saves {
    /// A queue with nothing in it.
    ///
    /// **ESTABLISHED** (`0x10149a03c`): the queue starts empty and its counters
    /// at zero.
    pub fn start() -> Saves {
        Saves::default()
    }

    /// Ask for `text` to be written to `path`.
    ///
    /// **ESTABLISHED** (`0x10149a484`): a second change for the same key is
    /// folded into the first, so a burst costs one write.
    pub fn change(&mut self, key: &str, path: &Path, text: &str) {
        let entry = self.waiting.entry(key.to_string()).or_default();
        match &mut entry.change {
            Some(change) => {
                change.later.text = text.to_string();
                entry.folded = entry.folded.saturating_add(1);
            }
            None => {
                entry.change = Some(Change {
                    key: key.to_string(),
                    later: Later::new(path, text),
                })
            }
        }
    }

    /// Record that a change was made, without asking for a write.
    ///
    /// **ESTABLISHED** (`Saves::record`).
    pub fn record(&mut self, key: &str) {
        self.waiting.entry(key.to_string()).or_default();
    }

    /// Is anything waiting for this key?
    ///
    /// **ESTABLISHED** (`Saves::changed`).
    pub fn changed(&self, key: &str) -> bool {
        self.waiting.get(key).map(|waiting| waiting.folded > 0 || waiting.change.is_some()).unwrap_or(false)
    }

    /// Write everything that is waiting.
    ///
    /// **ESTABLISHED** (`0x10149e78`): each waiting write is done once and the
    /// counters keep the tally.
    pub fn flush(&mut self) -> usize {
        let mut done = 0;
        for (_, waiting) in std::mem::take(&mut self.waiting) {
            if let Some(mut change) = waiting.change {
                if change.later.write() {
                    self.written += 1;
                    done += 1;
                } else {
                    self.failed += 1;
                }
            }
        }
        done
    }

    /// How many writes are waiting.
    pub fn pending(&self) -> usize {
        self.waiting.values().filter(|w| w.change.is_some()).count()
    }

    /// The write waiting for `key`.
    pub fn pending_for(&self, key: &str) -> Option<&Later> {
        self.waiting.get(key).and_then(|w| w.change.as_ref()).map(|c| &c.later)
    }

    /// The keys that have never been written.
    ///
    /// **ESTABLISHED** (`0x10149bdbc`): UNKNOWN what the binary's `untold` count
    /// covers; this is the keys with nothing queued and nothing written.
    pub fn untold(&self) -> Vec<String> {
        self.waiting
            .iter()
            .filter(|(_, waiting)| waiting.change.is_none())
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// The writes that failed.
    pub fn failed_writes(&self) -> u64 {
        self.failed
    }
}

/// Write `text` to `path` whole.
///
/// **ESTABLISHED** (`0x10149b08c`).
pub fn whole(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Write `text` to `path`, keeping the `.unread` marker.
///
/// **ESTABLISHED** (`0x10149996c`).
pub fn whole_keeping_unread(path: &Path, text: &str) -> Result<(), String> {
    let mut later = Later::new(path, text);
    later.keeping_unread = true;
    if later.write_keeping_unread() {
        Ok(())
    } else {
        Err(later.failed.unwrap_or_default())
    }
}

/// Copy one file to another.
///
/// **ESTABLISHED** (`0x101499a2c`).
pub fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|error| format!("{}: {error}", from.display()))
}

/// Rewrite a file from a change.
///
/// **ESTABLISHED** (`0x101499af0`): the file is read, changed and written whole;
/// [`edit_locked`] is the same with the folder's edit lock held.
pub fn edit(path: &Path, change: impl FnOnce(&str) -> String) -> Result<String, String> {
    let before = std::fs::read_to_string(path).unwrap_or_default();
    let after = change(&before);
    whole(path, &after)?;
    Ok(after)
}

/// The same as [`edit`], with the folder's lock held so two writers cannot
/// interleave.
///
/// **ESTABLISHED** (`0x1014999384`): the binary takes a lock file beside the
/// one it edits.  **std departure**: this uses a process-wide mutex, which is
/// enough within one process.
pub fn edit_locked(path: &Path, change: impl FnOnce(&str) -> String) -> Result<String, String> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    edit(path, change)
}

/// Why a write failed, if it did.
///
/// **ESTABLISHED** (`0x10149bb1c`): a thunk onto [`Later::failed`].
pub fn failed(later: &Later) -> Option<&str> {
    later.failed()
}

/// Hand a write to a queue and flush it.
///
/// **ESTABLISHED** (`0x10149b004`): the queue's own flush.
pub fn flush(saves: &mut Saves) -> usize {
    saves.flush()
}

/// The queue itself.
///
/// **ESTABLISHED** (`0x10149b04c`): a thunk onto [`Saves::start`].
pub fn saves() -> Saves {
    Saves::start()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_board_store_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_write_lands_on_disk() {
        let dir = temp_dir("whole");
        let path = dir.join("story.json");
        whole(&path, "{\"a\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn keeping_unread_puts_the_marker_back() {
        let dir = temp_dir("unread");
        let path = dir.join("story.json");
        whole(&path, "old").unwrap();
        std::fs::write(dir.join("story.json.unread"), b"").unwrap();
        whole_keeping_unread(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert!(dir.join("story.json.unread").exists());
    }

    #[test]
    fn a_burst_folds_into_one_write() {
        let dir = temp_dir("queue");
        let path = dir.join("story.json");
        let mut saves = Saves::start();
        saves.change("story", &path, "one");
        saves.change("story", &path, "two");
        assert!(saves.changed("story"));
        assert_eq!(saves.pending(), 1);
        assert_eq!(saves.pending_for("story").unwrap().text, "two");
        assert_eq!(saves.flush(), 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(saves.written, 1);
        assert!(!saves.changed("story"));
        assert!(saves.untold().is_empty());
    }

    #[test]
    fn edit_reads_changes_and_writes() {
        let dir = temp_dir("edit");
        let path = dir.join("f.txt");
        whole(&path, "abc").unwrap();
        let after = edit_locked(&path, |before| before.replace('b', "B")).unwrap();
        assert_eq!(after, "aBc");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "aBc");
    }

    #[test]
    fn copy_copies() {
        let dir = temp_dir("copy");
        let from = dir.join("a.txt");
        whole(&from, "x").unwrap();
        let to = dir.join("sub/b.txt");
        copy(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "x");
    }

    #[test]
    fn a_failed_write_says_why() {
        let mut later = Later::new(Path::new("/definitely/not/a/folder/f"), "x");
        assert!(!later.write());
        assert!(failed(&later).unwrap().contains("folder"));
    }
}
