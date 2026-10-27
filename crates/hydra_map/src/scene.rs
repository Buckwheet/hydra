//! The scene file: what one place looks like when it is laid out on a map.
//!
//! Recovered from the binary wholesale: five `serde` `Serialize` impls
//! (`MapScene` `0x1015ae688`, `SceneDoor` `0x1015aea00`, `SceneEdge`
//! `0x1015aeb54`, `SceneRoom` `0x1015aeca8`, `SceneLabel` `0x1015aedfc`), the
//! `Deserialize` field visitor for `MapScene` (`0x1015ac318`) and `EdgeKind`'s
//! deserializer (`0x1015ae814`). The field *names* are the literals the
//! serializers pass to `SerializeMap::serialize_entry`; the field *types* are
//! the monomorphizations they call, each of which was read to see whether it
//! serialises a `u32`, a `u64`, a `bool`, a string, an `(f32, f32)` tuple, an
//! `(i32, i32)` tuple, a sequence of rooms/edges/doors/labels or a sequence of
//! strings.
//!
//! ```text
//! { "area": String, "rooms": [SceneRoom], "edges": [SceneEdge],
//!   "doors": [SceneDoor], "buildings": [String], "labels": [SceneLabel],
//!   "min": [i32, i32], "max": [i32, i32] }
//!
//! SceneDoor  { street: u32, inside: u32, at: [f32, f32], place: String,
//!              named: bool, marks: [String] }
//! SceneEdge  { a: u32, b: u32, kind: EdgeKind, path: [[f32, f32]],
//!              label: Option<String>, building: Option<u64> }
//! SceneRoom  { id: u32, cell: [i32, i32], title: String,
//!              building: Option<u64>, paths: String }
//! SceneLabel { text: String, at: [f32, f32], building: Option<u64> }
//! EdgeKind   = "Directional" | "Connector" | "Stub"
//! ```
//!
//! The `MapScene::room` index is a `OnceLock<ById>` (offset `+0xa0`, state word
//! `+0xd0`) whose `ById` is a `HashMap<u32, usize>`; it is `#[serde(skip)]`ed,
//! which is why `MapScene`'s field visitor knows exactly eight names.
//!
//! **Phase 1.** These are now real `serde` derives (`serde` 1.0.229 with
//! `derive`, `serde_json` 1.0.151), both `x = { workspace = true }`. Deriving
//! keeps the recovered order for free: a derived `Serialize` writes its fields
//! in declaration order, which is exactly the order the five recovered
//! `serialize_entry` chains use, so the files come out byte for byte as before.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// What kind of line an edge is drawn as.
///
/// Externally tagged unit variants — `"Directional"`, `"Connector"`, `"Stub"` —
/// which is what `EdgeKind::deserialize` (`0x1015ae814`) reads back from the
/// three name literals (`0x102422e63`, `0x102422e6e`, `0x102422e77`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    #[default]
    Directional,
    Connector,
    Stub,
}

impl EdgeKind {
    /// The three variant names, `EdgeKind`'s own `serialize` literals
    /// (`0x102422e63`, `0x102422e6e`, `0x102422e77`).
    pub fn name(self) -> &'static str {
        match self {
            EdgeKind::Directional => "Directional",
            EdgeKind::Connector => "Connector",
            EdgeKind::Stub => "Stub",
        }
    }

    pub fn named(s: &str) -> Option<EdgeKind> {
        match s {
            "Directional" => Some(EdgeKind::Directional),
            "Connector" => Some(EdgeKind::Connector),
            "Stub" => Some(EdgeKind::Stub),
            _ => None,
        }
    }
}

/// A door on the scene: an opening between `street` and `inside`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneDoor {
    pub street: u32,
    pub inside: u32,
    /// Where the door sits on the sheet.
    pub at: (f32, f32),
    pub place: String,
    pub named: bool,
    pub marks: Vec<String>,
}

/// One drawn line between room `a` and room `b`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneEdge {
    pub a: u32,
    pub b: u32,
    pub kind: EdgeKind,
    /// The polyline the edge follows.
    pub path: Vec<(f32, f32)>,
    pub label: Option<String>,
    pub building: Option<u64>,
}

/// One room on the sheet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneRoom {
    pub id: u32,
    /// Column and row on the sheet's grid.
    pub cell: (i32, i32),
    pub title: String,
    pub building: Option<u64>,
    /// A packed list of neighbouring cells. The original stores this as a
    /// `String` too: `0x1015a1c14` is the same monomorphization `area`,
    /// `place`, `title` and `text` use.
    pub paths: String,
}

/// A free-standing label on the sheet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneLabel {
    pub text: String,
    pub at: (f32, f32),
    pub building: Option<u64>,
}

