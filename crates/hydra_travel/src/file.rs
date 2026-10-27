//! `travel.json` — the per-install travel data: remembered places, saved
//! targets, the last room seen and the elemental instabilities met.
//!
//! Recovered from `hydra_travel::file::*` (`0x101505338`..`0x10150990c` plus
//! `file::read` `0x101508de8`, `key_in` `0x101509198`, `spot_in` `0x101509314`,
//! `spot_name` `0x101509670` and `file::Refused`'s `Display` at `0x101510ea0`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::json;
use crate::walk::instability::Seen;

/// The file name, from the literal at `0x10241bc0b` (length 11).
pub const NAME: &str = "travel.json";

/// The newest schema this build understands.
///
/// **ESTABLISHED** (`0x101508f68`): `file::read` loads the schema word out of
/// the parsed `Shared` (`+0x48`) and compares it with `3`
/// (`cmp w8, #0x3; b.ls ok`). Anything larger becomes
/// `Refused::Newer`, whose message is *"travel.json is schema {found}, and this
/// build of Hydra reads up to {up_to}"* (pieces at `0x103e6beb4`, `0x103e6becb`).
pub const SCHEMA: u32 = 3;

/// Why `travel.json` could not be used.
///
/// The three `Display` texts are recovered verbatim:
///
/// | variant | message | evidence |
/// |---|---|---|
/// | [`Refused::Read`] | `travel.json cannot be read: {0}` | pieces at `0x103e6bef5` |
/// | [`Refused::Newer`] | `travel.json is schema {0}, and this build of Hydra reads up to {1}` | pieces at `0x103e6beb4` / `0x103e6becb` |
/// | [`Refused::NoName`] | `the character has no name a file can hold` | `0x10241bce0` |
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The file could not be read or parsed; the payload is the `io::Error` or
    /// `serde_json::Error` `Display` text (`file::read` formats both into a
    /// `String` before returning).
    Read(String),
    /// The file was written by a newer Hydra; payload is the schema it holds.
    Newer(u32),
    /// The character's names sanitise to nothing, so no file key exists.
    NoName,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // @0x103e6bef5: `\x1c`-prefixed piece "travel.json cannot be read: "
            Refused::Read(why) => write!(f, "travel.json cannot be read: {why}"),
            // @0x103e6beb4 / @0x103e6becb
            Refused::Newer(found) => write!(
                f,
                "travel.json is schema {found}, and this build of Hydra reads up to {SCHEMA}"
            ),
            // @0x10241bce0
            Refused::NoName => f.write_str("the character has no name a file can hold"),
        }
    }
}

impl std::error::Error for Refused {}

/// `From<Refused> for io::Error` (`0x101505338`).
///
/// **ESTABLISHED**: the body inlines `Refused`'s `Display` into a `String` and
/// calls `std::io::error::Error::new` with kind `0x15` — `ErrorKind`'s
/// `InvalidData` discriminant on this toolchain.
impl From<Refused> for std::io::Error {
    fn from(refused: Refused) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidData, refused.to_string())
    }
}

/// One character's slice of `travel.json`.
///
/// **ESTABLISHED**: the field set comes from `Spot`'s serde `__FieldVisitor`
/// (`0x101509934`), which compares `"targets"` (length 7, field index `0x17`),
/// `"last_room"` (length 9, index `0x18`) and `"memories"` (length 8, index
/// `0x16`); the *types* come from `drop_in_place<Spot>` (`0x10150a8c4`), which
/// drops a `BTreeMap<String, serde_json::Value>` and a
/// `BTreeMap<String, Vec<u32>>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spot {
    /// Remembered places: whatever the walker learned about them. The original
    /// stored free-form `serde_json::Value`s, so this build stores
    /// [`crate::json::Value`]s.
    pub memories: BTreeMap<String, json::Value>,
    /// Saved targets, each a path of room ids.
    pub targets: BTreeMap<String, Vec<u32>>,
    /// The room the character was last seen in.
    pub last_room: u32,
}

/// The whole file.
///
/// **ESTABLISHED**: `Shared::serialize` (`0x101506304`) writes
/// `schema_version`, `characters` and `instabilities` (`0x10241bc46`,
/// `0x10241bc54`, `0x10241bc2c`), and `drop_in_place<Shared>` (`0x10150abc0`)
/// drops a `BTreeMap<String, Vec<Seen>>` for the instabilities.
#[derive(Clone, Debug, PartialEq)]
pub struct Shared {
    pub schema_version: u32,
    pub characters: BTreeMap<String, Spot>,
    pub instabilities: BTreeMap<String, Vec<Seen>>,
    /// **UNKNOWN:** the deserializer's field list says
    /// `struct Shared with 4 elements` (`0x10241dfac`) but only three names are
    /// served by the serializer. The `memories` literal at `0x10241dff8`
    /// directly after the count string is the only surviving candidate for the
    /// fourth, so it is kept here as free-form JSON and preserved on rewrite.
    /// Nothing in the recovered code reads it.
    pub memories: BTreeMap<String, json::Value>,
}

