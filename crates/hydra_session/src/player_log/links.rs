//! Links between notes and log lines.
//!
//! Recovered from `player_log::links::{add, once, read, written, Does::of}` at
//! `0x101a50418`, `0x101a50878`, `0x101a50a6c`, `0x101a50bc0`, `0x101a506b8`.
//!
//! **ESTABLISHED**:
//! * the file is **JSON** (`<impl Deserialize for links::LogLink>::…` and
//!   `<impl Deserialize for links::Does>::…` are both in the symbol table, with
//!   two different `__FieldVisitor`s, so `Does` is a field-bearing struct or
//!   enum of its own).
//! * `written` (`0x101a50bc0`) formats with three adjacent arguments and no
//!   literal text between them (`0x103e413c1` is `{}` `{}` `{}`), i.e. it
//!   builds one key/id out of three values — a record written by the log
//!   writer (which calls `links::written` for every line it writes, see
//!   `0x101a511a8`).
//! * `read` sorts its result (`heapsort`), so links come back in a defined
//!   order.
//! * `Does::of` (`0x101a506b8`) takes something and returns a `Does`; `once`
//!   (`0x101a50878`) only writes when the link is not already there.
//!
//! **UNKNOWN**: the JSON member names, and what the third `written` argument
//! is.  We model a link as `{name, day, line}` and write `<name><day><line>`
//! as the recovered three-argument id.

use crate::textfmt::{self, Value};
use std::path::{Path, PathBuf};

/// The links file name.
pub const LINKS_FILE: &str = "links.json";
/// The schema this build writes.
pub const SCHEMA_VERSION: u32 = 1;

/// One link: a name (usually a note's key) pointing at a log line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogLink {
    /// what the link is called
    pub name: String,
    /// the day key, e.g. `2024-05-01`
    pub day: String,
    /// the 1-based line number in that day file
    pub line: u64,
}

impl LogLink {
    /// A link from `name` to `day` line `line`.
    pub fn new(name: &str, day: &str, line: u64) -> Self {
        LogLink {
            name: name.to_string(),
            day: day.to_string(),
            line,
        }
    }

    /// The identity `written` builds: the three parts with nothing between
    /// them (**ESTABLISHED**: the `{}` `{}` `{}` template).
    pub fn id(&self) -> String {
        format!("{}{}{}", self.name, self.day, self.line)
    }
}

/// What a link link does: add or drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Does {
    /// record the link
    #[default]
    Add,
    /// remove the link
    Remove,
}

impl Does {
    /// The verb a word names.  **ESTABLISHED** shape: `Does::of` maps a word
    /// to the enum, and the JSON has a field for it.
    pub fn of(word: &str) -> Option<Does> {
        match word.to_ascii_lowercase().as_str() {
            "add" | "link" | "linked" => Some(Does::Add),
            "remove" | "unlink" | "forget" => Some(Does::Remove),
            _ => None,
        }
    }

    /// The word this verb is written as.
    pub fn word(self) -> &'static str {
        match self {
            Does::Add => "add",
            Does::Remove => "remove",
        }
    }
}

/// `<root>/links.json`.
pub fn path(root: &Path) -> PathBuf {
    root.join(LINKS_FILE)
}

/// Read every link, sorted by `(day, line, name)`.
///
/// **ESTABLISHED** shape (`0x101a50a6c`): read the file, parse, sort.
pub fn read(root: &Path) -> Result<Vec<LogLink>, String> {
    let path = path(root);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let v = textfmt::parse_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut links = Vec::new();
    if let Some(Value::Arr(items)) = v.get("links") {
        for item in items {
            links.push(LogLink {
                name: item.str_of("name"),
                day: item.str_of("day"),
                line: item.u64_of("line").unwrap_or(0),
            });
        }
    }
    links.sort_by(|a, b| (&a.day, a.line, &a.name).cmp(&(&b.day, b.line, &b.name)));
    Ok(links)
}

