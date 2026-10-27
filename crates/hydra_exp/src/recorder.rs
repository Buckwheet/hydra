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
/// `hydra_ledger`'s writer; the two halves are a trait here so a caller can
/// supply its own, and [`crate::ledger::LedgerSink`] is the real implementation
/// over `hydra_ledger` (see `NOTES.md`).
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
/// It finishes by handing the loop to `tokio` — see [`spawn`], which is that
/// part on its own — and returns without waiting for it.
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
            .label
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

/// Hand the recording loop to the runtime, exactly as the original did.
///
/// **ESTABLISHED** from `0x1012fb4b4` itself, not from a type name alone:
/// after building the `Tracker` and the ledger `Reader`, `recorder::start`
/// reaches
///
/// ```text
/// 1012fb7e8  bl  0x1012dccf0   tokio::task::spawn::spawn
/// 1012fb7ec  mov x19, x0       ; the JoinHandle
/// 1012fb7f0  bl  0x101e69b90   tokio::runtime::task::state::State::drop_join_handle_fast
/// 1012fb7f8  bl  0x101e6b85c   tokio::runtime::task::raw::RawTask::drop_join_handle_slow
/// ```
///
/// so the spawned future is fire-and-forget: the handle is dropped
/// immediately and nothing is awaited.  The future's own type name,
/// `hydra_exp::recorder::start::{{closure}}`, is the async block, and
/// `hydra_exp::recorder::run::{{closure}}` is the `async fn run` it drives.
///
/// This reconstruction keeps [`start`] — the synchronous, `&mut dyn Sink`
/// core, which is this crate's public API — and exposes the spawn here so the
/// tokio dependency is real rather than a comment.  `who` and `at` are owned
/// because a spawned future must be `'static`; the original cloned the
/// character's two strings for the same reason.
pub fn spawn(sink: Box<dyn Sink + Send>, who: Who, at: i64) {
    let handle = tokio::spawn(async move {
        run(sink, who, at).await;
    });
    drop(handle);
}

/// The body of the spawned task (`hydra_exp::recorder::run::{{closure}}`).
///
/// `// UNKNOWN:` the loop itself — the binary's `run` is not isolatable (its
/// CGU holds the surrounding `INSERT`s), so this performs one [`start`] pass
/// and discards the result, which is what dropping the `JoinHandle` does with
/// it in the original.
async fn run(mut sink: Box<dyn Sink + Send>, who: Who, at: i64) {
    let _ = start(&mut *sink, &who, at);
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
                    label: Some("resource".to_string()),
                    at: 1_784_567_152,
                    kind: 1,
                    ..Snapshot::default()
                },
                Snapshot {
                    value: Some(1_234),
                    label: Some("Voln Favor: 1234".to_string()),
                    at: 1_784_567_152,
                    kind: 0,
                    ..Snapshot::default()
                },
            ],
            written: RefCell::new(Vec::new()),
        };
        let who = Who::new("Sindar");
        assert_eq!(start(&mut sink, &who, 1_784_567_152).unwrap(), 1);
        let written = sink.written.borrow();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0.value, Some(1_234));
        assert_eq!(written[0].1, "Sindar");
    }

    /// `tokio` is a real dependency of this crate: the binary's
    /// `recorder::start` calls `tokio::task::spawn::spawn` and drops the
    /// handle.  This pins that the spawn actually runs the recording pass on
    /// the runtime, and that nothing is awaited.
    #[test]
    fn spawn_hands_the_pass_to_the_runtime_and_drops_the_handle() {
        use std::sync::{Arc, Mutex};

        struct Shared(Arc<Mutex<Vec<(String, Option<i64>)>>>);

        impl Sink for Shared {
            fn latest_snapshots(&self) -> Result<Vec<Snapshot>, String> {
                Ok(vec![
                    Snapshot {
                        value: Some(4_242),
                        label: Some("Voln Favor: 4242".to_string()),
                        at: 7,
                        kind: 0,
                        ..Snapshot::default()
                    },
                    Snapshot {
                        label: Some("Health".to_string()),
                        at: 7,
                        kind: 2,
                        ..Snapshot::default()
                    },
                ])
            }
            fn record(&mut self, snap: &Snapshot, who: &Who) -> Result<(), String> {
                self.0
                    .lock()
                    .unwrap()
                    .push((who.name.clone(), snap.value));
                Ok(())
            }
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Box::new(Shared(Arc::clone(&seen)));
        let watch = Arc::clone(&seen);
        rt.block_on(async move {
            spawn(sink, Who::new("Sindar"), 1_784_567_152);
            // The handle was dropped, so nothing awaits the task; a
            // single-threaded runtime runs it as soon as this task yields.
            for _ in 0..10_000 {
                if !watch.lock().unwrap().is_empty() {
                    return;
                }
                tokio::task::yield_now().await;
            }
            panic!("the spawned recorder task never ran");
        });
        // Only the snapshot `names_a_snapshot` accepts is recorded.
        assert_eq!(
            *seen.lock().unwrap(),
            vec![("Sindar".to_string(), Some(4_242))]
        );
    }
}