impl Default for Shared {
    fn default() -> Shared {
        Shared {
            schema_version: SCHEMA,
            characters: BTreeMap::new(),
            instabilities: BTreeMap::new(),
            memories: BTreeMap::new(),
        }
    }
}

impl Shared {
    /// Render back to the wire format `serde_json` produced.
    pub fn to_json(&self) -> json::Value {
        let mut characters = BTreeMap::new();
        for (name, spot) in &self.characters {
            let mut obj = BTreeMap::new();
            obj.insert("memories".to_string(), json::Value::Object(spot.memories.clone()));
            let mut targets = BTreeMap::new();
            for (target, rooms) in &spot.targets {
                targets.insert(
                    target.clone(),
                    json::Value::Array(
                        rooms
                            .iter()
                            .map(|r| json::Value::Number(*r as f64))
                            .collect(),
                    ),
                );
            }
            obj.insert("targets".to_string(), json::Value::Object(targets));
            obj.insert(
                "last_room".to_string(),
                json::Value::Number(spot.last_room as f64),
            );
            characters.insert(name.clone(), json::Value::Object(obj));
        }
        let mut instabilities = BTreeMap::new();
        for (name, seen) in &self.instabilities {
            instabilities.insert(
                name.clone(),
                json::Value::Array(seen.iter().map(Seen::to_json).collect()),
            );
        }
        let mut root = BTreeMap::new();
        root.insert(
            "schema_version".to_string(),
            json::Value::Number(self.schema_version as f64),
        );
        root.insert("characters".to_string(), json::Value::Object(characters));
        root.insert(
            "instabilities".to_string(),
            json::Value::Object(instabilities),
        );
        if !self.memories.is_empty() {
            root.insert(
                "memories".to_string(),
                json::Value::Object(self.memories.clone()),
            );
        }
        json::Value::Object(root)
    }

    /// Read the wire format back.
    pub fn from_json(value: &json::Value) -> Option<Shared> {
        let schema_version = value.get("schema_version")?.as_u32()?;
        let mut characters = BTreeMap::new();
        if let Some(map) = value.get("characters").and_then(|v| v.as_object()) {
            for (name, spot) in map {
                let mut out = Spot::default();
                if let Some(memories) = spot.get("memories").and_then(|v| v.as_object()) {
                    out.memories = memories.clone();
                }
                if let Some(targets) = spot.get("targets").and_then(|v| v.as_object()) {
                    for (target, rooms) in targets {
                        let rooms: Vec<u32> = rooms
                            .as_array()
                            .map(|a| a.iter().filter_map(|r| r.as_u32()).collect())
                            .unwrap_or_default();
                        out.targets.insert(target.clone(), rooms);
                    }
                }
                out.last_room = spot
                    .get("last_room")
                    .and_then(|v| v.as_u32())
                    .unwrap_or_default();
                characters.insert(name.clone(), out);
            }
        }
        let mut instabilities = BTreeMap::new();
        if let Some(map) = value.get("instabilities").and_then(|v| v.as_object()) {
            for (name, seen) in map {
                let seen: Vec<Seen> = seen
                    .as_array()
                    .map(|a| a.iter().filter_map(Seen::from_json).collect())
                    .unwrap_or_default();
                instabilities.insert(name.clone(), seen);
            }
        }
        let mut memories = BTreeMap::new();
        if let Some(map) = value.get("memories").and_then(|v| v.as_object()) {
            memories = map.clone();
        }
        Some(Shared {
            schema_version,
            characters,
            instabilities,
            memories,
        })
    }
}

/// `dir.join("travel.json")` — `travel_path` is 16 bytes of
/// `Path::_join` with the length immediate `0xb` (`0x1015055b8`).
pub fn travel_path(dir: &Path) -> PathBuf {
    dir.join(NAME)
}

/// The file key for one character: `format!("{}_{}", safe_component(a),
/// safe_component(b))` (`file::spot_name`, `0x101509670`).
///
/// **ESTABLISHED**: two `hydra_session::store::safe_component` calls, then one
/// `format_inner` with the two-byte piece list at `0x103e21bc4` — the same
/// `{}_{}` shape `hydra_session::store::character_path` uses for its file
/// stems. A name that sanitises to nothing still produces a key here (only the
/// *file* path can fail), which is why `Refused::NoName` exists.
pub fn spot_name(first: &str, second: &str) -> String {
    format!(
        "{}_{}",
        hydra_session::store::safe_component(first),
        hydra_session::store::safe_component(second)
    )
}

