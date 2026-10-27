//! The map overrides file: hand corrections to the map's room titles and areas.
//!
//! Recovered from `overrides.rs`'s eight functions. The file is
//! `map-overrides.json` in the data directory (`0x10245c017`, 18 bytes) and the
//! export is `map-overrides-export.json` (`0x10245c029`, 25 bytes). The document
//! is one object with one key, `"areas"` (`0x10245c012` — the only
//! `serialize_entry` in `Overrides::save`):
//!
//! ```text
//! {"areas": [ … ]}
//! ```
//!
//! `Exported`'s own `Serialize` (`0x1015bda88`) writes four keys in this order —
//! `id` (`0x10245c003`), `uid` (`0x10245c005`), `title` (`0x10245c009`), `area`
//! (`0x10245c00e`).
//!
//! `overrides::written` (`0x1015be258`) is the durable write: `process::id`,
//! `Path::with_extension` for a `*.{pid}.tmp`, `OpenOptions`, `write_all`,
//! `File::sync_all`, `fs::rename`, then `fs::remove_file` on the failure path.
//!
//! **Phase 1.** The three documents are real `serde` derives now (`serde`
//! 1.0.229 with `derive`) read and written by `serde_json` 1.0.151, both
//! `x = { workspace = true }`. Declaration order *is* the recovered
//! `serialize_entry` order, so the files keep their byte-for-byte shape.

use std::fs;
use std::io::Result;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cena::{Map, Room, RoomId};

/// The overrides file's name (`0x10245c017`).
pub const NAME: &str = "map-overrides.json";

/// The export file's name (`0x10245c029`).
pub const EXPORT: &str = "map-overrides-export.json";

/// One room, as the overrides file and the export both spell it.
///
/// The four keys and their order are `Exported::serialize`'s; the types come
/// from the four `serialize_entry` monomorphizations it calls (a `u32`, a `u64`
/// and two strings).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Exported {
    /// The map's room id (`u32`).
    pub id: RoomId,
    /// The room's uid (`u64`).
    pub uid: u64,
    pub title: String,
    pub area: String,
}

impl Exported {
    /// The four recovered keys, in the recovered order.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("an override always serialises")
    }

    /// The `Deserialize` side of one element.
    pub fn from_value(v: &serde_json::Value) -> serde_json::Result<Exported> {
        serde_json::from_value(v.clone())
    }
}

/// One area's worth of corrections: the area's name and the rooms in it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Area {
    pub area: String,
    pub rooms: Vec<Exported>,
}

/// The whole file.
///
/// **Partly recovered.** The shape is certain — the struct is 24 bytes, i.e. a
/// single `Vec`, and `save` writes exactly one key, `"areas"` — but what an
/// element of that `Vec` looks like had to be inferred. `applied` rebuilds a
/// `cena_map::Map` from the result (`Map::from_rooms`, `0x1015bf4d8`) and
/// `filed_in` (`0x1015be424`) rewrites one `cena_map::Room`'s `Vec<String>`
/// fields, which is why an area groups its rooms here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Overrides {
    pub areas: Vec<Area>,
}

impl Overrides {
    /// @0x1015bea40 — read `dir/map-overrides.json`.
    ///
    /// `Path::join`, then `fs::read_to_string`, then `serde_json::from_str`. Any
    /// failure becomes the empty document: the sret is written with two zero
    /// words and a zero length at `0x1015beb24`.
    pub fn load(dir: &Path) -> Overrides {
        match fs::read_to_string(dir.join(NAME)) {
            Ok(text) => Overrides::from_json(&text).unwrap_or_default(),
            Err(_) => Overrides::default(),
        }
    }

    /// The `Deserialize` side of the document.
    pub fn from_json(text: &str) -> serde_json::Result<Overrides> {
        serde_json::from_str(text)
    }

    /// The `Serialize` side: exactly what `Overrides::save` writes.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("an overrides document always serialises")
    }

    /// @0x1015bebc4 — write `dir/map-overrides.json`.
    pub fn save(&self, dir: &Path) -> Result<()> {
        written(&dir.join(NAME), self.to_json().as_bytes())
    }

    /// @0x1015bf05c — write `dir/map-overrides-export.json`: every room the
    /// overrides mention, flattened into one sequence (`Serializer::collect_seq`
    /// at `0x1015bf130`).
    pub fn export(&self, dir: &Path) -> Result<()> {
        let rooms: Vec<&Exported> = self.areas.iter().flat_map(|a| a.rooms.iter()).collect();
        let text = serde_json::to_string(&rooms).expect("an override always serialises");
        written(&dir.join(EXPORT), text.as_bytes())
    }

    /// @0x1015bf4d8 — the map with these corrections applied.
    ///
    /// The body clones the map twice and calls `Map::from_rooms`, so the answer
    /// is a fresh map; rooms are matched by `id` (and by `uid` when the id is
    /// zero), and `title`/`area` are replaced where an override exists.
    pub fn applied(&self, map: &Map) -> Map {
        let mut rooms: Vec<Room> = map.rooms().to_vec();
        for area in &self.areas {
            for over in &area.rooms {
                let Some(room) = rooms
                    .iter_mut()
                    .find(|r| r.id == over.id || (over.id == 0 && u64::from(r.id) == over.uid))
                else {
                    continue;
                };
                filed_in(room, over);
            }
        }
        Map::from_rooms(rooms)
    }
}

