//! The survey log: one JSON object per line, and the records it holds.
//!
//! Recovered from the serde glue the binary carries for this crate (there is no
//! `Serialize` side in the binary — the log is written by another crate, so the
//! field names and orders below come from the `Deserialize` visitors, the
//! `FIELDS`/`VARIANTS` arrays and the `drop_in_place` bodies):
//!
//! | type | evidence |
//! |---|---|
//! | [`Record`] | `__FieldVisitor::visit_str` at `0x1013c2ff0` — `t`, `run`, `char`, `server`, plus an "other" arm that hands the key to a `Content` (`serde(flatten)`); 4×`String` + `Body` in `drop_in_place<Record>` at `0x1013b4d0c` |
//! | [`Body`] | `Deserialize for Body` at `0x1013c2674` — `TaggedContentVisitor::new("kind", "internally tagged enum Body")`, then `deserialize_struct` per variant |
//! | [`RoomRecord`] | `FIELDS` at `0x1044337e8` (17 names), `drop_in_place` at `0x1013bf188`, per-field value visitors in the `visit_map` at `0x1013b785c` (jump table at `0x1024157bc`) |
//! | [`VerbRecord`] | `FIELDS` at `0x1044338f8` (6), `drop_in_place` at `0x1013bf558` |
//! | [`EdgeRecord`] | `FIELDS` at `0x104433748` (7), `drop_in_place` at `0x1013bf088` |
//! | [`EventRecord`] | `FIELDS` at `0x104433958` (2) |
//! | [`Via`] | `visit_map` at `0x1013bc8f0` compares the keys `from` and `cmd`; `drop_in_place<Via>` at `0x1013bf03c` drops two `String`s |
//! | [`Happening`] | `VARIANTS` at `0x104433708` — `died`, `healed`, `resumed`, `stopped` (`visit_enum` at `0x1013c2c94`) |
//! | [`EdgeResult`] | `VARIANTS` at `0x1044337b8` — `arrived`, `failed`, `stayed` (`visit_enum` at `0x1013c2e54`) |
//!
//! Fields whose *shape* is recovered but whose element/primitive type is only
//! known by size are marked `// UNKNOWN:` where they are declared.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};

use crate::json::{self, Value};

/// How a survey walk arrived at something: the room it came `from` and the
/// command that moved it.
///
/// The two field names are read out of `ContentDeserializer::deserialize_struct`
/// at `0x1013bc8f0`, whose key comparisons are `from` (tag 0) and `cmd` (tag 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Via {
    pub from: String,
    pub cmd: String,
}

/// A room the walk has stood in.
///
/// Field order is the `FIELDS` array order at `0x1044337e8`:
/// `sid uid title desc paths compass exits roommeta via pos climate terrain
/// animals overhead sensed_exit tracked location`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomRecord {
    pub sid: String,
    pub uid: Option<String>,
    pub title: String,
    pub desc: String,
    /// The room's `paths` text, not a list — the `visit_map` arm for field 4
    /// calls `deserialize_string` (`0x1013b7d04`).
    pub paths: String,
    pub compass: Vec<String>,
    pub exits: Vec<String>,
    /// `deserialize_map` (arm 7, `0x1013b8038`) with `String` keys *and* values.
    pub roommeta: BTreeMap<String, String>,
    pub via: Option<Via>,
    /// The map cell and level: `deserialize_option` over a 3-tuple (three
    /// `next_element` calls and `invalid_length(1)`/`(2)` in `0x1013b706c`); the
    /// 16-byte field at offset `0x180` of `drop_in_place<RoomRecord>` holds a
    /// tag, an 8-byte pair and one more `i32`.
    pub pos: Option<(i32, i32, i32)>,
    pub climate: Option<String>,
    pub terrain: Option<String>,
    pub animals: Vec<String>,
    pub overhead: Option<String>,
    pub sensed_exit: Option<String>,
    pub tracked: Vec<String>,
    pub location: Option<String>,
}

