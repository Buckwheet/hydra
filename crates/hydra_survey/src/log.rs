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
//! **Phase 1:** the derives are `serde`'s again (`serde` 1.0.229 /
//! `serde_json` 1.0.151, both workspace pins), and [`parse`] is
//! `serde_json::from_str` — the recovered `serde_json::de::from_trait::<Record>`
//! at `0x1013c6f50`. `Serialize` is derived as well: the original had no
//! `Serialize` side, so it is *added* rather than recovered, and nothing in the
//! crate depends on it beyond the round-trip tests.
//!
//! Fields whose *shape* is recovered but whose element/primitive type is only
//! known by size are marked `// UNKNOWN:` where they are declared.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};

use serde::{Deserialize, Deserializer, Serialize};

/// How a survey walk arrived at something: the room it came `from` and the
/// command that moved it.
///
/// The two field names are read out of `ContentDeserializer::deserialize_struct`
/// at `0x1013bc8f0`, whose key comparisons are `from` (tag 0) and `cmd` (tag 1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Via {
    pub from: String,
    pub cmd: String,
}

/// A room the walk has stood in.
///
/// Field order is the `FIELDS` array order at `0x1044337e8`:
/// `sid uid title desc paths compass exits roommeta via pos climate terrain
/// animals overhead sensed_exit tracked location`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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
/// trailing fields own no allocation: `rt` at `+0x48`, `cast_rt` at `+0x50` and
/// `prompt_closed` as the byte at `+0x58` (the `visit_map`/`visit_seq` epilogue
/// stores them as `stp d8, d9, [x24, #0x48]` and `strb w8, [x24, #0x58]`, for a
/// 96-byte record).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct VerbRecord {
    pub sid: String,
    pub verb: String,
    pub lines: Vec<String>,
    // UNKNOWN: the recovered 8-byte slot holds a roundtime, but the field is
    // deserialised through `Option::<T>::deserialize` (`0x1013cc7d4`, twice in
    // the `VerbRecord` visitor), so the original accepted an absent or null
    // roundtime. Kept as the `f64` this crate has always exposed, with
    // [`roundtime`] folding "absent" and `null` into `0.0`; see `NOTES.md`.
    #[serde(default, deserialize_with = "roundtime")]
    pub rt: f64,
    #[serde(default, deserialize_with = "roundtime")]
    pub cast_rt: f64,
    // UNKNOWN: a `bool::deserialize` (`0x1013c9acc`) arm proves the flag and
    // suggests the original required it. `#[serde(default)]` is a deliberate
    // divergence: the recovered slot is a plain `bool` with no evidenced
    // default, this crate's field has always been `bool`, and `log::read` drops
    // a whole line it cannot parse — a missing flag would lose a record rather
    // than default a flag. Recorded in `NOTES.md`.
    #[serde(default)]
    pub prompt_closed: bool,
}

/// Read a roundtime: a number, or `null`, or a missing key (`0.0`).
///
/// The binary deserialises both roundtime slots through `Option::<T>` (see
/// [`VerbRecord`]), i.e. the log may leave them out or write `null`. This
/// crate's public field is a plain `f64`, so the two absent forms fold to `0.0`
/// — the behaviour the `std` stand-in had. An explicit compatibility choice,
/// recorded in `NOTES.md`.
fn roundtime<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(0.0))
}

/// How an attempted move ended (`VARIANTS` at `0x1044337b8`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize,
)]
#[serde(rename_all = "lowercase")]
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
}

/// One attempt to leave a room (`FIELDS` at `0x104433748`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct EdgeRecord {
    pub from: String,
    pub cmd: String,
    pub to: Option<String>,
    /// The round time as the game printed it (`Option<String>`, not a number).
    pub rt: Option<String>,
    pub dir: Option<String>,
    pub result: EdgeResult,
    pub lines: Vec<String>,
}

/// Something that happened to the character (`VARIANTS` at `0x104433708`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize,
)]
#[serde(rename_all = "lowercase")]
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
}

