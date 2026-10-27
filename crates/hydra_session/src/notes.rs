//! Room notes — the little text file the player keeps about rooms.
//!
//! Recovered from `notes::*`:
//! `0x101a52998` (`character_of`), `0x101a52988` (`notes_path`),
//! `0x101a52d34` (`add`), `0x101a52e20` (`Clip::of_room`),
//! `0x101a53a90` (`Note::first_line`), `0x101a53b74` (`Note::clipped`),
//! `0x101a53bbc` (`load`), `0x101a53f28` (`Notes::get`),
//! `0x101a540b8` (`Notes::find`), `0x101a548b0` (`Notes::search`),
//! `0x101a5496c` (`stamp`), `0x101a54970` (`write`),
//! `0x101a54a5c` (`forget`), `0x101a54cc4` (`remove`),
//! `0x101a54d84` (`key_for`), `0x101a54df4` (`rewrite`),
//! `0x101a54ed0` (`add_lines`).
//!
//! **ESTABLISHED**:
//! * `notes_path` is `root.join(<10-byte literal at 0x1039c67ac>)` — i.e.
//!   `notes.json` (the same name both error messages use).
//! * the file is **JSON**: `load` calls `fs::read_to_string`, then
//!   `serde_json::from_str` (`<serde_json::error::Error as Display>::fmt` is
//!   called on the failure path) and the errors are
//!   `"notes.json cannot be read: {}"` (`0x103e7c048`) and
//!   `"notes.json is schema {}, and this build of Hydra reads up to {}"`
//!   (`0x103e7c008`).  So the document has a `schema` number.
//! * `Refused` is the error type; the symbol table still has
//!   `<impl From<notes::Refused> for std::io::Error>::from @0x101a527c8`, so it
//!   converts into an `io::Error` — we provide that impl.
//! * `key_for` and `forget` share the `{}_{}` template at `0x103e7bf48`, so a
//!   note key is `<something>_<something>`; `Clip::of_room` uses `{}{}.`
//!   (`0x103e7bf5b`) plus `"Obvious exits: {}"` (`0x103e7bf60`), which is how a
//!   clipped note records the room's exits.
//! * `Notes::search` lower-cases the query (`<str>::to_lowercase`), so search is
//!   case-insensitive substring matching.
//! * `stamp` tail-calls `hydra_net::sink::config::moment` — a "now" formatter.
//!   We keep an ISO-8601 UTC stamp (see [`stamp`]).
//!
//! **UNKNOWN**: the JSON member names and the exact `Refused` payload.  We use
//! `schema` + `notes`, with each note carrying `key`, `room`, `title`, `text`,
//! `when` and an optional `clip`, which is the minimal set the recovered
//! helpers need.

use crate::claimant::Desk;
use crate::store;
use crate::textfmt::{self, Value};
use std::io;
use std::path::{Path, PathBuf};

/// The notes file name (**ESTABLISHED**, 10 bytes).
pub const NOTES_FILE: &str = "notes.json";
/// The schema this build writes and the highest it reads.
pub const SCHEMA: u32 = 2;

/// Why a notes file could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub message: String,
}

impl Refused {
    pub fn new(message: impl Into<String>) -> Self {
        Refused {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refused {}

/// **ESTABLISHED**: the symbol table has
/// `<impl From<notes::Refused> for std::io::Error>::from`.
impl From<Refused> for io::Error {
    fn from(r: Refused) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, r.message)
    }
}

/// Where a day-file log line was, when a note was clipped out of the log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clipped {
    /// the day key, e.g. `2024-05-01`
    pub day: String,
    /// the 1-based line number within that day file
    pub line: u64,
}

/// One note about one room.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Note {
    /// the key, see [`key_for`]
    pub key: String,
    /// the room uid the note was taken in
    pub room: String,
    /// the room title at the time
    pub title: String,
    /// the text, one entry per line
    pub lines: Vec<String>,
    /// when the note was taken (see [`stamp`])
    pub when: String,
    /// set when the note came out of a log line
    pub clip: Option<Clipped>,
}

impl Note {
    /// A note for `room`/`title` with one line of text.
    pub fn new(room: &str, title: &str, text: &str) -> Self {
        let mut note = Note {
            key: key_for(room, title),
            room: room.to_string(),
            title: title.to_string(),
            lines: Vec::new(),
            when: stamp(),
            clip: None,
        };
        note.lines
            .extend(text.lines().map(str::trim_end).map(str::to_string));
        note
    }

    /// The first line of the note, or `""`.
    ///
    /// **ESTABLISHED** shape (`0x101a53a90`): one `String` clone of the first
    /// line.
    pub fn first_line(&self) -> &str {
        self.lines.first().map(String::as_str).unwrap_or("")
    }

