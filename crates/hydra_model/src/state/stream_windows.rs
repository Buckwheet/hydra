//! Stream-window bookkeeping.
//!
//! `Windows::declare` (256 B) calls `Closed::read` (388 B); the recovered
//! window names come from the protocol's `<pushStream id=...>` tags.

use std::collections::BTreeMap;

/// A window's open/closed state.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Windows {
    pub open: BTreeMap<String, bool>,
}

/// A `closeStream`/`clearStream` marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closed {
    pub id: String,
    pub clear: bool,
}

impl Closed {
    /// @0x0000000101bec?? (388 B) — `<closeStream id="X"/>` /
    /// `<clearStream id="X"/>`.
    pub fn read(line: &str) -> Option<Closed> {
        let (id, clear) = if let Some((_, rest)) = line.split_once("clearStream") {
            (rest, true)
        } else if let Some((_, rest)) = line.split_once("closeStream") {
            (rest, false)
        } else {
            return None;
        };
        let id = attribute(id, "id")?;
        Some(Closed { id, clear })
    }
}

impl Windows {
    /// @0x0000000101becc28 (256 B) — apply a stream command.
    pub fn declare(&mut self, line: &str) -> bool {
        if let Some(closed) = Closed::read(line) {
            let previous = self.open.insert(closed.id, false);
            return previous != Some(false);
        }
        if let Some((_, rest)) = line.split_once("pushStream") {
            if let Some(id) = attribute(rest, "id") {
                let previous = self.open.insert(id, true);
                return previous != Some(true);
            }
        }
        false
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.open.get(id).copied().unwrap_or(false)
    }
}

fn attribute(text: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = text.find(&needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_close_track_the_window() {
        let mut windows = Windows::default();
        assert!(windows.declare("<pushStream id=\"thoughts\"/>"));
        assert!(windows.is_open("thoughts"));
        assert!(!windows.declare("<pushStream id=\"thoughts\"/>"));
        assert!(windows.declare("<closeStream id=\"thoughts\"/>"));
        assert!(!windows.is_open("thoughts"));
    }

    #[test]
    fn closed_parsing_reads_the_id() {
        let closed = Closed::read("<clearStream id=\"inv\"/>").unwrap();
        assert_eq!(closed.id, "inv");
        assert!(closed.clear);
        assert!(Closed::read("<pushStream id=\"x\"/>").is_none());
    }

    #[test]
    fn unrelated_lines_change_nothing() {
        let mut windows = Windows::default();
        assert!(!windows.declare("You stand still."));
    }
}
