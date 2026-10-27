//! The per-character state file: what the game told us about this character.
//!
//! Recovered from `character_store::{store_path, Persistence::{mark, take},
//! merge_and_save, load, save}` at `0x101aac818`, `0x101aac828`,
//! `0x101aac8a0`, `0x101aac8fc`, `0x101aacd5c`, `0x101aad268`.
//!
//! **ESTABLISHED**:
//! * `store_path` is a tail call into `store::character_path` with the
//!   extension literal at `0x1039c765a`, 5 bytes — `.json`.
//! * `load` references only the `"{}/{}"` template (`0x103e23b2f`), the same
//!   error shape `settings_store::load` uses, so the file is read with
//!   `fs::read_to_string` and a missing file is not an error.
//! * `save` writes through `store::save_bytes` (it is one of the three callers
//!   in the binary: `menu_store`, `character_store` and `settings_store`).
//! * `merge_and_save` merges the in-memory state into what is on disk before
//!   writing, which is the same protocol `menu_store::merge_and_save` uses for
//!   the learned-command table.
//! * `Persistence::{mark, take}` are the dirty flag: `mark` sets it and `take`
//!   clears and returns it (`mark` is 16 instructions of `Arc` +
//!   `AtomicBool::swap`, `take` is 18).
//!
//! **UNKNOWN**: the JSON member names of the character state.  We keep a
//! section map, exactly like [`crate::settings_store::SettingsFile::sections`],
//! because that is all the crate needs to carry.

use crate::store;
use crate::textfmt::{self, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The extension of a character state file (**ESTABLISHED**, 5 bytes).
pub const EXT: &str = ".json";
/// The highest schema this build reads.
pub const SCHEMA_VERSION: u32 = 1;

/// One character's stored state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterFile {
    /// the character the state belongs to
    pub character: String,
    /// the sections, key order preserved
    pub sections: Vec<(String, Value)>,
    /// the schema the file was written with
    pub schema_version: u32,
}

impl CharacterFile {
    /// A fresh file for `character`.
    pub fn new(character: &str) -> Self {
        CharacterFile {
            character: character.to_string(),
            sections: Vec::new(),
            schema_version: SCHEMA_VERSION,
        }
    }

    /// Look a section up.
    pub fn section(&self, name: &str) -> Option<&Value> {
        self.sections
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }

    /// Insert or replace a section.
    pub fn set(&mut self, name: &str, value: Value) {
        match self.sections.iter_mut().find(|(k, _)| k == name) {
            Some((_, slot)) => *slot = value,
            None => self.sections.push((name.to_string(), value)),
        }
    }

    /// Merge `other` into `self`; `other`'s sections win.
    ///
    /// **ESTABLISHED** shape of `merge_and_save`: read, merge, write.
    pub fn merge(&mut self, other: &CharacterFile) {
        for (key, value) in &other.sections {
            self.set(key, value.clone());
        }
        if !other.character.is_empty() {
            self.character = other.character.clone();
        }
    }

    /// The JSON text of the file.
    pub fn to_json(&self) -> String {
        textfmt::to_json(&Value::Obj(vec![
            (
                "schema_version".to_string(),
                Value::Num(self.schema_version as f64),
            ),
            ("character".to_string(), Value::Str(self.character.clone())),
            ("sections".to_string(), Value::Obj(self.sections.clone())),
        ]))
    }

    /// Read the file out of parsed JSON.
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let schema_version = v.u64_of("schema_version").unwrap_or(0) as u32;
        if schema_version > SCHEMA_VERSION {
            return Err(format!(
                "character.json is schema {schema_version}, and this build of Hydra reads up to \
                 {SCHEMA_VERSION}"
            ));
        }
        Ok(CharacterFile {
            character: v.str_of("character"),
            sections: match v.get("sections") {
                Some(Value::Obj(pairs)) => pairs.clone(),
                _ => Vec::new(),
            },
            schema_version,
        })
    }
}

/// `<root>/<instance>_<character>.json`.
///
/// **ESTABLISHED** (`0x101aac818`): a tail call into `character_path` with the
/// 5-byte `.json` extension.
pub fn store_path(root: &Path, instance: &str, character: &str) -> Option<PathBuf> {
    store::character_path(root, instance, character, EXT)
}