/// An event the log recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
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
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Record {
    pub t: String,
    pub run: String,
    pub char: String,
    pub server: String,
    #[serde(flatten)]
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
///
/// The original inlined `serde_json::from_str::<Record>` into [`read`]
/// (`serde_json::de::from_trait::<Record>` at `0x1013c6f50`); it is exposed
/// here for tests and for callers that keep their own I/O. Failures come back
/// as serde's own message, position suffix and all.
pub fn parse(line: &str) -> Result<Record, String> {
    serde_json::from_str::<Record>(line).map_err(|error| error.to_string())
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
        assert_eq!(
            room.roommeta.get("special").map(String::as_str),
            Some("none")
        );
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
        assert!(
            error.contains("expected one of `arrived`, `failed`, `stayed`"),
            "{}",
            error
        );
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
        assert_eq!(
            parse(line).unwrap_err(),
            "missing field `sid` at line 1 column 155"
        );
        let no_kind = r#"{"t":"1","run":"r","char":"c","server":"s"}"#;
        assert_eq!(
            parse(no_kind).unwrap_err(),
            "missing field `kind` at line 1 column 43"
        );
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

    // ---- Phase 1: the recovered schema, through `serde` and not the hand
    // ---- reader. Every test below goes through `serde_json` directly.

    #[test]
    fn serde_reads_one_line_of_each_kind() {
        let room: Record = serde_json::from_str(ROOM).unwrap();
        assert!(matches!(room.body, Body::Room(_)));
        let edge: Record = serde_json::from_str(EDGE).unwrap();
        assert!(matches!(edge.body, Body::Edge(_)));
        let verb: Record = serde_json::from_str(VERB).unwrap();
        assert!(matches!(verb.body, Body::Verb(_)));
        let event: Record = serde_json::from_str(EVENT).unwrap();
        assert!(matches!(event.body, Body::Event(_)));
    }

    #[test]
    fn the_body_keys_are_flattened_into_the_header_object() {
        // `Record::body` is `#[serde(flatten)]`: the `kind` tag and the body's
        // own keys sit in the same object as `t`/`run`/`char`/`server`, which
        // is what the binary's "other key" arm at `0x1013c30c0` buffers.
        let value: serde_json::Value = serde_json::from_str(EDGE).unwrap();
        let object = value.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "char", "cmd", "dir", "from", "kind", "lines", "result", "rt", "run", "server",
                "t", "to"
            ]
        );
        assert_eq!(object["kind"], serde_json::Value::from("edge"));

        // A nested body (`{"kind":"edge","edge":{…}}`) is *not* the format.
        let nested = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"event","event":{"sid":"s1","what":"died"}}"#;
        assert!(serde_json::from_str::<Record>(nested).is_err());
    }

    #[test]
    fn every_record_type_round_trips_through_serde() {
        for line in [ROOM, EDGE, VERB, EVENT] {
            let record: Record = serde_json::from_str(line).unwrap();
            let text = serde_json::to_string(&record).unwrap();
            let again: Record = serde_json::from_str(&text).unwrap();
            assert_eq!(record, again, "{}", text);
            // ... and the same through the crate's own entry point.
            assert_eq!(parse(&text).unwrap(), record);
        }
    }

    #[test]
    fn the_room_schema_is_the_seventeen_recovered_fields_in_order() {
        // `serde` writes a struct's fields in declaration order, so this pins
        // the recovered `FIELDS` order at `0x1044337e8` as well as every type.
        assert_eq!(
            serde_json::to_string(&RoomRecord::default()).unwrap(),
            r#"{"sid":"","uid":null,"title":"","desc":"","paths":"","compass":[],"exits":[],"roommeta":{},"via":null,"pos":null,"climate":null,"terrain":null,"animals":[],"overhead":null,"sensed_exit":null,"tracked":[],"location":null}"#
        );
        // ... and the other four, whose `FIELDS` tables are 7, 6 and 2 wide.
        assert_eq!(
            serde_json::to_string(&EdgeRecord::default()).unwrap(),
            r#"{"from":"","cmd":"","to":null,"rt":null,"dir":null,"result":"arrived","lines":[]}"#
        );
        assert_eq!(
            serde_json::to_string(&VerbRecord::default()).unwrap(),
            r#"{"sid":"","verb":"","lines":[],"rt":0.0,"cast_rt":0.0,"prompt_closed":false}"#
        );
        assert_eq!(
            serde_json::to_string(&EventRecord::default()).unwrap(),
            r#"{"sid":"","what":"died"}"#
        );
        assert_eq!(
            serde_json::to_string(&Via::default()).unwrap(),
            r#"{"from":"","cmd":""}"#
        );
    }

    #[test]
    fn a_record_is_the_header_then_the_flattened_body() {
        assert_eq!(
            serde_json::to_string(&Record::default()).unwrap(),
            r#"{"t":"","run":"","char":"","server":"","kind":"event","sid":"","what":"died"}"#
        );
        // Every `kind` tag the binary's `VARIANTS` table carries.
        let tags: Vec<String> = [
            Body::Room(Box::default()),
            Body::Verb(VerbRecord::default()),
            Body::Edge(EdgeRecord::default()),
            Body::Event(EventRecord::default()),
        ]
        .iter()
        .map(|body| {
            let record = Record {
                body: body.clone(),
                ..Record::default()
            };
            serde_json::to_string(&record).unwrap()
        })
        .collect();
        for (text, tag) in tags.iter().zip(["room", "verb", "edge", "event"]) {
            assert!(
                text.contains(&format!(r#""kind":"{}""#, tag)),
                "{}: {}",
                tag,
                text
            );
        }
    }

    #[test]
    fn an_optional_field_may_be_absent_or_null() {
        let absent = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"room","sid":"s1","title":"T","desc":"D","paths":"","compass":[],"exits":[],"roommeta":{},"animals":[],"tracked":[]}"#;
        let nulls = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"room","sid":"s1","uid":null,"title":"T","desc":"D","paths":"","compass":[],"exits":[],"roommeta":{},"via":null,"pos":null,"climate":null,"terrain":null,"animals":[],"overhead":null,"sensed_exit":null,"tracked":[],"location":null}"#;
        for line in [absent, nulls] {
            let Body::Room(room) = serde_json::from_str::<Record>(line).unwrap().body else {
                panic!("not a room record");
            };
            assert_eq!(room.uid, None, "{}", line);
            assert_eq!(room.via, None, "{}", line);
            assert_eq!(room.pos, None, "{}", line);
            assert_eq!(room.location, None, "{}", line);
            // ... and the two forms serialize identically.
            assert_eq!(
                serde_json::to_string(&*room).unwrap(),
                serde_json::to_string(&RoomRecord {
                    sid: "s1".to_owned(),
                    title: "T".to_owned(),
                    desc: "D".to_owned(),
                    ..RoomRecord::default()
                })
                .unwrap()
            );
        }
    }

    #[test]
    fn roommeta_is_a_string_to_string_map() {
        let line = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"room","sid":"s1","title":"T","desc":"D","paths":"","compass":[],"exits":[],"roommeta":{"b":"2","a":"1"},"animals":[],"tracked":[]}"#;
        let Body::Room(room) = serde_json::from_str::<Record>(line).unwrap().body else {
            panic!("not a room record");
        };
        // A `BTreeMap`, so the order is the map's, not the document's.
        let entries: Vec<(&str, &str)> = room
            .roommeta
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        assert_eq!(entries, [("a", "1"), ("b", "2")]);
        // A value that is not a string is refused, as `deserialize_string`
        // would refuse it (`0x1013b8038`).
        let numbers = line.replace(r#""a":"1""#, r#""a":1"#);
        let error = serde_json::from_str::<Record>(&numbers)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("invalid type: integer `1`, expected a string"),
            "{}",
            error
        );
    }

    #[test]
    fn pos_is_an_option_of_three_integers() {
        let base = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"room","sid":"s1","title":"T","desc":"D","paths":"","compass":[],"exits":[],"roommeta":{},"animals":[],"tracked":[],"pos":POS}"#;
        for (pos, want) in [
            ("[12,-4,0]", Some((12, -4, 0))),
            ("[-1,2,3]", Some((-1, 2, 3))),
            ("null", None),
        ] {
            let line = base.replace("POS", pos);
            let Body::Room(room) = serde_json::from_str::<Record>(&line).unwrap().body else {
                panic!("not a room record");
            };
            assert_eq!(room.pos, want, "{}", line);
        }
        // Three elements exactly: `invalid_length` is what the binary raises.
        let short = base.replace("POS", "[12,-4]");
        let error = serde_json::from_str::<Record>(&short)
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid length 2"), "{}", error);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let line = r#"{"t":"1","run":"r","char":"c","server":"s","zzz":1,"kind":"event","sid":"s1","what":"died","extra":"ignored"}"#;
        let record: Record = serde_json::from_str(line).unwrap();
        assert_eq!(record.t, "1");
        let Body::Event(event) = record.body else {
            panic!("not an event record");
        };
        assert_eq!(event.what, Happening::Died);
    }

    #[test]
    fn the_error_wording_is_serdes() {
        // Unknown `kind` tag: `TaggedContentVisitor` names all four variants.
        let bad_kind = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"roomy","sid":"s1"}"#;
        let error = serde_json::from_str::<Record>(bad_kind)
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "unknown variant `roomy`, expected one of `room`, `verb`, `edge`, `event` at line 1 column 69"
        );

        // Unknown `what`: the enum's own variant list.
        let bad_what = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"event","sid":"s1","what":"woke"}"#;
        let error = serde_json::from_str::<Record>(bad_what)
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "unknown variant `woke`, expected one of `died`, `healed`, `resumed`, `stopped` at line 1 column 83"
        );

        // A missing header field is reported by name.
        let no_run = r#"{"t":"1","char":"c","server":"s","kind":"event","sid":"s1","what":"died"}"#;
        let error = serde_json::from_str::<Record>(no_run)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "missing field `run` at line 1 column 73");

        // A body field that is missing is reported by name, too.
        let no_sid = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"event","what":"died"}"#;
        let error = serde_json::from_str::<Record>(no_sid)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "missing field `sid` at line 1 column 72");
    }

    #[test]
    fn a_bare_number_is_not_a_string() {
        // The `std` stand-in accepted `{"sid":1710}` (the log writes some ids
        // bare); the recovered arms call `deserialize_string` (`0x1013b7ae0`),
        // so `serde` refuses it. A deliberate, documented tightening.
        let line = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"room","sid":1710,"title":"T","desc":"D","paths":"","compass":[],"exits":[],"roommeta":{},"animals":[],"tracked":[]}"#;
        let error = serde_json::from_str::<Record>(line)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("invalid type: integer `1710`, expected a string"),
            "{}",
            error
        );
        assert!(parse(line).is_err());
    }

    #[test]
    fn a_verb_roundtime_may_be_written_or_left_out() {
        // The binary deserialises both roundtimes through `Option::<T>`
        // (`0x1013cc7d4`); the crate's `f64` fields fold that to `0.0`.
        let base = r#"{"t":"1","run":"r","char":"c","server":"s","kind":"verb","sid":"s1","verb":"lnorth","lines":[]RT}"#;
        for (extra, rt, cast_rt) in [
            (r#","rt":1.5,"cast_rt":2"#, 1.5, 2.0),
            (r#","rt":null,"cast_rt":null"#, 0.0, 0.0),
            ("", 0.0, 0.0),
            (r#","rt":3"#, 3.0, 0.0),
        ] {
            let line = base.replace("RT", extra);
            let Body::Verb(verb) = serde_json::from_str::<Record>(&line).unwrap().body else {
                panic!("not a verb record: {}", line);
            };
            assert_eq!((verb.rt, verb.cast_rt), (rt, cast_rt), "{}", line);
            assert!(!verb.prompt_closed);
        }
        // A roundtime that is not a number is still an error.
        let line = base.replace("RT", r#","rt":"soon""#);
        let error = serde_json::from_str::<Record>(&line)
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid type: string"), "{}", error);
    }

    #[test]
    fn the_two_enums_are_written_as_bare_variant_strings() {
        // `visit_enum` on `Content` reads the bare string (`0x1013c2c94`,
        // `0x1013c2e54`), not a `{"variant":…}` object.
        assert_eq!(
            serde_json::to_string(&Happening::Resumed).unwrap(),
            r#""resumed""#
        );
        assert_eq!(
            serde_json::to_string(&EdgeResult::Stayed).unwrap(),
            r#""stayed""#
        );
        assert_eq!(
            serde_json::from_str::<Happening>(r#""healed""#).unwrap(),
            Happening::Healed
        );
        assert!(serde_json::from_str::<Happening>(r#"{"healed":{}}"#).is_err());
    }
}
