//! The recorder thread.
//!
//! `task::start` (0x101390df4) opens a recorder, spawns a thread named
//! `hydra-combat-log` (`std::thread::Builder::name`, 0x102415100) with
//! `RUST_MIN_STACK` set, and drives a loop over the session's events: every
//! combat line is handed to `Recorder::record`, and the loop ends when the
//! session does.  Progress is reported through a bounded channel of [`Sent`].
//!
//! `// UNKNOWN:` the original runs that loop on a `tokio` runtime (`start` builds
//! one, `listen` is an async closure over a `tokio::sync::broadcast::Receiver`,
//! and `Sent` travels over a `tokio::sync::mpsc::Sender` with a
//! `batch_semaphore`).  `tokio` is not available here, so the loop is a plain
//! thread over a `std::sync::mpsc::Receiver`; the messages it reports are the
//! recovered ones (`combat log thread: {}`, 0x103e69109).

use crate::recorder::{Recorder, Who};
use crate::store::Store;
use crate::Error;
use std::sync::mpsc::Receiver;

/// The thread name the original gives the recorder (`0x102415100`,
/// `hydra-combat-log`).
pub const THREAD_NAME: &str = "hydra-combat-log";

/// What the recorder thread reports back.
///
/// `task::Sent` travels over the bounded channel in the binary; the payload it
/// carries is not visible past the channel implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sent {
    /// One line to record, with its Eastern-time stamp.
    Line { at: i64, text: String },
    /// The session ended; the recorder closes at this stamp.
    Ended { at: i64 },
}

/// What the recorder thread produced.
#[derive(Debug)]
pub struct Finished {
    /// The lines the thread reported, in order.
    pub reported: Vec<String>,
    /// The store the session was recorded into.
    pub store: Store,
}

/// Run the record loop on its own thread.
///
/// The recovered shape: `start` opens the recorder (its failure is returned),
/// spawns `THREAD_NAME`, and formats `combat log thread: {}` for every line it
/// cannot record — the text of that line is the message.  The thread ends when
/// the channel closes or an [`Sent::Ended`] arrives.
pub fn start(
    store: Store,
    who: Who,
    opened_at: i64,
    events: Receiver<Sent>,
) -> std::io::Result<std::thread::JoinHandle<Result<Finished, Error>>> {
    std::thread::Builder::new()
        .name(THREAD_NAME.to_string())
        .spawn(move || run(store, who, opened_at, events))
}

/// The body of the recorder thread.
pub fn run(
    store: Store,
    who: Who,
    opened_at: i64,
    events: Receiver<Sent>,
) -> Result<Finished, Error> {
    const REPORT: &str = "combat log thread: {}";
    let mut recorder = Recorder::open(store, who, opened_at)?;
    let mut reported = Vec::new();
    let mut ended = None;
    for event in events {
        match event {
            Sent::Line { at, text } => {
                if let Err(e) = recorder.record(&text, at) {
                    reported.push(REPORT.replace("{}", e.message()));
                }
            }
            Sent::Ended { at } => {
                ended = Some(at);
                break;
            }
        }
    }
    let at = ended.unwrap_or(recorder.hunt_id());
    recorder.sync_effects(at);
    let store = recorder.close(at)?;
    Ok(Finished { reported, store })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn thread_name_is_recovered() {
        assert_eq!(THREAD_NAME, "hydra-combat-log");
    }

    #[test]
    fn lines_are_recorded_and_the_session_closed() {
        let (tx, rx) = channel();
        let who = Who {
            instance: 1,
            character: String::from("Testchar"),
        };
        let handle = start(Store::new(), who, 1_000_000, rx).expect("spawns");
        tx.send(Sent::Line {
            at: 1_000_010,
            text: String::from("Testchar jabs a kobold for 5 points of damage"),
        })
        .unwrap();
        tx.send(Sent::Ended { at: 1_000_100 }).unwrap();
        drop(tx);
        let finished = handle.join().expect("joins").expect("ran");
        assert!(finished.reported.is_empty());
        let hunt = finished.store.hunt(1).expect("hunt");
        assert_eq!(hunt.ended_at, Some(1_000_100));
        assert_eq!(finished.store.attacks.len(), 1);
        assert_eq!(finished.store.hits.len(), 1);
    }
}