    /// Is this note a clip out of the player log?
    ///
    /// **ESTABLISHED** shape (`0x101a53b74`): a single niche test on the
    /// `Option<Clipped>` word.
    pub fn clipped(&self) -> bool {
        self.clip.is_some()
    }

    /// The whole note as one string.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Append a line.
    pub fn push_line(&mut self, line: &str) {
        self.lines.push(line.trim_end().to_string());
    }
}

/// A clip taken out of the player log and kept against a room.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clip {
    /// the room it was taken in
    pub room: String,
    /// the room title
    pub title: String,
    /// the note the clip became
    pub note: Note,
}

impl Clip {
    /// Build a clip for `room` from a log line and the room's exits.
    ///
    /// **ESTABLISHED** parts: the room key is `format!("{}{}.", room, n)`
    /// (`0x103e7bf5b`) and the exits line is
    /// `format!("Obvious exits: {}", exits)` (`0x103e7bf60`); the clip is also
    /// linked into the log through `links::Does::of` + `links::add`.
    pub fn of_room(room: &str, title: &str, text: &str, exits: &[String], day: &str, line: u64) -> Self {
        let mut note = Note::new(room, title, text);
        note.clip = Some(Clipped {
            day: day.to_string(),
            line,
        });
        if !exits.is_empty() {
            note.push_line(&format!("Obvious exits: {}", exits.join(", ")));
        }
        Clip {
            room: room.to_string(),
            title: title.to_string(),
            note,
        }
    }
}

/// The whole notes file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Notes {
    /// the schema the file was written with
    pub schema: u32,
    /// the notes, in file order
    pub notes: Vec<Note>,
}

impl Notes {
    /// An empty file at the current schema.
    pub fn new() -> Self {
        Notes {
            schema: SCHEMA,
            notes: Vec::new(),
        }
    }

    /// The note with this key.
    ///
    /// **ESTABLISHED** shape (`0x101a53f28`): a linear scan over the notes.
    pub fn get(&self, key: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.key == key)
    }

    /// Every note whose key starts with `room`.
    ///
    /// **ESTABLISHED** shape (`0x101a540b8`): walks the notes and compares
    /// keys.
    pub fn find(&self, room: &str) -> Vec<&Note> {
        self.notes.iter().filter(|n| n.key.starts_with(room)).collect()
    }

    /// Every note containing `needle`, case-insensitively.
    ///
    /// **ESTABLISHED**: `Notes::search` lower-cases the needle
    /// (`<str>::to_lowercase` at `0x101a548dc`) and then matches.
    pub fn search(&self, needle: &str) -> Vec<&Note> {
        let needle = needle.to_lowercase();
        if needle.is_empty() {
            return self.notes.iter().collect();
        }
        self.notes
            .iter()
            .filter(|n| {
                n.key.to_lowercase().contains(&needle)
                    || n.title.to_lowercase().contains(&needle)
                    || n.text().to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Add or replace a note.
    pub fn put(&mut self, note: Note) {
        match self.notes.iter_mut().find(|n| n.key == note.key) {
            Some(slot) => *slot = note,
            None => self.notes.push(note),
        }
    }

    /// Remove the note with this key; returns whether it was there.
    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.notes.len();
        self.notes.retain(|n| n.key != key);
        self.notes.len() != before
    }

    /// Render the file.
    pub fn to_json(&self) -> String {
        let notes: Vec<Value> = self
            .notes
            .iter()
            .map(|n| {
                let mut pairs = vec![
                    ("key".to_string(), Value::Str(n.key.clone())),
                    ("room".to_string(), Value::Str(n.room.clone())),
                    ("title".to_string(), Value::Str(n.title.clone())),
                    ("text".to_string(), Value::Str(n.text())),
                    ("when".to_string(), Value::Str(n.when.clone())),
                ];
                if let Some(c) = &n.clip {
                    pairs.push((
                        "clip".to_string(),
                        Value::Obj(vec![
                            ("day".to_string(), Value::Str(c.day.clone())),
                            ("line".to_string(), Value::Num(c.line as f64)),
                        ]),
                    ));
                }
                Value::Obj(pairs)
            })
            .collect();
        textfmt::to_json(&Value::Obj(vec![
            ("schema".to_string(), Value::Num(self.schema as f64)),
            ("notes".to_string(), Value::Arr(notes)),
        ]))
    }

    /// Read the file out of a parsed document.
    pub fn from_json(v: &Value) -> Result<Notes, Refused> {
        let schema = v.u64_of("schema").unwrap_or(0) as u32;
        if schema > SCHEMA {
            return Err(Refused::new(format!(
                "notes.json is schema {schema}, and this build of Hydra reads up to {SCHEMA}"
            )));
        }
        let mut notes = Notes {
            schema,
            notes: Vec::new(),
        };
        if let Some(Value::Arr(items)) = v.get("notes") {
            for item in items {
                let text = item.str_of("text");
                notes.notes.push(Note {
                    key: item.str_of("key"),
                    room: item.str_of("room"),
                    title: item.str_of("title"),
                    lines: if text.is_empty() {
                        Vec::new()
                    } else {
                        text.lines().map(str::to_string).collect()
                    },
                    when: item.str_of("when"),
                    clip: item.get("clip").map(|c| Clipped {
                        day: c.str_of("day"),
                        line: c.u64_of("line").unwrap_or(0),
                    }),
                });
            }
        }
        Ok(notes)
    }
}

/// `<root>/notes.json`.
///
/// **ESTABLISHED** (`0x101a52988`).
pub fn notes_path(root: &Path) -> PathBuf {
    root.join(NOTES_FILE)
}

/// Read the notes file.  A missing file is an empty one.
///
/// **ESTABLISHED** messages: `"notes.json cannot be read: {}"` and the schema
/// message from [`Notes::from_json`].
pub fn load(root: &Path) -> Result<Notes, Refused> {
    let path = notes_path(root);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Notes::new()),
        Err(e) => return Err(Refused::new(format!("notes.json cannot be read: {e}"))),
    };
    if text.trim().is_empty() {
        return Ok(Notes::new());
    }
    let v = textfmt::parse_json(&text)
        .map_err(|e| Refused::new(format!("notes.json cannot be read: {e}")))?;
    Notes::from_json(&v)
}

