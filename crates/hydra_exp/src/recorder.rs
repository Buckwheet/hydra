//! `hydra_exp::recorder` — folding the game's own experience output into the
//! ledger.
//!
//! Two entry points: [`names_a_snapshot`] (`0x1012fb41c`, 152 B, **full**) is the
//! filter that decides whether a line the game printed is one of the snapshot
//! lines worth recording, and [`start`] (`0x1012fb4b4`) walks the ledger's
//! snapshots and records the new ones.

use crate::row::{Snapshot, Who};

/// `recorder::names_a_snapshot(name: &str) -> bool` — `@0x1012fb41c` (**full**).
///
/// The body is three tests, in this order:
///
/// 1. `name.starts_with("Voln Favor:")` — an eleven-byte literal at `0x10240dbe6`
///    compared with the outlined `str::starts_with` at `0x1012f9d28`;
/// 2. `name.starts_with("(Weekly)Suffused")` — the eight-byte literal at
///    `0x10240dbf1`;
/// 3. `name.len() >= 9 && name[..8] == "Suffused" && name.as_bytes()[8] == b' '`
///    — the qword `0x6465737566667553` and `cmp w9, #0x20`.
///
/// So `"Suffused Essence"` counts, `"Suffused"` does not (too short), and
/// `"Suffused"` with anything but a space at index 8 does not either.
pub fn names_a_snapshot(name: &str) -> bool {
    if name.starts_with("Voln Favor:") || name.starts_with("(Weekly)Suffused") {
        return true;
    }
    let bytes = name.as_bytes();
    bytes.len() >= 9 && &bytes[..8] == b"Suffused" && bytes[8] == b' '
}

/// Where [`start`] reads snapshots from and writes them to.
///
/// The original called `hydra_ledger::Ledger::reader()` and
/// `Reader::latest_snapshots()` (`0x10168c174`) and then wrote through
/// `hydra_ledger`'s writer; that crate is still a stub in this workspace, so the
/// two halves are a trait here (see `NOTES.md`).
pub trait Sink {
    /// The most recent snapshot of every kind (`Reader::latest_snapshots`).
    fn latest_snapshots(&self) -> Result<Vec<Snapshot>, String>;
    /// Store one snapshot row for `who`.
    fn record(&mut self, snap: &Snapshot, who: &Who) -> Result<(), String>;
}

/// `recorder::start(sink, who, at) -> Result<usize, String>` —
/// `@0x1012fb4b4` (**partly**).
///
/// The recovered prologue: clone the two `String`s of the character argument
/// (`+0x00` and `+0x18` of the third argument), open the ledger reader, and bail
/// out with the `rusqlite` `Display` text when the connection failed (tag `3`,
/// `drop_in_place<rusqlite::error::Error>` at `0x1012fa470`). It then walks
/// `latest_snapshots()` (stride `0x90`), cloning the `Option<String>` at `+0x60`
/// and reading the kind byte at `+0x88` for every row.
///
/// `// UNKNOWN:` the exact write. The 45 KB body contains the SQL of several
/// other crates' tables (`attacks`, `hits`, `flares`, `creatures`), so the
/// `INSERT` this function itself uses could not be isolated; this build records
/// every snapshot whose label [`names_a_snapshot`] accepts and reports how many
/// rows were written.
pub fn start(sink: &mut dyn Sink, who: &Who, at: i64) -> Result<usize, String> {
    let snaps = sink.latest_snapshots()?;
    let mut written = 0usize;
    for snap in &snaps {
        let named = snap
            .text
            .as_deref()
            .map(names_a_snapshot)
            .unwrap_or(false);
        if !named {
            continue;
        }
        let row = Snapshot {
            at: if snap.at == 0 { at } else { snap.at },
            ..snap.clone()
        };
        sink.record(&row, who)?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn snapshot_names_match_the_binary() {
        assert!(names_a_snapshot("Voln Favor: 1234"));
        assert!(names_a_snapshot("Voln Favor:"));
        assert!(names_a_snapshot("(Weekly)Suffused 5000"));
        assert!(names_a_snapshot("Suffused Essence: 5000"));
        // Not enough bytes for the qword compare.
        assert!(!names_a_snapshot("Suffused"));
        // Eight bytes but the ninth is not a space.
        assert!(!names_a_snapshot("Suffused, 5000"));
        assert!(!names_a_snapshot("suffused Essence: 5000"));
        assert!(!names_a_snapshot(""));
        assert!(!names_a_snapshot("Health: 100"));
    }

    struct Fake {
        snaps: Vec<Snapshot>,
        written: RefCell<Vec<(Snapshot, String)>>,
    }

    impl Sink for Fake {
        fn latest_snapshots(&self) -> Result<Vec<Snapshot>, String> {
            Ok(self.snaps.clone())
        }
        fn record(&mut self, snap: &Snapshot, who: &Who) -> Result<(), String> {
            self.written
                .borrow_mut()
                .push((snap.clone(), who.name.clone()));
            Ok(())
        }
    }

    #[test]
    fn start_records_only_the_named_snapshots() {
        let mut sink = Fake {
            snaps: vec![
                Snapshot {
                    weekly: Some(12_000),
                    total: Some(240_000),
                    text: Some("resource".to_string()),
                    at: 1_784_567_152,
                },
                Snapshot {
                    weekly: Some(1_234),
                    total: None,
                    text: Some("Voln Favor: 1234".to_string()),
                    at: 1_784_567_152,
                },
            ],
            written: RefCell::new(Vec::new()),
        };
        let who = Who::new("Sindar");
        assert_eq!(start(&mut sink, &who, 1_784_567_152).unwrap(), 1);
        let written = sink.written.borrow();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0.weekly, Some(1_234));
        assert_eq!(written[0].1, "Sindar");
    }
}