/// @0x1015be424 — apply one override to one room: the title and the area, with
/// the room's own area list rewritten so the new name is the innermost one.
///
/// The body clones the room's `Vec<String>` fields, `Vec::retain`s the old name
/// out of them and formats the new one in, which is what this does.
pub fn filed_in(room: &mut Room, over: &Exported) {
    if !over.title.is_empty() {
        room.title = over.title.clone();
    }
    if !over.area.is_empty() {
        room.areas.retain(|a| a != &over.area);
        room.areas.push(over.area.clone());
    }
}

/// @0x1015bdba0 — the map's own rooms, grouped into an overrides document.
///
/// **Partly recovered.** `pocket` calls `cena_map::Map::room` in a loop, folds
/// the rooms into a `BTreeMap` (so the areas come out ordered) and extends a
/// `VecDeque` of them; that is the grouping implemented here. Its arguments
/// could not be read out of the 3 744-byte body, so this one takes the map.
pub fn pocket(map: &Map) -> Overrides {
    let mut by_area: std::collections::BTreeMap<String, Vec<Exported>> = Default::default();
    for room in map.rooms() {
        let area = room.area().unwrap_or("").to_string();
        by_area.entry(area.clone()).or_default().push(Exported {
            id: room.id,
            uid: u64::from(room.id),
            title: room.title.clone(),
            area,
        });
    }
    Overrides {
        areas: by_area
            .into_iter()
            .map(|(area, rooms)| Area { area, rooms })
            .collect(),
    }
}

