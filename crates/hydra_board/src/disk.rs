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

    /// A cache of values built once and kept, the way a page keeps its textures.
    ///
    /// This is the constructor of the recovered `Disk::kept_at` cache; the cache
    /// itself is [`Kept`], generic over what it keeps, and [`Textures`] /
    /// [`Pictures`] are the two instantiations the binary names
    /// (`docs/EGUI.md` §3.5).
    pub fn kept<T>() -> Kept<T> {
        Kept::new()
    }
}

/// A cache that builds a value once at a key and keeps it until it is dropped.
///
/// **ESTABLISHED** (`Disk::kept_at`, `docs/EGUI.md` §3.5): the binary's cache is
/// generic over the value it keeps — its drop glue names `epaint::TextureHandle`
/// and `(epaint::TextureHandle, Arc<[f32]>)`.  Keeping the handles alive *is* the
/// job: `epaint` frees a texture when the last [`epaint::TextureHandle`] for it is
/// dropped, so a page that rebuilt its texture every frame would thrash the
/// atlas.
pub struct Kept<T> {
    at: BTreeMap<String, T>,
}

impl<T> Kept<T> {
    /// An empty cache.
    pub fn new() -> Kept<T> {
        Kept {
            at: BTreeMap::new(),
        }
    }

    /// The value kept at `key`, building it with `make` when it is not there.
    ///
    /// **ESTABLISHED** (`kept_at`): the second call with the same key does not
    /// build again — that is what keeps one texture handle alive across frames.
    pub fn kept_at(&mut self, key: &str, make: impl FnOnce() -> T) -> &T {
        self.at
            .entry(key.to_string())
            .or_insert_with(make)
    }

    /// The value kept at `key`, if any.
    pub fn get(&self, key: &str) -> Option<&T> {
        self.at.get(key)
    }

    /// Keep `value` at `key`, giving back what was there.
    pub fn insert(&mut self, key: &str, value: T) -> Option<T> {
        self.at.insert(key.to_string(), value)
    }

    /// Forget `key`, dropping its value (and so freeing a texture).
    pub fn remove(&mut self, key: &str) -> Option<T> {
        self.at.remove(key)
    }

    /// How many values are kept.
    pub fn len(&self) -> usize {
        self.at.len()
    }

    /// Is the cache empty?
    pub fn is_empty(&self) -> bool {
        self.at.is_empty()
    }

    /// The keys kept, in order.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.at.keys()
    }

    /// Drop everything kept.
    pub fn clear(&mut self) {
        self.at.clear();
    }
}

impl<T> Default for Kept<T> {
    fn default() -> Kept<T> {
        Kept::new()
    }
}

impl<T> std::fmt::Debug for Kept<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kept").field("keys", &self.at.keys()).finish()
    }
}

impl<T: Clone> Clone for Kept<T> {
    fn clone(&self) -> Kept<T> {
        Kept {
            at: self.at.clone(),
        }
    }
}

impl<T: PartialEq> PartialEq for Kept<T> {
    fn eq(&self, other: &Kept<T>) -> bool {
        self.at == other.at
    }
}

/// The textures a page keeps: the binary's own instantiation of [`Kept`].
pub type Textures = Kept<epaint::TextureHandle>;

/// Textures with the pixels they were built from: the binary's other one.
pub type Pictures = Kept<(epaint::TextureHandle, std::sync::Arc<[f32]>)>;

impl Kept<epaint::TextureHandle> {
    /// The texture kept at `key`, loading `image` into `ctx` the first time.
    ///
    /// A [`epaint::TextureHandle`] from [`egui::Context::load_texture`] is the
    /// only kind this crate can name, so this is the constructor a page uses.
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        image: impl Into<egui::ImageData>,
        options: egui::TextureOptions,
    ) -> &epaint::TextureHandle {
        self.kept_at(key, || ctx.load_texture(key, image, options))
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

    #[test]
    fn kept_at_builds_once_and_keeps_the_value() {
        let mut kept: Kept<u32> = Disk::kept();
        assert!(kept.is_empty());
        let mut built = 0;
        for _ in 0..3 {
            let value = kept.kept_at("count", || {
                built += 1;
                7
            });
            assert_eq!(*value, 7);
        }
        assert_eq!(built, 1, "built once, kept after that");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept.keys().collect::<Vec<_>>(), vec!["count"]);
        // A second key is its own value.
        assert_eq!(*kept.kept_at("other", || 9), 9);
        assert_eq!(kept.len(), 2);
        // Inserting replaces, removing drops.
        assert_eq!(kept.insert("other", 11), Some(9));
        assert_eq!(kept.get("other"), Some(&11));
        assert_eq!(kept.remove("other"), Some(11));
        assert!(kept.get("other").is_none());
        kept.clear();
        assert!(kept.is_empty());
    }

    #[test]
    fn a_kept_texture_is_the_same_texture_twice() {
        let ctx = egui::Context::default();
        let mut textures: Textures = Kept::new();
        let image = || egui::ColorImage::filled([2, 2], egui::Color32::from_rgb(1, 2, 3));
        let mut ids = Vec::new();
        // Inside a frame, which is where a page builds its textures.
        let out = ctx.run_ui(crate::testkit::input(crate::testkit::SCREEN), |ui| {
            let _ = ui;
            for _ in 0..2 {
                let handle = textures.texture(
                    &ctx,
                    "portrait",
                    image(),
                    egui::TextureOptions::LINEAR,
                );
                ids.push(handle.id());
                assert_eq!(handle.size(), [2, 2]);
            }
        });
        let _ = out;
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], ids[1], "the same handle is kept, not rebuilt");
        assert_eq!(textures.len(), 1);
        // The kept handle is a live texture: it was uploaded this frame.
        let uploaded: Vec<_> = out
            .textures_delta
            .set
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert!(uploaded.contains(&ids[0]), "{uploaded:?}");
        // Dropping it frees the texture, which is why the cache keeps it.
        drop(textures.remove("portrait"));
        assert!(textures.is_empty());
    }

    #[test]
    fn a_copy_of_the_cache_keeps_the_same_handle() {
        let ctx = egui::Context::default();
        let mut textures: Textures = Kept::default();
        ctx.run_ui(crate::testkit::input(crate::testkit::SCREEN), |ui| {
            let _ = ui;
            let handle = textures.texture(
                &ctx,
                "emblem",
                egui::ColorImage::filled([1, 1], egui::Color32::WHITE),
                egui::TextureOptions::NEAREST,
            );
            let id = handle.id();
            let copy = textures.clone();
            assert_eq!(copy.get("emblem").map(|handle| handle.id()), Some(id));
        });
        // `Kept` compares by what it keeps.
        assert_eq!(textures, textures.clone());
    }
}
