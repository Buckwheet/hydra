//! Importing sound files for a highlight rule.
//!
//! `highlight::sound::imported` @0x10195fef4 (232 B) and `refused`
//! @0x10195f870 (1.7 KB).  The pair is the import result: a file either lands
//! in the sounds folder and gets an `Imported { path }`, or it is refused with
//! the reason.

use std::path::{Path, PathBuf};

/// A sound that was copied in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    /// Where the sound now lives.
    pub path: PathBuf,
    /// The name the rule refers to it by.
    pub name: String,
}

/// Why a sound was not accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The file was not a sound the player understands.
    NotASound { extension: String },
    /// The file could not be read.
    Unreadable { message: String },
    /// The name was already taken.
    Duplicate { name: String },
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::NotASound { extension } => {
                write!(f, "`.{extension}` is not a sound Hydra can play")
            }
            Refused::Unreadable { message } => write!(f, "cannot read that sound: {message}"),
            Refused::Duplicate { name } => write!(f, "there is already a sound called {name}"),
        }
    }
}

/// The extensions the sound player accepts.
pub const SOUND_EXTENSIONS: [&str; 4] = ["wav", "ogg", "oga", "mp3"];

/// `@0x10195fef4` — the import record for a sound that was accepted.
///
/// `@0x10195fef4` in the binary just builds the struct; the extension check
/// lives in [`refused`].
pub fn imported(path: &Path) -> Imported {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Imported {
        path: path.to_path_buf(),
        name,
    }
}

/// `@0x10195f870` — why a sound would be refused, if it would.
pub fn refused(path: &Path, existing: &[String]) -> Option<Refused> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !SOUND_EXTENSIONS.contains(&ext.as_str()) {
        return Some(Refused::NotASound { extension: ext });
    }
    if !path.is_file() {
        return Some(Refused::Unreadable {
            message: "no such file".into(),
        });
    }
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if existing.iter().any(|e| *e == name) {
        return Some(Refused::Duplicate { name });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_takes_the_stem_as_name() {
        let i = imported(Path::new("/tmp/Orc Death.wav"));
        assert_eq!(i.name, "Orc Death");
        assert_eq!(i.path, PathBuf::from("/tmp/Orc Death.wav"));
    }

    #[test]
    fn bad_extensions_are_refused() {
        let r = refused(Path::new("/tmp/x.txt"), &[]).unwrap();
        assert_eq!(
            r,
            Refused::NotASound {
                extension: "txt".into()
            }
        );
        assert_eq!(r.to_string(), "`.txt` is not a sound Hydra can play");
    }

    #[test]
    fn good_extensions_still_need_the_file() {
        let r = refused(Path::new("/definitely/not/here.wav"), &[]).unwrap();
        assert!(matches!(r, Refused::Unreadable { .. }));
    }

    #[test]
    fn duplicate_names_are_refused() {
        let dir = std::env::temp_dir();
        let path = dir.join("hydra_ui_sound_test.ogg");
        std::fs::write(&path, b"x").unwrap();
        assert!(refused(&path, &[]).is_none());
        let r = refused(&path, &["hydra_ui_sound_test".to_string()]).unwrap();
        assert!(matches!(r, Refused::Duplicate { .. }));
        let _ = std::fs::remove_file(&path);
    }
}