/// `overrides::written` — write `bytes` to `path` durably: a `*.{pid}.tmp`
/// sibling, `sync_all`, then `rename` (and `remove_file` if the rename fails).
///
/// The temporary name comes from `Path::with_extension` (`0x1015be2b0`) fed with
/// `format!("{}.{}.tmp", extension, pid)`, which is what makes a half-written
/// overrides file impossible.
pub fn written(path: &Path, bytes: &[u8]) -> Result<()> {
    let pid = std::process::id();
    let extension = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{ext}.{pid}.tmp"),
        None => format!("{pid}.tmp"),
    };
    let temp: PathBuf = path.with_extension(extension);
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if let Err(e) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hydra-map-overrides-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn document() -> Overrides {
        Overrides {
            areas: vec![Area {
                area: "North Blue".into(),
                rooms: vec![Exported {
                    id: 7116001,
                    uid: 7116001,
                    title: "The Tenebrous Cauldron".into(),
                    area: "North Blue".into(),
                }],
            }],
        }
    }

    #[test]
    fn the_document_uses_the_recovered_keys() {
        assert_eq!(
            document().to_json(),
            concat!(
                r#"{"areas":[{"area":"North Blue","rooms":["#,
                r#"{"id":7116001,"uid":7116001,"title":"The Tenebrous Cauldron","area":"North Blue"}"#,
                r#"]}]}"#
            )
        );
        assert_eq!(
            Overrides::from_json(&document().to_json()).unwrap(),
            document()
        );
        assert_eq!(Overrides::from_json("{}").unwrap(), Overrides::default());
        // serde's derive also accepts the *sequence* spelling of a struct, so
        // `[]` is now `Overrides { areas: [] }` where the hand-written reader
        // refused it. A wrong type inside still reports serde_json's wording.
        assert_eq!(Overrides::from_json("[]").unwrap(), Overrides::default());
        let err = Overrides::from_json(r#"{"areas":{}}"#).unwrap_err();
        assert!(
            err.to_string().contains("invalid type: map, expected a sequence"),
            "{err}"
        );
    }

    #[test]
    fn save_load_and_export_round_trip() {
        let dir = scratch("roundtrip");
        let doc = document();
        doc.save(&dir).unwrap();
        assert_eq!(fs::read_to_string(dir.join(NAME)).unwrap(), doc.to_json());
        assert_eq!(Overrides::load(&dir), doc);

        doc.export(&dir).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join(EXPORT)).unwrap(),
            concat!(
                r#"[{"id":7116001,"uid":7116001,"#,
                r#""title":"The Tenebrous Cauldron","area":"North Blue"}]"#
            )
        );
        // A directory with no overrides file loads as the empty document.
        let empty = scratch("empty");
        assert_eq!(Overrides::load(&empty), Overrides::default());
        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&empty).ok();
    }

    #[test]
    fn applied_rewrites_the_title_and_the_area() {
        let map = Map::from_rooms(vec![Room {
            id: 7116001,
            title: "Old Name".into(),
            areas: vec!["[North Blue]".into()],
            ..Default::default()
        }]);
        let fixed = document().applied(&map);
        assert_eq!(fixed.room(7116001).unwrap().title, "The Tenebrous Cauldron");
        assert_eq!(fixed.room(7116001).unwrap().area(), Some("North Blue"));
        // An override for a room the map does not have is ignored.
        let other = Overrides {
            areas: vec![Area {
                area: "x".into(),
                rooms: vec![Exported {
                    id: 5,
                    ..Default::default()
                }],
            }],
        };
        assert_eq!(other.applied(&map).len(), 1);
    }

    #[test]
    fn pocket_groups_the_map_by_area() {
        let map = Map::from_rooms(vec![
            Room {
                id: 2,
                title: "B".into(),
                areas: vec!["Beta".into()],
                ..Default::default()
            },
            Room {
                id: 1,
                title: "A".into(),
                areas: vec!["Alpha".into()],
                ..Default::default()
            },
        ]);
        let doc = pocket(&map);
        let areas: Vec<&str> = doc.areas.iter().map(|a| a.area.as_str()).collect();
        assert_eq!(areas, vec!["Alpha", "Beta"]);
        assert_eq!(doc.areas[0].rooms[0].title, "A");
        assert_eq!(doc.areas[0].rooms[0].uid, 1);
    }

    #[test]
    fn written_replaces_the_file_and_leaves_no_temp_behind() {
        let dir = scratch("written");
        let path = dir.join("thing.json");
        written(&path, b"one").unwrap();
        written(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        let mut left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(left, vec!["thing.json".to_string()]);
        fs::remove_dir_all(&dir).ok();
    }

    // ---- Phase 1: what the real derives now provide -------------------------

    #[test]
    fn the_derive_round_trips_through_values_and_tolerates_absent_or_extra_keys() {
        let doc = document();
        // Struct → value → struct, the path `serde_json` itself takes.
        let value = serde_json::to_value(&doc).unwrap();
        assert_eq!(serde_json::from_value::<Overrides>(value).unwrap(), doc);
        // Known keys, a misspelt one and an absent one: the absent keys default
        // (`#[serde(default)]`) and the unknown one is ignored, which is what
        // the previous hand-written reader did.
        let doc = Overrides::from_json(
            r#"{"areas":[{"rooms":[{"id":1,"uid":2,"title":"t","area":"a","note":9}],"extra":true}],"uid":7}"#,
        )
        .unwrap();
        assert_eq!(doc.areas.len(), 1);
        assert_eq!(doc.areas[0].area, "");
        assert_eq!(doc.areas[0].rooms[0].id, 1);
        assert_eq!(doc.areas[0].rooms[0].title, "t");
        // A single override element stands alone too.
        let one = Exported::from_value(
            &serde_json::from_str(r#"{"id":3,"uid":4,"title":"T","area":"A"}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(one.to_json(), r#"{"id":3,"uid":4,"title":"T","area":"A"}"#);
    }

    #[test]
    fn a_broken_document_loads_as_the_empty_one() {
        // `load` swallows every error into the empty document, exactly as the
        // recovered sret write at `0x1015beb24` does — including serde's
        // wording, which never reaches the caller.
        let dir = scratch("broken");
        fs::write(dir.join(NAME), "{not json").unwrap();
        assert_eq!(Overrides::load(&dir), Overrides::default());
        fs::write(dir.join(NAME), r#"{"areas":{}}"#).unwrap();
        assert_eq!(Overrides::load(&dir), Overrides::default());
        // But `from_json` reports what serde_json saw, with its line/column.
        let err = Overrides::from_json("{not json").unwrap_err();
        assert!(err.to_string().contains("line 1 column 2"), "{err}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn strings_are_escaped_by_serde_json() {
        let doc = Overrides {
            areas: vec![Area {
                area: "a\"b\\c\nd\u{1}é".into(),
                rooms: vec![Exported {
                    id: 1,
                    uid: 2,
                    title: "t".into(),
                    area: String::new(),
                }],
            }],
        };
        let text = doc.to_json();
        assert_eq!(
            text,
            concat!(
                r#"{"areas":[{"area":"a\"b\\c\nd\u0001é","rooms":["#,
                r#"{"id":1,"uid":2,"title":"t","area":""}]}]}"#
            )
        );
        assert_eq!(Overrides::from_json(&text).unwrap(), doc);
    }
}
