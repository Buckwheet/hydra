//! The per-character settings file.
//!
//! Recovered from `settings_store::{settings_path, SettingsFile::new, load,
//! save}` (`0x101a059c0`, `0x101a058ac`, `0x101a05ae8`, `0x101a060d0`).
//!
//! **ESTABLISHED**:
//! * `settings_path` is a tail call into `store::character_path` with two
//!   `&str` arguments and a 14-byte extension at `0x1039c59cb` —
//!   `.settings.json`.  So the file is `<root>/<a>_<b>.settings.json`.
//! * the file is **JSON**, not TOML: the `Serialize` impl right after
//!   `settings_path` (`0x101a059d0`) drives `serde_json::ser::Compound` and
//!   writes exactly four keys, in this order (key literals read straight out
//!   of `__TEXT`):
//!
//!   | key | length | field offset in the struct |
//!   | --- | --- | --- |
//!   | `schema_version` | 14 | `+0x48` |
//!   | `instance` | 8 | `+0x00` |
//!   | `character` | 9 | `+0x18` |
//!   | `sections` | 8 | `+0x30` |
//!
//!   `instance` and `character` are the 24-byte `String`s `SettingsFile::new`
//!   allocates; `sections` serialises through a different `serialize_entry`
//!   monomorphisation, so it is a map (we keep an ordered list of pairs);
//!   `schema_version` is the `u32` at `+0x48`.
//! * `load` references the `{}/{}` template at `0x103e23b2f`; that is the only
//!   literal, which fits a `format!("{}/{}", dir, name)` error message rather
//!   than a `Path::join` (every other path in the crate uses `join`).
//!
//! **UNKNOWN:** the exact `sections` value type (`serde_json::Value`? a typed
//! map?).  We keep [`crate::textfmt::Value`] pairs, which round-trips.
//!
//! **Phase 1**: the file is written and read by `serde_json`.
//! [`SettingsFile`] implements `serde::Serialize` by hand so that the four keys
//! keep the recovered order (`serde`'s derive would emit them in declaration
//! order, which is *not* the order in `__TEXT`, and a `BTreeMap` map would
//! sort them), and `serde::Deserialize` through a permissive helper so a
//! missing member falls back to the default instead of failing.

use crate::store;
use crate::textfmt::Value;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serializer};
use std::path::{Path, PathBuf};

/// The extension `settings_path` appends (**ESTABLISHED**, 14 bytes).
pub const SETTINGS_EXT: &str = ".settings.json";
/// The highest `schema_version` this build understands.
pub const SCHEMA_VERSION: u32 = 1;

/// The contents of one `<world>_<character>.settings.json`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsFile {
    /// `instance` — which account/instance the settings belong to
    pub instance: String,
    /// `character`
    pub character: String,
    /// `sections` — everything else, key order preserved
    pub sections: Vec<(String, Value)>,
    /// `schema_version`
    pub schema_version: u32,
}

impl SettingsFile {
    /// A fresh settings file at the current schema version with no sections.
    ///
    /// **ESTABLISHED** shape: `SettingsFile::new` allocates two empty `String`s
    /// and stores `1` in the schema word.
    pub fn new() -> Self {
        SettingsFile {
            instance: String::new(),
            character: String::new(),
            sections: Vec::new(),
            schema_version: SCHEMA_VERSION,
        }
    }

    /// The same file, opened for a particular instance and character.
    pub fn for_character(instance: &str, character: &str) -> Self {
        let mut f = SettingsFile::new();
        f.instance = instance.to_string();
        f.character = character.to_string();
        f
    }

    /// Look a section up.
    pub fn section(&self, name: &str) -> Option<&Value> {
        self.sections
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }

    /// Insert or replace a section.
    pub fn set_section(&mut self, name: &str, value: Value) {
        match self.sections.iter_mut().find(|(k, _)| k == name) {
            Some((_, slot)) => *slot = value,
            None => self.sections.push((name.to_string(), value)),
        }
    }