/// A verb the game answered with, and the lines it answered with.
///
/// Only `sid`, `verb` and `lines` are drop-visible (two `String`s and a
/// `Vec<String>` in `drop_in_place<VerbRecord>` at `0x1013bf558`); the three
/// trailing fields own no allocation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VerbRecord {
    pub sid: String,
    pub verb: String,
    pub lines: Vec<String>,
    // UNKNOWN: `rt`/`cast_rt` are numbers (roundtime, cast roundtime) and
    // `prompt_closed` is a flag; the binary only shows that none of the three
    // owns an allocation. Read as `f64`/`bool` here.
    pub rt: f64,
    pub cast_rt: f64,
    pub prompt_closed: bool,
}

/// How an attempted move ended (`VARIANTS` at `0x1044337b8`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeResult {
    #[default]
    Arrived,
    Failed,
    Stayed,
}

impl EdgeResult {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeResult::Arrived => "arrived",
            EdgeResult::Failed => "failed",
            EdgeResult::Stayed => "stayed",
        }
    }

    fn parse(text: &str) -> Option<EdgeResult> {
        match text {
            "arrived" => Some(EdgeResult::Arrived),
            "failed" => Some(EdgeResult::Failed),
            "stayed" => Some(EdgeResult::Stayed),
            _ => None,
        }
    }
}

/// One attempt to leave a room (`FIELDS` at `0x104433748`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgeRecord {
    pub from: String,
    pub cmd: String,
    pub to: Option<String>,
    pub rt: Option<String>,
    pub dir: Option<String>,
    pub result: EdgeResult,
    pub lines: Vec<String>,
}

/// Something that happened to the character (`VARIANTS` at `0x104433708`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Happening {
    #[default]
    Died,
    Healed,
    Resumed,
    Stopped,
}

impl Happening {
    pub fn as_str(self) -> &'static str {
        match self {
            Happening::Died => "died",
            Happening::Healed => "healed",
            Happening::Resumed => "resumed",
            Happening::Stopped => "stopped",
        }
    }

    fn parse(text: &str) -> Option<Happening> {
        match text {
            "died" => Some(Happening::Died),
            "healed" => Some(Happening::Healed),
            "resumed" => Some(Happening::Resumed),
            "stopped" => Some(Happening::Stopped),
            _ => None,
        }
    }
}

/// An event the log recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventRecord {
    pub sid: String,
    pub what: Happening,
}

/// The body of a log record, tagged by the `kind` key it carries in the same
/// JSON object (`Deserialize for Body` at `0x1013c2674`).
///
/// The room variant is boxed because the binary allocates a 424-byte
/// `RoomRecord` in `Box<RoomRecord>::deserialize` (`0x1013cc85c`); the other
/// three are inline.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Room(Box<RoomRecord>),
    Verb(VerbRecord),
    Edge(EdgeRecord),
    Event(EventRecord),
}

impl Default for Body {
    fn default() -> Body {
        Body::Event(EventRecord::default())
    }
}

impl Body {
    /// The `kind` tag this body is written under.
    pub fn kind(&self) -> &'static str {
        match self {
            Body::Room(_) => "room",
            Body::Verb(_) => "verb",
            Body::Edge(_) => "edge",
            Body::Event(_) => "event",
        }
    }
}

/// One line of the survey log.
///
/// Every line carries the same four header fields (`t`, `run`, `char`,
/// `server` — `Record::__FieldVisitor::visit_str` at `0x1013c2ff0`) and then the
/// `kind`-tagged body's own keys, flattened into the same object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub t: String,
    pub run: String,
    pub char: String,
    pub server: String,
    pub body: Body,
}

/// Read a survey log: one JSON object per line.
///
/// `@0x1013c2878`. Recovered shape: open the path (`OpenOptions`, read-only —
/// the failure path formats `"{}: {}"` from the path and the error, the two
/// `0xC0` placeholders around `": "` at `0x103e697be`), wrap it in a
/// `BufReader` of 8192 bytes, and collect the lines into a `Vec<Record>`.
/// Lines that do not parse are dropped: the only `Err` this can return is the
/// open failure (nothing else writes into the `Result` discriminant).
pub fn read(path: &str) -> Result<Vec<Record>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {}", path, e))?;
    let reader = BufReader::with_capacity(8192, file);
    let mut records = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(record) = parse(&line) {
            records.push(record);
        }
    }
    Ok(records)
}