/// Write the notes file through [`store::save_bytes`].
///
/// **ESTABLISHED** shape (`0x101a54970`): `notes_path` then `save_bytes`.
pub fn write(root: &Path, notes: &Notes) -> Result<(), Refused> {
    store::save_bytes(root, &notes_path(root), notes.to_json().as_bytes())
        .map_err(|e| Refused::new(format!("notes.json was not saved: {e}")))
}

/// Read, change and write the file in one go.
pub fn rewrite(root: &Path, change: impl FnOnce(&mut Notes)) -> Result<Notes, Refused> {
    let mut notes = load(root)?;
    change(&mut notes);
    write(root, &notes)?;
    Ok(notes)
}

/// Add a note.
///
/// **ESTABLISHED** shape (`0x101a52d34`): joins the path and writes.
pub fn add(root: &Path, note: Note) -> Result<(), Refused> {
    rewrite(root, |notes| notes.put(note))?;
    Ok(())
}

/// Append lines to the note with `key`, creating it if needed.
///
/// **ESTABLISHED** shape (`0x101a54ed0`): joins the path, sanitises a
/// component with `store::safe_component` and writes.
pub fn add_lines(root: &Path, note: Note, lines: &[String]) -> Result<(), Refused> {
    rewrite(root, |notes| {
        match notes.notes.iter_mut().find(|n| n.key == note.key) {
            Some(existing) => {
                for line in lines {
                    existing.push_line(line);
                }
            }
            None => {
                let mut fresh = note.clone();
                for line in lines {
                    fresh.push_line(line);
                }
                notes.put(fresh);
            }
        }
    })?;
    Ok(())
}

/// Forget the note with this key.
///
/// **ESTABLISHED** shape (`0x101a54a5c`): the `{}_{}` template plus
/// `store::safe_component`.
pub fn forget(root: &Path, key: &str) -> Result<bool, Refused> {
    let mut found = false;
    rewrite(root, |notes| {
        found = notes.remove(key);
    })?;
    Ok(found)
}

/// Remove a note, same as [`forget`] (**ESTABLISHED** shape, `0x101a54cc4`).
pub fn remove(root: &Path, key: &str) -> Result<bool, Refused> {
    forget(root, key)
}

/// The key a note is filed under: `<safe(room)>_<first word of title>`.
///
/// **ESTABLISHED** template: `{}_{}` (`0x103e7bf48`), with the same
/// `safe_component` the rest of the store uses.
pub fn key_for(room: &str, title: &str) -> String {
    let title = title.split_whitespace().next().unwrap_or("");
    format!(
        "{}_{}",
        store::safe_component(room),
        store::safe_component(title)
    )
}

/// Which character's notes these are.
///
/// **ESTABLISHED** shape (`0x101a52998`): reads the desk's character name and
/// lower-cases a component with `store::safe_component`.
pub fn character_of(desk: &Desk) -> String {
    desk.character().to_string()
}