/// The index `MapScene::room` builds lazily (`hydra_map::scene::ById`).
///
/// Derived, never written: `MapScene`'s field visitor knows only the eight
/// document names.
#[derive(Clone, Debug, Default)]
pub struct ById(pub HashMap<u32, usize>);

impl ById {
    pub fn of(rooms: &[SceneRoom]) -> ById {
        let mut map = HashMap::with_capacity(rooms.len());
        for (i, r) in rooms.iter().enumerate() {
            map.entry(r.id).or_insert(i);
        }
        ById(map)
    }
}

/// A laid-out place.
///
/// **`#[serde(default)]`** (Phase 1, INFERRED): the recovered field visitor has
/// eight tags plus an `ignore` arm and says nothing about absent keys; the
/// previous hand-written reader defaulted them, and so does this.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MapScene {
    pub area: String,
    pub rooms: Vec<SceneRoom>,
    pub edges: Vec<SceneEdge>,
    pub doors: Vec<SceneDoor>,
    pub buildings: Vec<String>,
    pub labels: Vec<SceneLabel>,
    pub min: (i32, i32),
    pub max: (i32, i32),
    /// `#[serde(skip)]`: the id → room index, built on first use.
    #[serde(skip)]
    pub(crate) by_id: OnceLock<ById>,
}

impl PartialEq for MapScene {
    /// The index is derived, so it takes no part in equality.
    fn eq(&self, other: &Self) -> bool {
        self.area == other.area
            && self.rooms == other.rooms
            && self.edges == other.edges
            && self.doors == other.doors
            && self.buildings == other.buildings
            && self.labels == other.labels
            && self.min == other.min
            && self.max == other.max
    }
}

impl MapScene {
    /// @0x1015aef34 — lazily index the rooms, look the id up, then return
    /// `&self.rooms[index]`. The index is bounds-checked on every hit
    /// (`cmp x8, [x19,#0x28]` / `b.hs`).
    pub fn room(&self, id: u32) -> Option<&SceneRoom> {
        let by_id = self.by_id.get_or_init(|| ById::of(&self.rooms));
        let ix = *by_id.0.get(&id)?;
        self.rooms.get(ix)
    }