/// Write the whole list.
fn write_all(root: &Path, links: &[LogLink]) -> Result<(), String> {
    let items: Vec<Value> = links
        .iter()
        .map(|l| {
            Value::Obj(vec![
                ("name".to_string(), Value::Str(l.name.clone())),
                ("day".to_string(), Value::Str(l.day.clone())),
                ("line".to_string(), Value::Num(l.line as f64)),
            ])
        })
        .collect();
    let doc = Value::Obj(vec![
        ("schema".to_string(), Value::Num(SCHEMA_VERSION as f64)),
        ("links".to_string(), Value::Arr(items)),
    ]);
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    crate::store::save_bytes(root, &path(root), textfmt::to_json(&doc).as_bytes())
        .map_err(|e| format!("{}: {e}", path(root).display()))
}

/// Add a link (replacing an identical one).
///
/// **ESTABLISHED** shape (`0x101a50418`): read, insert, write.
pub fn add(root: &Path, link: LogLink) -> Result<(), String> {
    let mut links = read(root)?;
    match links.iter_mut().find(|l| l.id() == link.id()) {
        Some(slot) => *slot = link,
        None => links.push(link),
    }
    write_all(root, &links)
}

/// Add a link only if it is not there yet.  Returns whether it was added.
///
/// **ESTABLISHED** shape (`0x101a50878`).
pub fn once(root: &Path, link: LogLink) -> Result<bool, String> {
    let mut links = read(root)?;
    if links.iter().any(|l| l.id() == link.id()) {
        return Ok(false);
    }
    let link_id = link.id();
    links.push(link);
    links.sort_by(|a, b| (&a.day, a.line, &a.name).cmp(&(&b.day, b.line, &b.name)));
    write_all(root, &links)?;
    let _ = link_id;
    Ok(true)
}

/// Record that `name` was written to `day` at `line`.
///
/// **ESTABLISHED** (`0x101a50bc0`): builds the three-part id and appends it;
/// the log writer calls this for every line it writes.
pub fn written(root: &Path, name: &str, day: &str, line: u64) -> Result<(), String> {
    let link = LogLink::new(name, day, line);
    let _ = link.id();
    once(root, link).map(|_| ())
}

/// Drop a link; returns whether it was there.
pub fn remove(root: &Path, link: &LogLink) -> Result<bool, String> {
    let mut links = read(root)?;
    let before = links.len();
    links.retain(|l| l.id() != link.id());
    let removed = links.len() != before;
    if removed {
        write_all(root, &links)?;
    }
    Ok(removed)
}

/// Apply a [`Does`] to a link.
pub fn apply(root: &Path, does: Does, link: LogLink) -> Result<bool, String> {
    match does {
        Does::Add => once(root, link),
        Does::Remove => remove(root, &link),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_is_the_three_parts_with_nothing_between() {
        let l = LogLink::new("cart", "2024-05-01", 12);
        assert_eq!(l.id(), "cart2024-05-0112");
    }

    #[test]
    fn does_maps_words() {
        assert_eq!(Does::of("link"), Some(Does::Add));
        assert_eq!(Does::of("REMOVE"), Some(Does::Remove));
        assert_eq!(Does::of("maybe"), None);
        assert_eq!(Does::Add.word(), "add");
    }

    #[test]
    fn links_round_trip_and_sort() {
        let root = std::env::temp_dir().join(format!("hydra-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(read(&root).unwrap(), Vec::new());
        assert!(once(&root, LogLink::new("b", "2024-05-02", 3)).unwrap());
        assert!(!once(&root, LogLink::new("b", "2024-05-02", 3)).unwrap());
        add(&root, LogLink::new("a", "2024-05-01", 9)).unwrap();
        written(&root, "c", "2024-05-01", 10).unwrap();
        let links = read(&root).unwrap();
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].name, "a");
        assert_eq!(links[1].name, "c");
        assert_eq!(links[2].name, "b");
        assert!(remove(&root, &LogLink::new("a", "2024-05-01", 9)).unwrap());
        assert!(!remove(&root, &LogLink::new("a", "2024-05-01", 9)).unwrap());
        assert_eq!(read(&root).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_dispatches_on_does() {
        let root = std::env::temp_dir().join(format!("hydra-links2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(apply(&root, Does::Add, LogLink::new("x", "2024-05-01", 1)).unwrap());
        assert!(!apply(&root, Does::Add, LogLink::new("x", "2024-05-01", 1)).unwrap());
        assert!(apply(&root, Does::Remove, LogLink::new("x", "2024-05-01", 1)).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }
}