/// Parse one log line into a [`Record`].
pub fn parse(line: &str) -> Result<Record, String> {
    let value = json::parse(line)?;
    let entries = value
        .as_object()
        .ok_or_else(|| json::wrong_type(&value, "struct Record"))?;

    let record = Record {
        t: string(entries, "t")?,
        run: string(entries, "run")?,
        char: string(entries, "char")?,
        server: string(entries, "server")?,
        body: body(entries)?,
    };
    Ok(record)
}

/// The body of a record: the `kind` tag plus that variant's own keys, which
/// live in the *same* object (the original flattens an internally tagged enum).
fn body(entries: &[(String, Value)]) -> Result<Body, String> {
    let kind = member(entries, "kind").ok_or_else(|| "missing field `kind`".to_owned())?;
    let kind = kind
        .as_str()
        .ok_or_else(|| json::wrong_type(kind, "a string"))?;
    let body = match kind {
        "room" => Body::Room(Box::new(room_record(entries)?)),
        "verb" => Body::Verb(verb_record(entries)?),
        "edge" => Body::Edge(edge_record(entries)?),
        "event" => Body::Event(event_record(entries)?),
        other => {
            return Err(format!(
                "unknown variant `{}`, expected one of `room`, `verb`, `edge`, `event`",
                other
            ))
        }
    };
    Ok(body)
}

fn room_record(entries: &[(String, Value)]) -> Result<RoomRecord, String> {
    let room = RoomRecord {
        sid: string(entries, "sid")?,
        uid: opt_string(entries, "uid")?,
        title: string(entries, "title")?,
        desc: string(entries, "desc")?,
        paths: string(entries, "paths")?,
        compass: strings(entries, "compass")?,
        exits: strings(entries, "exits")?,
        roommeta: string_map(entries, "roommeta")?,
        via: opt_via(entries, "via")?,
        pos: opt_triple(entries, "pos")?,
        climate: opt_string(entries, "climate")?,
        terrain: opt_string(entries, "terrain")?,
        animals: strings(entries, "animals")?,
        overhead: opt_string(entries, "overhead")?,
        sensed_exit: opt_string(entries, "sensed_exit")?,
        tracked: strings(entries, "tracked")?,
        location: opt_string(entries, "location")?,
    };
    Ok(room)
}

fn verb_record(entries: &[(String, Value)]) -> Result<VerbRecord, String> {
    let verb = VerbRecord {
        sid: string(entries, "sid")?,
        verb: string(entries, "verb")?,
        lines: strings(entries, "lines")?,
        rt: number(entries, "rt")?.unwrap_or(0.0),
        cast_rt: number(entries, "cast_rt")?.unwrap_or(0.0),
        prompt_closed: boolean(entries, "prompt_closed")?.unwrap_or(false),
    };
    Ok(verb)
}

fn edge_record(entries: &[(String, Value)]) -> Result<EdgeRecord, String> {
    let result = match opt_string(entries, "result")? {
        None => EdgeResult::default(),
        Some(text) => EdgeResult::parse(&text).ok_or_else(|| {
            format!(
                "unknown variant `{}`, expected one of `arrived`, `failed`, `stayed`",
                text
            )
        })?,
    };
    let edge = EdgeRecord {
        from: string(entries, "from")?,
        cmd: string(entries, "cmd")?,
        to: opt_string(entries, "to")?,
        rt: opt_string(entries, "rt")?,
        dir: opt_string(entries, "dir")?,
        result,
        lines: strings(entries, "lines")?,
    };
    Ok(edge)
}