    /// The JSON text of the file, key order as the binary writes it.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Read a settings file out of parsed JSON.
    pub fn from_json(v: &Value) -> Result<Self, String> {
        Self::of_value(v)
    }

    fn of_value(v: &Value) -> Result<Self, String> {
        let schema_version = v.u64_of("schema_version").unwrap_or(0) as u32;
        guard(schema_version)?;
        let sections = match v.get("sections") {
            Some(Value::Obj(pairs)) => pairs.clone(),
            _ => Vec::new(),
        };
        Ok(SettingsFile {
            instance: v.str_of("instance"),
            character: v.str_of("character"),
            sections,
            schema_version,
        })
    }
}

/// `serde::Serialize` by hand: the four recovered keys, in the recovered order,
/// with `sections` as a map.
impl serde::Serialize for SettingsFile {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(4))?;
        map.serialize_entry("schema_version", &self.schema_version)?;
        map.serialize_entry("instance", &self.instance)?;
        map.serialize_entry("character", &self.character)?;
        map.serialize_entry("sections", &Value::Obj(self.sections.clone()))?;
        map.end()
    }
}

/// The permissive shape [`SettingsFile`]'s `Deserialize` reads: every member is
/// optional, which is what the original's `#[serde(default)]` fields did.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct RawSettings {
    schema_version: u32,
    instance: String,
    character: String,
    sections: Option<Value>,
}

impl<'de> serde::Deserialize<'de> for SettingsFile {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawSettings::deserialize(d)?;
        let sections = match raw.sections {
            Some(Value::Obj(pairs)) => pairs,
            _ => Vec::new(),
        };
        Ok(SettingsFile {
            instance: raw.instance,
            character: raw.character,
            sections,
            schema_version: raw.schema_version,
        })
    }
}

/// `<root>/<safe(instance)>_<safe(character)>.settings.json`, or `None` when a
/// component has nothing usable in it.
pub fn settings_path(root: &Path, instance: &str, character: &str) -> Option<PathBuf> {
    store::character_path(root, instance, character, SETTINGS_EXT)
}

/// Read the settings file, or [`SettingsFile::new`] when it does not exist.
///
/// **ESTABLISHED**: `load` reads the file with `fs::read_to_string`, formats an
/// `io::Error` and a `serde_json::Error` into the same message shape
/// (`0x103e23b2f`, `{}/{}`), and falls back to the default file for a missing
/// path.
pub fn load(path: &Path) -> Result<SettingsFile, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            // Phase 1: the parse is `serde_json`'s, straight into the struct.
            let file: SettingsFile = serde_json::from_str(&text)
                .map_err(|e| format!("{} cannot be read: {e}", path.display()))?;
            guard(file.schema_version)
                .map_err(|e| format!("{} cannot be read: {e}", path.display()))?;
            Ok(file)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SettingsFile::new()),
        Err(e) => Err(format!("{} cannot be read: {e}", path.display())),
    }
}

/// Refuse a file from a newer build, with the recovered message.
fn guard(schema_version: u32) -> Result<(), String> {
    if schema_version > SCHEMA_VERSION {
        return Err(format!(
            "settings.json is schema {schema_version}, and this build of Hydra reads up to \
             {SCHEMA_VERSION}"
        ));
    }
    Ok(())
}

/// Write the settings file atomically through [`crate::store::save_bytes`].
///
/// **ESTABLISHED** shape: `save` serialises to a `String`, calls
/// `store::save_bytes` (it is one of the three callers in the binary), and
/// formats a failure as `format!("{} was not saved: {}", path, error)`
/// (`0x103e6ae97`).
pub fn save(path: &Path, file: &SettingsFile) -> Result<(), String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    store::save_bytes(dir, path, file.to_json().as_bytes())
        .map_err(|e| format!("{} was not saved: {e}", path.display()))
}