/// The `Spot` for one character, created on demand (`file::spot_in`,
/// `0x101509314`: `BTreeMap::entry(..).or_default()` on the key from
/// [`spot_name`]).
pub fn spot_in<'a>(shared: &'a mut Shared, first: &str, second: &str) -> &'a mut Spot {
    shared
        .characters
        .entry(spot_name(first, second))
        .or_default()
}

/// The value under one key of a `Spot`, created on demand (`file::key_in`,
/// `0x101509198`). The recovered body is `entry(key).or_default()`-shaped: it
/// clones the key only when the entry is vacant and inserts through
/// `VacantEntry::insert_entry`.
pub fn key_in<'a>(spot: &'a mut Spot, key: &str) -> &'a mut json::Value {
    spot.memories.entry(key.to_string()).or_default()
}

/// Read and parse `travel.json`.
///
/// **ESTABLISHED** (`0x101508de8`):
/// `travel_path` → `fs::read_to_string` → `serde_json::from_trait::<Shared>`.
/// Both failures are turned into a `String` (the `io::Error` or
/// `serde_json::Error` `Display` text) and returned as [`Refused::Read`]; a
/// schema word above [`SCHEMA`] becomes [`Refused::Newer`].
pub fn read(dir: &Path) -> Result<Shared, Refused> {
    let path = travel_path(dir);
    let text = std::fs::read_to_string(&path).map_err(|e| Refused::Read(e.to_string()))?;
    let value = json::parse(&text).map_err(|e| Refused::Read(e.to_string()))?;
    let shared = Shared::from_json(&value)
        .ok_or_else(|| Refused::Read("missing schema_version".to_string()))?;
    if shared.schema_version > SCHEMA {
        return Err(Refused::Newer(shared.schema_version));
    }
    Ok(shared)
}

/// Write `travel.json` for `dir`.
pub fn write(dir: &Path, shared: &Shared) -> Result<(), Refused> {
    let path = travel_path(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Refused::Read(e.to_string()))?;
    }
    std::fs::write(&path, shared.to_json().write() + "\n")
        .map_err(|e| Refused::Read(e.to_string()))
}

/// Read the file, let `edit` change it, write it back.
///
/// **`std`-only departure:** the original called
/// `hydra_session::store::changing(path, closure)` (the three monomorphisations
/// `0x10152b9e8`, `0x10152c4d4`, `0x10152c8d8`, `0x10152d230`) which reads the
/// JSON, hands the closure the parsed value and rewrites the file when the
/// closure reports a change. That function is not part of the reconstructed
/// `hydra_session`, so the same read/edit/write cycle lives here.
fn changing<T>(dir: &Path, edit: impl FnOnce(&mut Shared) -> T) -> Result<T, Refused> {
    let mut shared = match read(dir) {
        Ok(shared) => shared,
        // A missing or empty file starts a fresh image; anything else is fatal.
        Err(Refused::Read(_)) if !travel_path(dir).exists() => Shared::default(),
        Err(other) => return Err(other),
    };
    shared.schema_version = SCHEMA;
    let out = edit(&mut shared);
    write(dir, &shared)?;
    Ok(out)
}

/// Load the file, or an empty image when there is nothing usable on disk.
///
/// **Partly recovered** (`0x10150858c`): the body calls [`spot_name`] then
/// [`read`], then clones the three maps and inserts the loaded entries into
/// them one at a time (three `BTreeMap` clone monomorphisations, each followed
/// by an `insert` loop over `IntoIter::dying_next`). It therefore *merges* the
/// loaded image into a fresh one rather than handing the file image back; the
/// per-key merge policy across that loop is not recoverable from the
/// instruction stream, so this build returns the loaded image and a default
/// image when the file is unusable.
pub fn load(dir: &Path) -> Shared {
    match read(dir) {
        Ok(mut shared) => {
            shared.schema_version = SCHEMA;
            shared
        }
        Err(_) => Shared::default(),
    }
}

/// Store a named target (a path of room ids) for one character
/// (`0x1015054f8`). The value the closure builds is the `Spot`'s `targets`
/// entry.
pub fn save_target(
    dir: &Path,
    first: &str,
    second: &str,
    target: &str,
    rooms: &[u32],
) -> Result<(), Refused> {
    changing(dir, |shared| {
        spot_in(shared, first, second)
            .targets
            .insert(target.to_string(), rooms.to_vec());
    })
}

/// Forget a saved target; `true` when something was removed
/// (`0x1015055c8` — the `Result<bool, _>` return is visible as the byte written
/// at `+1` of the sret next to the `Ok` tag at `+0`).
pub fn forget_target(
    dir: &Path,
    first: &str,
    second: &str,
    target: &str,
) -> Result<bool, Refused> {
    changing(dir, |shared| {
        spot_in(shared, first, second)
            .targets
            .remove(target)
            .is_some()
    })
}