fn event_record(entries: &[(String, Value)]) -> Result<EventRecord, String> {
    let what = member(entries, "what").ok_or_else(|| "missing field `what`".to_owned())?;
    let what = what
        .as_str()
        .ok_or_else(|| json::wrong_type(what, "a string"))?;
    let what = Happening::parse(what).ok_or_else(|| {
        format!(
            "unknown variant `{}`, expected one of `died`, `healed`, `resumed`, `stopped`",
            what
        )
    })?;
    let event = EventRecord {
        sid: string(entries, "sid")?,
        what,
    };
    Ok(event)
}

fn opt_via(entries: &[(String, Value)], name: &str) -> Result<Option<Via>, String> {
    let Some(value) = member(entries, name) else {
        return Ok(None);
    };
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let inner = value
        .as_object()
        .ok_or_else(|| json::wrong_type(value, "struct Via"))?;
    Ok(Some(Via {
        from: string(inner, "from")?,
        cmd: string(inner, "cmd")?,
    }))
}

fn member<'a>(entries: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    entries
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

fn string(entries: &[(String, Value)], name: &str) -> Result<String, String> {
    let value = member(entries, name)
        .ok_or_else(|| format!("missing field `{}`", name))?;
    match value {
        Value::String(text) => Ok(text.clone()),
        // `serde` would reject these, but ids are written bare sometimes.
        Value::Number(text) => Ok(text.clone()),
        other => Err(format!(
            "field `{}`: {}",
            name,
            json::wrong_type(other, "a string")
        )),
    }
}

fn opt_string(entries: &[(String, Value)], name: &str) -> Result<Option<String>, String> {
    match member(entries, name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Number(text)) => Ok(Some(text.clone())),
        Some(other) => Err(json::wrong_type(other, "a string")),
    }
}

fn strings(entries: &[(String, Value)], name: &str) -> Result<Vec<String>, String> {
    let Some(value) = member(entries, name) else {
        return Ok(Vec::new());
    };
    let items = value.as_array().ok_or_else(|| {
        format!(
            "invalid type for `{}`: {}, expected a sequence",
            name,
            value.kind()
        )
    })?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::String(text) => out.push(text.clone()),
            Value::Number(text) => out.push(text.clone()),
            other => return Err(json::wrong_type(other, "a string")),
        }
    }
    Ok(out)
}

fn number(entries: &[(String, Value)], name: &str) -> Result<Option<f64>, String> {
    match member(entries, name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(text)) => text
            .parse::<f64>()
            .map(Some)
            .map_err(|_| format!("invalid number for `{}`", name)),
        Some(other) => Err(json::wrong_type(other, "a number")),
    }
}

fn boolean(entries: &[(String, Value)], name: &str) -> Result<Option<bool>, String> {
    match member(entries, name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(flag)) => Ok(Some(*flag)),
        Some(other) => Err(json::wrong_type(other, "a boolean")),
    }
}

fn string_map(entries: &[(String, Value)], name: &str) -> Result<BTreeMap<String, String>, String> {
    let Some(value) = member(entries, name) else {
        return Ok(BTreeMap::new());
    };
    let inner = value
        .as_object()
        .ok_or_else(|| json::wrong_type(value, "a map"))?;
    let mut out = BTreeMap::new();
    for (key, item) in inner {
        let text = match item {
            Value::String(text) => text.clone(),
            Value::Number(text) => text.clone(),
            Value::Bool(flag) => flag.to_string(),
            Value::Null => String::new(),
            other => return Err(json::wrong_type(other, "a string")),
        };
        out.insert(key.clone(), text);
    }
    Ok(out)
}

