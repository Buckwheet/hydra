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

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::json::{self, Value, Writer};

/// What kind of line an edge is drawn as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, Default, PartialEq)]
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
#[derive(Clone, Debug, Default, PartialEq)]
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
#[derive(Clone, Debug, Default, PartialEq)]
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneLabel {
    pub text: String,
    pub at: (f32, f32),
    pub building: Option<u64>,
}

/// The index `MapScene::room` builds lazily (`hydra_map::scene::ById`).
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
#[derive(Clone, Debug, Default)]
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

    /// The `Serialize` impl, entry for entry, in the recovered order.
    pub fn to_json(&self) -> String {
        let mut w = Writer::new();
        w.begin_object();
        w.key("area");
        w.str(&self.area);
        w.key("rooms");
        w.begin_array();
        for r in &self.rooms {
            r.write(&mut w);
        }
        w.end_array();
        w.key("edges");
        w.begin_array();
        for e in &self.edges {
            e.write(&mut w);
        }
        w.end_array();
        w.key("doors");
        w.begin_array();
        for d in &self.doors {
            d.write(&mut w);
        }
        w.end_array();
        w.key("buildings");
        w.begin_array();
        for b in &self.buildings {
            w.str(b);
        }
        w.end_array();
        w.key("labels");
        w.begin_array();
        for l in &self.labels {
            l.write(&mut w);
        }
        w.end_array();
        w.key("min");
        pair_i32(&mut w, self.min);
        w.key("max");
        pair_i32(&mut w, self.max);
        w.end_object();
        w.into_string()
    }

    /// The `Deserialize` side.
    pub fn from_json(text: &str) -> Result<MapScene, json::Error> {
        let v = json::parse(text)?;
        MapScene::from_value(&v)
    }

    pub fn from_value(v: &Value) -> Result<MapScene, json::Error> {
        let obj = want_object(v)?;
        let mut scene = MapScene::default();
        for (key, value) in obj {
            match key.as_str() {
                // The field visitor's tag numbers, in order: area=0 (len 4),
                // rooms=1 (5), edges=2 (5), doors=3 (5), buildings=4 (9),
                // labels=5 (6), min=6 (3), max=7 (3); anything else is the
                // `ignore` arm (tag 8) and is skipped.
                "area" => scene.area = want_string(value)?.to_string(),
                "rooms" => {
                    scene.rooms = want_array(value)?
                        .iter()
                        .map(SceneRoom::from_value)
                        .collect::<Result<_, _>>()?
                }
                "edges" => {
                    scene.edges = want_array(value)?
                        .iter()
                        .map(SceneEdge::from_value)
                        .collect::<Result<_, _>>()?
                }
                "doors" => {
                    scene.doors = want_array(value)?
                        .iter()
                        .map(SceneDoor::from_value)
                        .collect::<Result<_, _>>()?
                }
                "buildings" => {
                    scene.buildings = want_array(value)?
                        .iter()
                        .map(|b| Ok(want_string(b)?.to_string()))
                        .collect::<Result<_, json::Error>>()?
                }
                "labels" => {
                    scene.labels = want_array(value)?
                        .iter()
                        .map(SceneLabel::from_value)
                        .collect::<Result<_, _>>()?
                }
                "min" => scene.min = pair_i32_from(value)?,
                "max" => scene.max = pair_i32_from(value)?,
                _ => {}
            }
        }
        Ok(scene)
    }
}

impl SceneDoor {
    fn write(&self, w: &mut Writer) {
        w.begin_object();
        w.key("street");
        w.u32(self.street);
        w.key("inside");
        w.u32(self.inside);
        w.key("at");
        pair_f32(w, self.at);
        w.key("place");
        w.str(&self.place);
        w.key("named");
        w.bool(self.named);
        w.key("marks");
        w.begin_array();
        for m in &self.marks {
            w.str(m);
        }
        w.end_array();
        w.end_object();
    }

    fn from_value(v: &Value) -> Result<SceneDoor, json::Error> {
        let obj = want_object(v)?;
        let mut d = SceneDoor::default();
        for (key, value) in obj {
            match key.as_str() {
                "street" => d.street = want_u32(value)?,
                "inside" => d.inside = want_u32(value)?,
                "at" => d.at = pair_f32_from(value)?,
                "place" => d.place = want_string(value)?.to_string(),
                "named" => d.named = want_bool(value)?,
                "marks" => {
                    d.marks = want_array(value)?
                        .iter()
                        .map(|m| Ok(want_string(m)?.to_string()))
                        .collect::<Result<_, json::Error>>()?
                }
                _ => {}
            }
        }
        Ok(d)
    }
}

impl SceneEdge {
    fn write(&self, w: &mut Writer) {
        w.begin_object();
        w.key("a");
        w.u32(self.a);
        w.key("b");
        w.u32(self.b);
        w.key("kind");
        w.str(self.kind.name());
        w.key("path");
        w.begin_array();
        for p in &self.path {
            pair_f32(w, *p);
        }
        w.end_array();
        w.key("label");
        match &self.label {
            Some(l) => w.str(l),
            None => w.null(),
        }
        w.key("building");
        match self.building {
            Some(b) => w.u64(b),
            None => w.null(),
        }
        w.end_object();
    }