/// Store one character's remembered places (`0x101505a18`).
pub fn save_memories(
    dir: &Path,
    first: &str,
    second: &str,
    memories: &BTreeMap<String, json::Value>,
) -> Result<(), Refused> {
    changing(dir, |shared| {
        spot_in(shared, first, second).memories = memories.clone();
    })
}

/// Store the room the character was last seen in (`0x101506174`; the room id
/// arrives in `w6` and is written straight into the `Spot`).
pub fn save_last_room(
    dir: &Path,
    first: &str,
    second: &str,
    room: u32,
) -> Result<(), Refused> {
    changing(dir, |shared| {
        spot_in(shared, first, second).last_room = room;
    })
}

/// Store the elemental instabilities one character has seen
/// (`0x101506240`).
pub fn save_instabilities(
    dir: &Path,
    first: &str,
    second: &str,
    seen: &[Seen],
) -> Result<(), Refused> {
    changing(dir, |shared| {
        shared
            .instabilities
            .insert(spot_name(first, second), seen.to_vec());
    })
}

/// The elemental instabilities recorded for one character
/// (`0x1015056ac`).
pub fn instabilities(dir: &Path, first: &str, second: &str) -> Vec<Seen> {
    let shared = load(dir);
    shared
        .instabilities
        .get(&spot_name(first, second))
        .cloned()
        .unwrap_or_default()
}

/// Turn one character's stored spot into travel settings
/// (`0x101505ae0`).
///
/// **Partly recovered**: the body reads the `Spot` out of the shared image,
/// looks each known setting key up in its `memories` map and keeps the ones
/// that are present (`settings::TravelSettings`'s `BTreeMap<String, String>`
/// shape — see [`crate::settings`]). A spot that is not in the file yields
/// default settings.
pub fn take_settings(dir: &Path, first: &str, second: &str) -> crate::settings::TravelSettings {
    let shared = load(dir);
    match shared.characters.get(&spot_name(first, second)) {
        Some(spot) => crate::settings::TravelSettings::from_memories(&spot.memories),
        None => crate::settings::TravelSettings::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "hydra_travel_file_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn file_name_and_path() {
        assert_eq!(NAME, "travel.json");
        assert_eq!(travel_path(Path::new("/tmp/x")), Path::new("/tmp/x/travel.json"));
        assert_eq!(SCHEMA, 3);
    }

    #[test]
    fn spot_names_sanitise_like_character_paths() {
        // `safe_component` drops everything but ASCII alphanumerics and folds
        // case; the join is `_`.
        assert_eq!(spot_name("Aelith", "GemStone IV"), "aelith_gemstoneiv");
        assert_eq!(spot_name("", ""), "_");
    }

    #[test]
    fn refused_messages_are_the_recovered_literals() {
        assert_eq!(
            Refused::Read("bad".to_string()).to_string(),
            "travel.json cannot be read: bad"
        );
        assert_eq!(
            Refused::Newer(9).to_string(),
            "travel.json is schema 9, and this build of Hydra reads up to 3"
        );
        assert_eq!(
            Refused::NoName.to_string(),
            "the character has no name a file can hold"
        );
        let io: std::io::Error = Refused::Newer(9).into();
        assert_eq!(io.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn round_trips_targets_and_instabilities() {
        let dir = tempdir("round");
        save_target(&dir, "Aelith", "GS4", "den", &[284, 300]).unwrap();
        save_last_room(&dir, "Aelith", "GS4", 10862).unwrap();
        save_instabilities(
            &dir,
            "Aelith",
            "GS4",
            &[Seen {
                room: 1234,
                element: "fire".to_string(),
                seen: 3,
            }],
        )
        .unwrap();
        let shared = read(&dir).unwrap();
        let spot = shared.characters.get("aelith_gs4").unwrap();
        assert_eq!(spot.targets.get("den").unwrap(), &vec![284, 300]);
        assert_eq!(spot.last_room, 10862);
        let seen = &shared.instabilities["aelith_gs4"];
        assert_eq!(seen[0].element, "fire");
        assert_eq!(instabilities(&dir, "Aelith", "GS4"), seen.clone());
        assert!(forget_target(&dir, "Aelith", "GS4", "den").unwrap());
        assert!(!forget_target(&dir, "Aelith", "GS4", "den").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempdir("newer");
        std::fs::write(
            travel_path(&dir),
            r#"{"schema_version":9,"characters":{},"instabilities":{}}"#,
        )
        .unwrap();
        assert_eq!(read(&dir), Err(Refused::Newer(9)));
        assert_eq!(load(&dir), Shared::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