/// The `{}/{}` message template `load` formats (**ESTABLISHED** literal).
pub fn message_template() -> &'static str {
    "{}/{}"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_path_uses_the_recovered_extension() {
        let p = settings_path(Path::new("/d"), "GemStone IV", "Aelith").unwrap();
        assert_eq!(p, PathBuf::from("/d/gemstoneiv_aelith.settings.json"));
        assert_eq!(SETTINGS_EXT.len(), 14, "the extension literal is 14 bytes");
        assert_eq!(message_template(), "{}/{}");
    }

    #[test]
    fn json_keys_are_in_the_recovered_order() {
        let f = SettingsFile::for_character("dragonrealms", "Aelith");
        let j = f.to_json();
        assert!(j.contains("\"schema_version\": 1"));
        let iv = j.find("\"schema_version\"").unwrap();
        let i1 = j.find("\"instance\"").unwrap();
        let i2 = j.find("\"character\"").unwrap();
        let i3 = j.find("\"sections\"").unwrap();
        assert!(iv < i1 && i1 < i2 && i2 < i3, "{j}");
        let back: SettingsFile = serde_json::from_str(&j).unwrap();
        assert_eq!(back, f);
        let via_value = SettingsFile::from_json(&crate::textfmt::parse_json(&j).unwrap()).unwrap();
        assert_eq!(via_value, f);
    }

    #[test]
    fn a_newer_schema_is_refused_with_the_recovered_message() {
        let v = crate::textfmt::parse_json(r#"{"schema_version": 9, "instance": "x", "character": "y"}"#)
            .unwrap();
        let e = SettingsFile::from_json(&v).unwrap_err();
        assert_eq!(
            e,
            "settings.json is schema 9, and this build of Hydra reads up to 1"
        );
    }

    #[test]
    fn sections_round_trip() {
        let mut f = SettingsFile::new();
        f.set_section(
            "lich",
            Value::Obj(vec![("path".into(), Value::Str("/ruby".into()))]),
        );
        f.set_section("symbol", Value::Str(".".into()));
        assert_eq!(f.sections.len(), 2);
        let back: SettingsFile = serde_json::from_str(&f.to_json()).unwrap();
        assert_eq!(back.section("symbol").unwrap().as_str(), Some("."));
    }

    #[test]
    fn load_returns_the_default_for_a_missing_file() {
        let p = std::env::temp_dir().join("hydra-settings-does-not-exist.json");
        let _ = std::fs::remove_file(&p);
        let f = load(&p).unwrap();
        assert_eq!(f.schema_version, SCHEMA_VERSION);
        assert!(f.instance.is_empty());
    }

    #[test]
    fn a_hand_written_file_loads_through_serde_and_saves_back() {
        let dir = std::env::temp_dir().join(format!("hydra-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gemstone_aelith.settings.json");
        // Unknown members are ignored, missing ones fall back to the default —
        // the permissive shape the original's `serde(default)` fields had.
        std::fs::write(
            &path,
            r#"{"schema_version": 1, "instance": "gemstone", "character": "Aelith",
                "sections": {"symbol": "!", "extra": {"n": 2}}, "unknown": [1, 2]}"#,
        )
        .unwrap();
        let file = load(&path).unwrap();
        assert_eq!(file.instance, "gemstone");
        assert_eq!(file.section("symbol").unwrap().as_str(), Some("!"));
        assert_eq!(
            file.section("extra").unwrap().get("n").unwrap().as_u64(),
            Some(2)
        );
        save(&path, &file).unwrap();
        // Saving keeps the four recovered keys in the recovered order.
        let text = std::fs::read_to_string(&path).unwrap();
        let restored: SettingsFile = serde_json::from_str(&text).unwrap();
        assert_eq!(restored, file);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_schema_in_a_file_is_refused_on_load() {
        let dir = std::env::temp_dir().join(format!("hydra-settings-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.settings.json");
        std::fs::write(&path, r#"{"schema_version": 7}"#).unwrap();
        let e = load(&path).unwrap_err();
        assert!(
            e.ends_with("settings.json is schema 7, and this build of Hydra reads up to 1"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