/// Read a character file.  A missing file is an empty one.
pub fn load(root: &Path, instance: &str, character: &str) -> Result<CharacterFile, String> {
    let Some(path) = store_path(root, instance, character) else {
        return Ok(CharacterFile::new(character));
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CharacterFile::new(character))
        }
        Err(e) => return Err(format!("{}/{}: {e}", root.display(), path.display())),
    };
    let v = textfmt::parse_json(&text).map_err(|e| format!("{}/{}: {e}", root.display(), path.display()))?;
    CharacterFile::from_json(&v).map_err(|e| format!("{}/{}: {e}", root.display(), path.display()))
}

/// Write a character file through [`store::save_bytes`].
pub fn save(root: &Path, file: &CharacterFile, instance: &str) -> Result<(), String> {
    let Some(path) = store_path(root, instance, &file.character) else {
        return Err("that character name has nothing usable in it".to_string());
    };
    store::save_bytes(root, &path, file.to_json().as_bytes())
        .map_err(|e| format!("{} was not saved: {e}", path.display()))
}

/// Merge `file` into what is on disk and write the result.
///
/// **ESTABLISHED** shape (`0x101aac8fc`): load, merge, save.
pub fn merge_and_save(
    root: &Path,
    instance: &str,
    file: &CharacterFile,
) -> Result<CharacterFile, String> {
    let mut on_disk = load(root, instance, &file.character)?;
    on_disk.merge(file);
    save(root, &on_disk, instance)?;
    Ok(on_disk)
}

/// The dirty flag a session uses to know whether the state needs saving.
///
/// **ESTABLISHED**: `mark` sets it, `take` clears and returns it.
#[derive(Debug, Clone, Default)]
pub struct Persistence {
    dirty: Arc<AtomicBool>,
}

impl Persistence {
    /// A flag that is not dirty.
    pub fn new() -> Self {
        Persistence::default()
    }

    /// Mark the state dirty.
    ///
    /// **ESTABLISHED** (`0x101aac828`).
    pub fn mark(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    /// Clear the flag and report whether it was set.
    ///
    /// **ESTABLISHED** (`0x101aac8a0`): `swap(false)`.
    pub fn take(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    /// Is the flag set, without clearing it?
    pub fn peek(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_character_path_with_json() {
        assert_eq!(crate::character_store::EXT, ".json");
        assert_eq!(
            store_path(Path::new("/d"), "gemstone", "Aelith"),
            Some(PathBuf::from("/d/gemstone_aelith.json"))
        );
        assert_eq!(store_path(Path::new("/d"), "..", "Aelith"), None);
    }

    #[test]
    fn mark_and_take_are_a_one_shot_flag() {
        let p = Persistence::new();
        assert!(!p.peek());
        assert!(!p.take());
        p.mark();
        assert!(p.peek());
        assert!(p.take());
        assert!(!p.take());
        let q = p.clone();
        p.mark();
        assert!(q.take(), "clones share the flag");
    }

    #[test]
    fn saving_merges_into_what_is_on_disk() {
        let root = std::env::temp_dir().join(format!("hydra-char-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut first = CharacterFile::new("Aelith");
        first.set("level", Value::Num(42.0));
        merge_and_save(&root, "gemstone", &first).unwrap();

        let mut second = CharacterFile::new("Aelith");
        second.set("race", Value::Str("Human".into()));
        let merged = merge_and_save(&root, "gemstone", &second).unwrap();
        assert_eq!(merged.section("level").unwrap().as_u64(), Some(42));
        assert_eq!(merged.section("race").unwrap().as_str(), Some("Human"));

        let loaded = load(&root, "gemstone", "Aelith").unwrap();
        assert_eq!(loaded, merged);
        assert_eq!(loaded.sections.len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_file_loads_empty_and_a_bad_schema_is_refused() {
        let root = std::env::temp_dir().join(format!("hydra-char2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            load(&root, "gemstone", "Aelith").unwrap(),
            CharacterFile::new("Aelith")
        );
        let v = textfmt::parse_json(r#"{"schema_version": 7}"#).unwrap();
        assert!(CharacterFile::from_json(&v).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