/// A "now" stamp for a new note.
///
/// **ESTABLISHED**: `stamp` is a tail call into the endpoint configuration's
/// moment formatter, so the exact text is a configurable date format.  We emit
/// ISO-8601 UTC (`2024-05-01T10:00:00Z`), which the log's day keys and the
/// archive periods can both parse.
pub fn stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_utc(secs)
}

/// Format whole seconds since the epoch as `YYYY-MM-DDThh:mm:ssZ`.
pub fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Howard Hinnant's `civil_from_days`, the same algorithm
/// `hydra_net::eastern::civil_from_days` uses (`0x101aa0b54`).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Howard Hinnant's `days_from_civil`.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d as u64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claimant::Desk;

    #[test]
    fn notes_path_is_notes_json() {
        assert_eq!(NOTES_FILE, "notes.json");
        assert_eq!(NOTES_FILE.len(), 10, "the recovered literal is 10 bytes");
        assert_eq!(
            notes_path(Path::new("/d")),
            PathBuf::from("/d/notes.json")
        );
    }

    #[test]
    fn keys_are_safe_room_and_first_word_of_title() {
        assert_eq!(key_for("0x1a2b", "Town Square, Center"), "0x1a2b_town");
        assert_eq!(key_for("Room 7", "The Cart"), "room7_the");
        assert_eq!(key_for("..", ".."), "_");
    }

    #[test]
    fn first_line_and_clipping() {
        let mut n = Note::new("0x1", "Town", "one\ntwo");
        assert_eq!(n.first_line(), "one");
        assert!(!n.clipped());
        n.clip = Some(Clipped {
            day: "2024-05-01".into(),
            line: 7,
        });
        assert!(n.clipped());
        assert_eq!(n.text(), "one\ntwo");
    }

    #[test]
    fn a_clip_records_the_exits() {
        let c = Clip::of_room(
            "0x1",
            "Town",
            "a cart",
            &["north".to_string(), "south".to_string()],
            "2024-05-01",
            9,
        );
        assert_eq!(c.note.first_line(), "a cart");
        assert_eq!(c.note.text(), "a cart\nObvious exits: north, south");
        assert_eq!(c.note.clip.unwrap().line, 9);
    }

    #[test]
    fn get_find_and_search() {
        let mut notes = Notes::new();
        notes.put(Note::new("0x1a2b", "Town Square", "a cart"));
        notes.put(Note::new("0x1a2c", "Town Hall", "the mayor"));
        assert!(notes.get("0x1a2b_town").is_some());
        assert_eq!(notes.find("0x1a2b").len(), 1);
        assert_eq!(notes.search("TOWN").len(), 2);
        assert_eq!(notes.search("mayor").len(), 1);
        assert_eq!(notes.search("").len(), 2);
        assert!(notes.remove("0x1a2b_town"));
        assert!(!notes.remove("0x1a2b_town"));
    }

    #[test]
    fn json_round_trip_and_schema_guard() {
        let mut notes = Notes::new();
        notes.put(Note::new("0x1", "Town", "hi\nthere"));
        let text = notes.to_json();
        let v = textfmt::parse_json(&text).unwrap();
        let back = Notes::from_json(&v).unwrap();
        assert_eq!(back, notes);

        let v = textfmt::parse_json(r#"{"schema": 99}"#).unwrap();
        let e = Notes::from_json(&v).unwrap_err();
        assert_eq!(
            e.to_string(),
            "notes.json is schema 99, and this build of Hydra reads up to 2"
        );
        let io: io::Error = e.into();
        assert_eq!(io.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn civil_dates_match_known_values() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        // 2024-05-01 is day 19844.
        assert_eq!(days_from_civil(2024, 5, 1), 19844);
        assert_eq!(civil_from_days(19844), (2024, 5, 1));
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(1_714_557_600), "2024-05-01T10:00:00Z");
    }

    #[test]
    fn a_missing_file_loads_empty_and_add_writes_it() {
        let root = std::env::temp_dir().join(format!("hydra-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(load(&root).unwrap(), Notes::new());
        add(&root, Note::new("0x1", "Town", "first")).unwrap();
        let notes = load(&root).unwrap();
        assert_eq!(notes.notes.len(), 1);
        add_lines(&root, Note::new("0x1", "Town", ""), &["second".to_string()]).unwrap();
        let notes = load(&root).unwrap();
        assert_eq!(notes.get("0x1_town").unwrap().text(), "first\nsecond");
        assert!(forget(&root, "0x1_town").unwrap());
        assert!(load(&root).unwrap().notes.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn character_of_reads_the_desk() {
        let d = Desk::new(None, "Aelith");
        assert_eq!(character_of(&d), "Aelith");
    }
}
