//! Copying a data folder: the files Hydra owns, read back and written again.
//!
//! **ESTABLISHED**:
//!
//!  * `Disk` holds the folder's state and a `Job` for the work in hand
//!    (`hydra_board::disk::Job`, `hydra_board::disk::State`).
//!  * `look` (`0x1014981d0`) reads the folder, `look_again` (`0x1014977ac`)
//!    rereads it, `remake` (`0x101498ecc`) writes the set again and `start`
//!    (`0x101498b7c`) begins a copy.
//!  * `hydras` (`0x101498e8c`) is a 16-byte thunk onto
//!    [`crate::pad::file::ControllerFile::hydras`].
//!
//! **std departure**: the copy ran on a tokio task in the binary; here
//! [`Disk::start`] does the work as it is called and reports progress through
//! [`State`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A file Hydra owns, and whether a folder has it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entry {
    /// The file's name in the folder.
    pub name: String,
    /// Its size, when it is there.
    pub size: u64,
    /// Whether it is there at all.
    pub there: bool,
}

/// The files a folder is expected to have.
///
/// **ESTABLISHED** (the literals the copy reads: `controller.toml`,
/// `controller_wheels`, `window.toml`, `hotbars.toml`, `macros.toml`,
/// `highlights.json`, `chips.toml`, `targets.toml`).
pub const FILES: [&str; 8] = [
    "controller.toml",
    "controller_wheels",
    "window.toml",
    "hotbars.toml",
    "macros.toml",
    "highlights.json",
    "chips.toml",
    "targets.toml",
];

/// The work in hand.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Job {
    /// Nothing to do.
    #[default]
    Idle,
    /// Reading a folder.
    Looking,
    /// Copying one folder to another.
    Copying { from: PathBuf, to: PathBuf },
    /// Writing the files a folder is missing.
    Remaking,
    /// Finished, with what happened.
    Done(String),
    /// It failed, and why.
    Failed(String),
}

/// What a folder holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The files, by name.
    pub files: BTreeMap<String, Entry>,
    /// What the last look found.
    pub said: String,
}

/// The disk page's state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Disk {
    pub state: State,
    pub job: Job,
}

impl Disk {
    /// The default controller file, so a folder can be given one.
    ///
    /// **ESTABLISHED** (`0x101498e8c`): a thunk onto
    /// [`crate::pad::file::ControllerFile::hydras`].
    pub fn hydras() -> &'static str {
        crate::pad::file::ControllerFile::hydras()
    }

    /// Read a folder.
    ///
    /// **ESTABLISHED** (`0x1014981d0`): each of the files is looked for and its
    /// size noted.
    pub fn look(&mut self, folder: &Path) -> State {
        self.job = Job::Looking;
        let mut files = BTreeMap::new();
        for name in FILES {
            let path = folder.join(name);
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            files.insert(
                name.to_string(),
                Entry {
                    name: name.to_string(),
                    size,
                    there: size > 0 || path.exists(),
                },
            );
        }
        let there = files.values().filter(|entry| entry.there).count();
        self.state = State {
            files,
            said: format!("{there} of {} files are here.", FILES.len()),
        };
        self.job = Job::Idle;
        self.state.clone()
    }

    /// Read the folder again.
    ///
    /// **ESTABLISHED** (`0x1014977ac`): the same look, kept as its own entry so
    /// the page can tell the two apart.
    pub fn look_again(&mut self, folder: &Path) -> State {
        self.look(folder)
    }

    /// Give a folder the files it is missing.
    ///
    /// **ESTABLISHED** (`0x101498ecc`): the files that are not there are
    /// written, starting with the default controller file.
    pub fn remake(&mut self, folder: &Path) -> Result<Vec<String>, String> {
        self.job = Job::Remaking;
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("{}: {error}", folder.display()))?;
        let mut wrote = Vec::new();
        for name in FILES {
            let path = folder.join(name);
            if path.exists() {
                continue;
            }
            let text = if name == "controller.toml" {
                Disk::hydras()
            } else {
                ""
            };
            std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
            wrote.push(name.to_string());
        }
        self.job = Job::Done(format!("{} files were written.", wrote.len()));
        Ok(wrote)
    }

    /// Copy one folder to another.
    ///
    /// **ESTABLISHED** (`0x101498b7c`): each of the files is copied when it is
    /// there; a file that will not copy is reported.
    pub fn start(&mut self, from: &Path, to: &Path) -> Result<usize, String> {
        self.job = Job::Copying {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
        };
        std::fs::create_dir_all(to).map_err(|error| format!("{}: {error}", to.display()))?;
        let mut copied = 0;
        for name in FILES {
            let source = from.join(name);
            if !source.exists() {
                continue;
            }
            let target = to.join(name);
            crate::store::copy(&source, &target)?;
            copied += 1;
        }
        self.job = Job::Done(format!("{copied} files were copied."));
        Ok(copied)
    }

    /// What the copy is doing.
    pub fn job(&self) -> &Job {
        &self.job
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_board_disk_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_look_counts_the_files() {
        let dir = temp_dir("look");
        std::fs::write(dir.join("window.toml"), "x").unwrap();
        let mut disk = Disk::default();
        let state = disk.look(&dir);
        assert_eq!(state.files.len(), FILES.len());
        assert!(state.files["window.toml"].there);
        assert!(!state.files["chips.toml"].there);
        assert_eq!(state.said, "1 of 8 files are here.");
        assert_eq!(disk.job(), &Job::Idle);
    }

    #[test]
    fn remake_writes_what_is_missing() {
        let dir = temp_dir("remake");
        std::fs::write(dir.join("window.toml"), "kept").unwrap();
        let mut disk = Disk::default();
        let wrote = disk.remake(&dir).unwrap();
        assert_eq!(wrote.len(), FILES.len() - 1);
        assert_eq!(
            std::fs::read_to_string(dir.join("controller.toml")).unwrap(),
            Disk::hydras()
        );
        // The file that was there is untouched.
        assert_eq!(
            std::fs::read_to_string(dir.join("window.toml")).unwrap(),
            "kept"
        );
    }

    #[test]
    fn start_copies_the_files_that_are_there() {
        let from = temp_dir("from");
        let to = temp_dir("to");
        std::fs::write(from.join("macros.toml"), "m").unwrap();
        let mut disk = Disk::default();
        assert_eq!(disk.start(&from, &to).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(to.join("macros.toml")).unwrap(), "m");
        assert!(matches!(disk.job(), Job::Done(_)));
    }
}