fn opt_triple(entries: &[(String, Value)], name: &str) -> Result<Option<(i32, i32, i32)>, String> {
    let Some(value) = member(entries, name) else {
        return Ok(None);
    };
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let items = value
        .as_array()
        .ok_or_else(|| json::wrong_type(value, "a tuple of length 3"))?;
    if items.len() != 3 {
        return Err(format!(
            "invalid length {}, expected a tuple of length 3",
            items.len()
        ));
    }
    let mut parts = [0i32; 3];
    for (slot, item) in parts.iter_mut().zip(items) {
        let text = match item {
            Value::Number(text) => text,
            other => return Err(json::wrong_type(other, "an integer")),
        };
        *slot = text
            .parse::<i32>()
            .map_err(|_| format!("invalid integer `{}`", text))?;
    }
    Ok(Some((parts[0], parts[1], parts[2])))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One log line of each kind, written the way the front end writes them:
    /// the four header fields, the `kind` tag, then the body's own keys.
    const ROOM: &str = r#"{"t":"1712345678","run":"run-1","char":"Ashryn","server":"Prime","kind":"room","sid":"s1710","uid":"7120","title":"Town Square, Center","desc":"The heart of the Landing.","paths":"to north: 2318","compass":["n","ne","e"],"exits":["north","northeast","east"],"roommeta":{"object":"0","special":"none"},"via":{"from":"s1709","cmd":"south"},"pos":[12,-4,0],"climate":"temperate","terrain":"urban","animals":["pigeons"],"overhead":"sky","sensed_exit":"down","tracked":["north"],"location":"Wehnimer's Landing"}"#;
    const EDGE: &str = r#"{"t":"1712345679","run":"run-1","char":"Ashryn","server":"Prime","kind":"edge","from":"s1710","cmd":"north","to":"s2318","rt":"0.2","dir":"n","result":"arrived","lines":["You walk north.","Town Square, North"]}"#;
    const VERB: &str = r#"{"t":"1712345680","run":"run-1","char":"Ashryn","server":"Prime","kind":"verb","sid":"s1710","verb":"llook","lines":["You see nothing."],"rt":0.5,"cast_rt":0,"prompt_closed":true}"#;
    const EVENT: &str = r#"{"t":"1712345681","run":"run-1","char":"Ashryn","server":"Prime","kind":"event","sid":"s1710","what":"healed"}"#;

    #[test]
    fn reads_a_room_line() {
        let record = parse(ROOM).unwrap();
        assert_eq!(record.t, "1712345678");
        assert_eq!(record.run, "run-1");
        assert_eq!(record.char, "Ashryn");
        assert_eq!(record.server, "Prime");
        assert_eq!(record.body.kind(), "room");
        let Body::Room(room) = record.body else {
            panic!("not a room record");
        };
        assert_eq!(room.sid, "s1710");
        assert_eq!(room.uid.as_deref(), Some("7120"));
        assert_eq!(room.title, "Town Square, Center");
        assert_eq!(room.desc, "The heart of the Landing.");
        assert_eq!(room.paths, "to north: 2318");
        assert_eq!(room.compass, ["n", "ne", "e"]);
        assert_eq!(room.exits, ["north", "northeast", "east"]);
        assert_eq!(room.roommeta.get("object").map(String::as_str), Some("0"));
        assert_eq!(room.roommeta.get("special").map(String::as_str), Some("none"));
        assert_eq!(
            room.via,
            Some(Via {
                from: "s1709".to_owned(),
                cmd: "south".to_owned()
            })
        );
        assert_eq!(room.pos, Some((12, -4, 0)));
        assert_eq!(room.climate.as_deref(), Some("temperate"));
        assert_eq!(room.terrain.as_deref(), Some("urban"));
        assert_eq!(room.animals, ["pigeons"]);
        assert_eq!(room.overhead.as_deref(), Some("sky"));
        assert_eq!(room.sensed_exit.as_deref(), Some("down"));
        assert_eq!(room.tracked, ["north"]);
        assert_eq!(room.location.as_deref(), Some("Wehnimer's Landing"));
    }

    #[test]
    fn reads_an_edge_line() {
        let record = parse(EDGE).unwrap();
        assert_eq!(record.body.kind(), "edge");
        let Body::Edge(edge) = record.body else {
            panic!("not an edge record");
        };
        assert_eq!(edge.from, "s1710");
        assert_eq!(edge.cmd, "north");
        assert_eq!(edge.to.as_deref(), Some("s2318"));
        assert_eq!(edge.rt.as_deref(), Some("0.2"));
        assert_eq!(edge.dir.as_deref(), Some("n"));
        assert_eq!(edge.result, EdgeResult::Arrived);
        assert_eq!(edge.lines, ["You walk north.", "Town Square, North"]);
    }

    #[test]
    fn edge_results_are_the_three_variants() {
        for (name, want) in [
            ("arrived", EdgeResult::Arrived),
            ("failed", EdgeResult::Failed),
            ("stayed", EdgeResult::Stayed),
        ] {
            let line = EDGE.replace("\"arrived\"", &format!("\"{}\"", name));
            let Body::Edge(edge) = parse(&line).unwrap().body else {
                panic!("not an edge record");
            };
            assert_eq!(edge.result, want);
            assert_eq!(want.as_str(), name);
        }
        let broken = EDGE.replace("\"arrived\"", "\"wandered\"");
        let error = parse(&broken).unwrap_err();
        assert!(error.contains("unknown variant `wandered`"), "{}", error);
    }

    #[test]
    fn reads_a_verb_line() {
        let Body::Verb(verb) = parse(VERB).unwrap().body else {
            panic!("not a verb record");
        };
        assert_eq!(verb.sid, "s1710");
        assert_eq!(verb.verb, "llook");
        assert_eq!(verb.lines, ["You see nothing."]);
        assert_eq!(verb.rt, 0.5);
        assert_eq!(verb.cast_rt, 0.0);
        assert!(verb.prompt_closed);
    }

    #[test]
    fn reads_an_event_line() {
        let Body::Event(event) = parse(EVENT).unwrap().body else {
            panic!("not an event record");
        };
        assert_eq!(event.sid, "s1710");
        assert_eq!(event.what, Happening::Healed);

        for (name, want) in [
            ("died", Happening::Died),
            ("healed", Happening::Healed),
            ("resumed", Happening::Resumed),
            ("stopped", Happening::Stopped),
        ] {
            let line = EVENT.replace("\"healed\"", &format!("\"{}\"", name));
            let Body::Event(event) = parse(&line).unwrap().body else {
                panic!("not an event record");
            };
            assert_eq!(event.what, want);
            assert_eq!(want.as_str(), name);
        }
    }

    #[test]
    fn optional_room_fields_may_be_absent() {
        let line = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"room","sid":"s1","title":"T","desc":"D","paths":"","compass":[],"exits":[],"roommeta":{},"animals":[],"tracked":[]}"#;
        let Body::Room(room) = parse(line).unwrap().body else {
            panic!("not a room record");
        };
        assert_eq!(room.sid, "s1");
        assert_eq!(room.uid, None);
        assert_eq!(room.via, None);
        assert_eq!(room.pos, None);
        assert_eq!(room.climate, None);
        assert!(room.roommeta.is_empty());
    }

    #[test]
    fn required_fields_are_reported() {
        let line = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"room","title":"T","desc":"D","paths":"","compass":[],"exits":[],"roommeta":{},"animals":[],"tracked":[]}"#;
        assert_eq!(parse(line).unwrap_err(), "missing field `sid`");
        let no_kind = r#"{"t":"1","run":"r","char":"c","server":"s"}"#;
        assert_eq!(parse(no_kind).unwrap_err(), "missing field `kind`");
    }

    #[test]
    fn reads_a_log_of_lines() {
        let dir = std::env::temp_dir().join(format!("hydra-survey-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("survey-run-1.log");
        let text = format!("{}\n{}\n\nnot json\n{}\n{}\n", ROOM, EDGE, VERB, EVENT);
        std::fs::write(&path, text).unwrap();

        let records = read(path.to_str().unwrap()).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).ok();

        // The unparsable and blank lines are dropped.
        assert_eq!(records.len(), 4);
        let kinds: Vec<&str> = records.iter().map(|record| record.body.kind()).collect();
        assert_eq!(kinds, ["room", "edge", "verb", "event"]);
    }

    #[test]
    fn a_missing_file_is_an_error_naming_the_path() {
        let error = read("/nonexistent/hydra/survey.log").unwrap_err();
        assert!(
            error.starts_with("/nonexistent/hydra/survey.log: "),
            "{}",
            error
        );
    }
}