    fn from_value(v: &Value) -> Result<SceneEdge, json::Error> {
        let obj = want_object(v)?;
        let mut e = SceneEdge::default();
        for (key, value) in obj {
            match key.as_str() {
                "a" => e.a = want_u32(value)?,
                "b" => e.b = want_u32(value)?,
                "kind" => {
                    let name = want_string(value)?;
                    e.kind = EdgeKind::named(name).ok_or_else(|| {
                        json::Error(format!(
                            "unknown variant `{name}`, expected one of \
                             `Directional`, `Connector`, `Stub`"
                        ))
                    })?;
                }
                "path" => {
                    e.path = want_array(value)?
                        .iter()
                        .map(pair_f32_from)
                        .collect::<Result<_, _>>()?
                }
                "label" => {
                    e.label = match value {
                        Value::Null => None,
                        other => Some(want_string(other)?.to_string()),
                    }
                }
                "building" => e.building = want_opt_u64(value)?,
                _ => {}
            }
        }
        Ok(e)
    }
}

impl SceneRoom {
    fn write(&self, w: &mut Writer) {
        w.begin_object();
        w.key("id");
        w.u32(self.id);
        w.key("cell");
        pair_i32(w, self.cell);
        w.key("title");
        w.str(&self.title);
        w.key("building");
        match self.building {
            Some(b) => w.u64(b),
            None => w.null(),
        }
        w.key("paths");
        w.str(&self.paths);
        w.end_object();
    }

    fn from_value(v: &Value) -> Result<SceneRoom, json::Error> {
        let obj = want_object(v)?;
        let mut r = SceneRoom::default();
        for (key, value) in obj {
            match key.as_str() {
                "id" => r.id = want_u32(value)?,
                "cell" => r.cell = pair_i32_from(value)?,
                "title" => r.title = want_string(value)?.to_string(),
                "building" => r.building = want_opt_u64(value)?,
                "paths" => r.paths = want_string(value)?.to_string(),
                _ => {}
            }
        }
        Ok(r)
    }
}

impl SceneLabel {
    fn write(&self, w: &mut Writer) {
        w.begin_object();
        w.key("text");
        w.str(&self.text);
        w.key("at");
        pair_f32(w, self.at);
        w.key("building");
        match self.building {
            Some(b) => w.u64(b),
            None => w.null(),
        }
        w.end_object();
    }

    fn from_value(v: &Value) -> Result<SceneLabel, json::Error> {
        let obj = want_object(v)?;
        let mut l = SceneLabel::default();
        for (key, value) in obj {
            match key.as_str() {
                "text" => l.text = want_string(value)?.to_string(),
                "at" => l.at = pair_f32_from(value)?,
                "building" => l.building = want_opt_u64(value)?,
                _ => {}
            }
        }
        Ok(l)
    }
}

fn pair_f32(w: &mut Writer, p: (f32, f32)) {
    w.begin_array();
    w.f32(p.0);
    w.f32(p.1);
    w.end_array();
}

fn pair_i32(w: &mut Writer, p: (i32, i32)) {
    w.begin_array();
    w.i32(p.0);
    w.i32(p.1);
    w.end_array();
}

fn want_object(v: &Value) -> Result<&std::collections::BTreeMap<String, Value>, json::Error> {
    v.as_object()
        .ok_or_else(|| json::Error("invalid type: expected a map".into()))
}

fn want_array(v: &Value) -> Result<&[Value], json::Error> {
    v.as_array()
        .ok_or_else(|| json::Error("invalid type: expected a sequence".into()))
}

fn want_string(v: &Value) -> Result<&str, json::Error> {
    v.as_str()
        .ok_or_else(|| json::Error("invalid type: expected a string".into()))
}

fn want_bool(v: &Value) -> Result<bool, json::Error> {
    v.as_bool()
        .ok_or_else(|| json::Error("invalid type: expected a boolean".into()))
}

fn want_u32(v: &Value) -> Result<u32, json::Error> {
    let n = v
        .as_i64()
        .ok_or_else(|| json::Error("invalid type: expected u32".into()))?;
    u32::try_from(n).map_err(|_| json::Error("invalid value: u32 out of range".into()))
}

fn want_opt_u64(v: &Value) -> Result<Option<u64>, json::Error> {
    match v {
        Value::Null => Ok(None),
        other => {
            let n = other
                .as_i64()
                .ok_or_else(|| json::Error("invalid type: expected u64".into()))?;
            u64::try_from(n)
                .map(Some)
                .map_err(|_| json::Error("invalid value: u64 out of range".into()))
        }
    }
}

fn pair_f32_from(v: &Value) -> Result<(f32, f32), json::Error> {
    let a = want_array(v)?;
    if a.len() != 2 {
        return Err(json::Error("invalid length 2, expected a pair".into()));
    }
    let f = |x: &Value| {
        x.as_f64()
            .map(|f| f as f32)
            .ok_or_else(|| json::Error("invalid type: expected f32".into()))
    };
    Ok((f(&a[0])?, f(&a[1])?))
}

fn pair_i32_from(v: &Value) -> Result<(i32, i32), json::Error> {
    let a = want_array(v)?;
    if a.len() != 2 {
        return Err(json::Error("invalid length 2, expected a pair".into()));
    }
    let f = |x: &Value| {
        x.as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| json::Error("invalid type: expected i32".into()))
    };
    Ok((f(&a[0])?, f(&a[1])?))
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
}