    /// The `Serialize` impl, entry for entry, in the recovered order —
    /// `serde_json::to_string` of the derived impl (no spaces, no trailing
    /// newline, which is what the original's writer produced too).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a scene always serialises")
    }

    /// The `Deserialize` side, from the document text.
    pub fn from_json(text: &str) -> serde_json::Result<MapScene> {
        serde_json::from_str(text)
    }

    /// The `Deserialize` side, from an already-parsed document.
    pub fn from_value(v: &serde_json::Value) -> serde_json::Result<MapScene> {
        serde_json::from_value(v.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "area": "North Blue",
        "rooms": [
            {"id": 7116001, "cell": [10, 53], "title": "The Tenebrous Cauldron",
             "building": null, "paths": "n=7116224"},
            {"id": 7116002, "cell": [11, 53], "title": "The Tenebrous Cauldron",
             "building": 42, "paths": ""}
        ],
        "edges": [
            {"a": 7116001, "b": 7116002, "kind": "Directional",
             "path": [[10.0, 53.0], [11.0, 53.0]], "label": "go door",
             "building": null},
            {"a": 7116002, "b": 7116001, "kind": "Stub", "path": [],
             "label": null, "building": 7}
        ],
        "doors": [
            {"street": 7116001, "inside": 7116002, "at": [10.5, 53.0],
             "place": "The Tenebrous Cauldron", "named": true,
             "marks": ["a", "b"]}
        ],
        "buildings": ["The Tenebrous Cauldron"],
        "labels": [{"text": "N", "at": [9.0, 52.0], "building": null}],
        "min": [10, 53],
        "max": [11, 53]
    }"#;

    #[test]
    fn field_names_and_types_round_trip() {
        let scene = MapScene::from_json(SAMPLE).unwrap();
        assert_eq!(scene.area, "North Blue");
        assert_eq!(scene.rooms.len(), 2);
        assert_eq!(scene.rooms[0].id, 7116001);
        assert_eq!(scene.rooms[0].cell, (10, 53));
        assert_eq!(scene.rooms[1].building, Some(42));
        assert_eq!(scene.edges[0].kind, EdgeKind::Directional);
        assert_eq!(scene.edges[0].path[1], (11.0, 53.0));
        assert_eq!(scene.edges[0].label.as_deref(), Some("go door"));
        assert_eq!(scene.edges[1].kind, EdgeKind::Stub);
        assert_eq!(scene.edges[1].building, Some(7));
        assert_eq!(scene.doors[0].street, 7116001);
        assert!(scene.doors[0].named);
        assert_eq!(scene.doors[0].marks, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(scene.buildings, vec!["The Tenebrous Cauldron".to_string()]);
        assert_eq!(scene.labels[0].at, (9.0, 52.0));
        assert_eq!(scene.min, (10, 53));
        assert_eq!(scene.max, (11, 53));

        let text = scene.to_json();
        assert_eq!(
            text,
            concat!(
                r#"{"area":"North Blue","rooms":["#,
                r#"{"id":7116001,"cell":[10,53],"title":"The Tenebrous Cauldron","building":null,"paths":"n=7116224"},"#,
                r#"{"id":7116002,"cell":[11,53],"title":"The Tenebrous Cauldron","building":42,"paths":""}],"#,
                r#""edges":[{"a":7116001,"b":7116002,"kind":"Directional","path":[[10.0,53.0],[11.0,53.0]],"label":"go door","building":null},"#,
                r#"{"a":7116002,"b":7116001,"kind":"Stub","path":[],"label":null,"building":7}],"#,
                r#""doors":[{"street":7116001,"inside":7116002,"at":[10.5,53.0],"place":"The Tenebrous Cauldron","named":true,"marks":["a","b"]}],"#,
                r#""buildings":["The Tenebrous Cauldron"],"#,
                r#""labels":[{"text":"N","at":[9.0,52.0],"building":null}],"#,
                r#""min":[10,53],"max":[11,53]}"#
            )
        );
        assert_eq!(MapScene::from_json(&text).unwrap(), scene);
    }

    #[test]
    fn unknown_fields_are_ignored_like_serde() {
        let scene = MapScene::from_json(r#"{"area":"x","uid":99}"#).unwrap();
        assert_eq!(scene.area, "x");
        assert!(scene.rooms.is_empty());
    }

    #[test]
    fn room_index_is_built_once_and_bounds_checked() {
        let scene = MapScene::from_json(SAMPLE).unwrap();
        assert_eq!(scene.room(7116002).unwrap().cell, (11, 53));
        assert_eq!(scene.room(7116002).unwrap().building, Some(42));
        assert!(scene.room(1).is_none());
    }

    #[test]
    fn edge_kind_names_are_the_recovered_variants() {
        assert_eq!(EdgeKind::Directional.name(), "Directional");
        assert_eq!(EdgeKind::Connector.name(), "Connector");
        assert_eq!(EdgeKind::Stub.name(), "Stub");
        assert_eq!(EdgeKind::named("Stub"), Some(EdgeKind::Stub));
        assert_eq!(EdgeKind::named("stub"), None);
    }

    // ---- Phase 1: what the real `serde` derive now provides ----------------

    #[test]
    fn serde_derive_round_trips_through_values_and_text() {
        let scene = MapScene::from_json(SAMPLE).unwrap();
        // Text → value → struct, the path `serde_json` itself takes.
        let value = serde_json::to_value(&scene).unwrap();
        assert_eq!(MapScene::from_value(&value).unwrap(), scene);
        // Struct → text → struct.
        let text = serde_json::to_string(&scene).unwrap();
        assert_eq!(text, scene.to_json());
        assert_eq!(serde_json::from_str::<MapScene>(&text).unwrap(), scene);
        // An empty scene is still the eight recovered keys.
        assert_eq!(
            MapScene::default().to_json(),
            r#"{"area":"","rooms":[],"edges":[],"doors":[],"buildings":[],"labels":[],"min":[0,0],"max":[0,0]}"#
        );
    }

    #[test]
    fn every_child_document_round_trips_on_its_own() {
        let door = SceneDoor {
            street: 1,
            inside: 2,
            at: (1.5, -2.25),
            place: "a\tb".into(),
            named: false,
            marks: vec!["m".into()],
        };
        assert_eq!(
            serde_json::to_string(&door).unwrap(),
            r#"{"street":1,"inside":2,"at":[1.5,-2.25],"place":"a\tb","named":false,"marks":["m"]}"#
        );
        assert_eq!(
            serde_json::from_str::<SceneDoor>(&serde_json::to_string(&door).unwrap()).unwrap(),
            door
        );

        let edge = SceneEdge {
            a: 3,
            b: 4,
            kind: EdgeKind::Connector,
            path: vec![(0.0, 0.0), (1.25, 2.0)],
            label: Some("gate".into()),
            building: Some(u64::MAX),
        };
        assert_eq!(
            serde_json::to_string(&edge).unwrap(),
            r#"{"a":3,"b":4,"kind":"Connector","path":[[0.0,0.0],[1.25,2.0]],"label":"gate","building":18446744073709551615}"#
        );
        assert_eq!(
            serde_json::from_str::<SceneEdge>(&serde_json::to_string(&edge).unwrap()).unwrap(),
            edge
        );

        let room = SceneRoom {
            id: 5,
            cell: (-3, 4),
            title: "T".into(),
            building: None,
            paths: "n=6".into(),
        };
        assert_eq!(
            serde_json::to_string(&room).unwrap(),
            r#"{"id":5,"cell":[-3,4],"title":"T","building":null,"paths":"n=6"}"#
        );
        assert_eq!(
            serde_json::from_str::<SceneRoom>(&serde_json::to_string(&room).unwrap()).unwrap(),
            room
        );

        let label = SceneLabel {
            text: "Seaway".into(),
            at: (0.5, 0.5),
            building: Some(9),
        };
        assert_eq!(
            serde_json::to_string(&label).unwrap(),
            r#"{"text":"Seaway","at":[0.5,0.5],"building":9}"#
        );
        assert_eq!(
            serde_json::from_str::<SceneLabel>(&serde_json::to_string(&label).unwrap()).unwrap(),
            label
        );
    }

    #[test]
    fn string_escapes_are_serde_jsons() {
        // The six mandatory escapes, a control character as `\u00XX`, a `/`
        // left alone, and non-ASCII passed through as UTF-8 — the same set the
        // hand-written writer had (it was modelled on
        // `serde_json::ser::format_escaped_str`).
        let scene = MapScene {
            area: "a/b\"c\\d\n\r\t\u{8}\u{c}\u{1}é😀".into(),
            ..Default::default()
        };
        let text = scene.to_json();
        assert_eq!(
            text,
            "{\"area\":\"a/b\\\"c\\\\d\\n\\r\\t\\b\\f\\u0001é😀\",\"rooms\":[],\"edges\":[],\
             \"doors\":[],\"buildings\":[],\"labels\":[],\"min\":[0,0],\"max\":[0,0]}"
        );
        assert_eq!(MapScene::from_json(&text).unwrap().area, scene.area);

        // `\uXXXX` escapes — including a surrogate pair — read back.
        let scene =
            MapScene::from_json(r#"{"area":"\u00e9\ud83d\ude00\u001f"}"#).unwrap();
        assert_eq!(scene.area, "é😀\u{1f}");
    }

    #[test]
    fn numbers_take_serde_jsons_forms() {
        // An `f32` pair may be spelled in any JSON number form, so a sheet
        // written with `1e3` still reads; the integer pairs stay integers.
        let scene =
            MapScene::from_json(r#"{"min":[1000,0],"max":[1,2],"doors":[{"at":[1e3,1.0]}]}"#)
                .unwrap();
        assert_eq!(scene.min, (1000, 0));
        assert_eq!(scene.max, (1, 2));
        assert_eq!(scene.doors[0].at, (1000.0, 1.0));
        // And the writer emits the shortest round-trip form, `.0` included.
        let scene = MapScene {
            min: (1, -2),
            max: (3, 4),
            ..Default::default()
        };
        assert!(scene.to_json().ends_with(r#""min":[1,-2],"max":[3,4]}"#));
    }

    #[test]
    fn a_wrong_type_names_the_field_and_the_line() {
        let err = MapScene::from_json(r#"{"area":7}"#).unwrap_err();
        assert!(err.to_string().contains("invalid type: integer"), "{err}");
        assert!(err.to_string().contains("line 1 column"), "{err}");

        // An unknown `EdgeKind` variant is serde's own wording now.
        let err = MapScene::from_json(
            r#"{"edges":[{"kind":"stub"}]}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown variant `stub`"), "{err}");
        assert!(err.to_string().contains("`Directional`"), "{err}");
    }

    #[test]
    fn the_derived_index_is_not_part_of_the_document() {
        let mut scene = MapScene::from_json(SAMPLE).unwrap();
        // Touch the `OnceLock` so it is populated...
        assert!(scene.room(7116001).is_some());
        // ...and the document is still the eight recovered keys.
        let value: serde_json::Value = serde_json::to_value(&scene).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "area", "buildings", "doors", "edges", "labels", "max", "min", "rooms"
            ]
        );
        // A round trip through the text drops the index, as `skip` implies.
        let again = MapScene::from_json(&scene.to_json()).unwrap();
        assert!(again.by_id.get().is_none());
        assert_eq!(again.room(7116002).unwrap().id, 7116002);
        // `MapScene` equality ignores it either way.
        scene.by_id = OnceLock::new();
        assert_eq!(scene, again);
    }
}
