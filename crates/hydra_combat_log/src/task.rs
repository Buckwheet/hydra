//! The recorder thread and its async loop.
//!
//! `task::start` (0x101390df4) opens a recorder, spawns a thread named
//! `hydra-combat-log` (`std::thread::Builder::name`, 0x102415100) with
//! `RUST_MIN_STACK` set, and drives a loop over the session's events: every
//! combat line is handed to `Recorder::record`, and the loop ends when the
//! session does.  Progress is reported through a bounded channel of [`Sent`].
//!
//! **Phase 1: `tokio` is back.** The recovered shape is an async loop —
//! `start` builds a runtime, `listen` is an async closure over a
//! `tokio::sync::broadcast::Receiver`, and `Sent` travels over a
//! `tokio::sync::mpsc::Sender` with a `batch_semaphore` behind it:
//!
//! * [`listen`] is that async loop (`mpsc::Receiver<Sent>` in, a closed
//!   [`Finished`] out).
//! * [`relay`] is the half of `listen`'s closure that feeds it: it follows a
//!   real `hydra_session::command::Event` broadcast channel and turns the game's
//!   frames and the session's end into [`Sent`] messages.
//! * [`run`] and [`start`] keep their **synchronous** public shape — `start`
//!   spawns the recovered thread name and builds a current-thread runtime in it,
//!   `run` does the same inline — so callers that are not async (and the
//!   crate's own tests) are unaffected.

use crate::recorder::{Recorder, Who};
use crate::store::Store;
use crate::Error;
use tokio::sync::{broadcast, mpsc};

/// The thread name the original gives the recorder (`0x102415100`,
/// `hydra-combat-log`).
pub const THREAD_NAME: &str = "hydra-combat-log";

/// The bounded channel's depth for [`Sent`].
///
/// `// UNKNOWN:` the original's channel capacity is not visible past
/// `tokio::sync::mpsc::channel` (the `batch_semaphore` construction the loop's
/// back-pressure uses); the ledger's own writer queue is 1024
/// (`hydra_ledger::Ledger::writer`), so the recorder's is the same.
pub const SENT_QUEUE: usize = 1024;

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

/// The async record loop — the recovered `listen::{{closure}}`.
///
/// Opens the recorder (its failure is the returned error), then takes [`Sent`]
/// messages **in order** off the channel until it closes or an
/// [`Sent::Ended`] arrives, handing every line to `Recorder::record` and
/// formatting `combat log thread: {}` for the ones it cannot record (0x103e69109)
/// — the text of that line is the message.
pub async fn listen(
    store: Store,
    who: Who,
    opened_at: i64,
    mut events: mpsc::Receiver<Sent>,
) -> Result<Finished, Error> {
    const REPORT: &str = "combat log thread: {}";
    let mut recorder = Recorder::open(store, who, opened_at)?;
    let mut reported = Vec::new();
    let mut ended = None;
    while let Some(event) = events.recv().await {
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
    recorder.sync_effects(at)?;
    let store = recorder.close(at)?;
    Ok(Finished { reported, store })
}

/// Follow a session's event channel and push what it says onto the recorder's
/// queue — the recovered `listen` closure's other half.
///
/// The session's channel is the real `hydra_session::command::Event`
/// `broadcast::Sender`/`Receiver` pair (`SessionHandle::watch`, 0x101a3bd…);
/// `Event::Frames` are the game's lines, so each one becomes an [`Sent::Line`]
/// stamped with `hydra_net::eastern::now()`, and `Event::Over` ends the session
/// ([`Sent::Ended`]).  Events the combat log has nothing to do with (`Said`,
/// `Typed`, the symbol changes) are dropped: the recorder only parses game
/// lines.
///
/// Returns when the channel is closed (`Closed`) or the session ends; a
/// `Lagged` receiver keeps going, which is what a recording wants — a dropped
/// frame is better than a broken recorder.
pub async fn relay(
    mut events: broadcast::Receiver<hydra_session::command::Event>,
    sent: mpsc::Sender<Sent>,
) -> Result<(), Error> {
    loop {
        match events.recv().await {
            Ok(hydra_session::command::Event::Frames(lines)) => {
                for text in lines {
                    let at = crate::eastern::now();
                    if sent.send(Sent::Line { at, text }).await.is_err() {
                        return Ok(());
                    }
                }
            }
            Ok(hydra_session::command::Event::Over(_)) => {
                let _ = sent.send(Sent::Ended { at: crate::eastern::now() }).await;
                return Ok(());
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return Ok(()),
        }
    }
}

/// The runtime kind [`start`] and [`run`] build.
///
/// `// UNKNOWN:` `start`'s runtime flavour is not visible (the binary's
/// `Builder::new_*` call is inlined); a current-thread runtime is enough for a
/// loop that only awaits one channel, and keeps the recorder thread from adding
/// worker threads of its own.
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// Run the record loop synchronously, on a runtime of its own.
///
/// The body of the recorder thread: [`listen`] on a current-thread runtime.
pub fn run(
    store: Store,
    who: Who,
    opened_at: i64,
    events: mpsc::Receiver<Sent>,
) -> Result<Finished, Error> {
    let runtime = runtime().map_err(|e| Error::new(e.to_string()))?;
    runtime.block_on(listen(store, who, opened_at, events))
}

/// Run the record loop on its own thread.
///
/// The recovered shape: `start` opens the recorder (its failure is returned),
/// spawns `THREAD_NAME`, and formats `combat log thread: {}` for every line it
/// cannot record.  The thread ends when the channel closes or an
/// [`Sent::Ended`] arrives.
pub fn start(
    store: Store,
    who: Who,
    opened_at: i64,
    events: mpsc::Receiver<Sent>,
) -> std::io::Result<std::thread::JoinHandle<Result<Finished, Error>>> {
    std::thread::Builder::new()
        .name(THREAD_NAME.to_string())
        .spawn(move || run(store, who, opened_at, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_session::claimant::Desk;

    fn who() -> Who {
        Who {
            instance: 1,
            character: String::from("Testchar"),
        }
    }

    #[test]
    fn thread_name_is_recovered() {
        assert_eq!(THREAD_NAME, "hydra-combat-log");
    }

    #[test]
    fn lines_are_recorded_and_the_session_closed() {
        let (tx, rx) = mpsc::channel(SENT_QUEUE);
        let handle = start(Store::new(), who(), 1_000_000, rx).expect("spawns");
        // `try_send` would race with the thread's own `recv`; blocking sends are
        // what the caller of `start` does.
        let sender = tx.clone();
        std::thread::spawn(move || {
            let runtime = runtime().expect("runtime");
            runtime.block_on(async move {
                sender
                    .send(Sent::Line {
                        at: 1_000_010,
                        text: String::from("Testchar jabs a kobold for 5 points of damage"),
                    })
                    .await
                    .unwrap();
                sender.send(Sent::Ended { at: 1_000_100 }).await.unwrap();
            });
        });
        drop(tx);
        let finished = handle.join().expect("joins").expect("ran");
        assert!(finished.reported.is_empty());
        let hunt = finished.store.hunt(1).expect("query").expect("hunt");
        assert_eq!(hunt.ended_at, Some(1_000_100));
        assert_eq!(finished.store.attacks_of(1).expect("query").len(), 1);
        assert_eq!(finished.store.hits_of_hunt(1).expect("query").len(), 1);
    }

    #[test]
    fn the_queue_keeps_the_order_the_lines_arrive_in() {
        let (tx, rx) = mpsc::channel(SENT_QUEUE);
        let runtime = runtime().expect("runtime");
        const LINES: usize = 24;
        let handle = std::thread::spawn(move || run(Store::new(), who(), 1_000_000, rx));
        runtime.block_on(async move {
            for n in 0..LINES {
                tx.send(Sent::Line {
                    at: 1_000_100 + n as i64,
                    text: format!("Testchar jabs a kobold for {} points of damage", n + 1),
                })
                .await
                .unwrap();
            }
            tx.send(Sent::Ended { at: 1_000_900 }).await.unwrap();
        });
        let finished = handle.join().expect("joins").expect("ran");
        let attacks = finished.store.attacks_of(1).expect("query");
        let hits = finished.store.hits_of_hunt(1).expect("query");
        assert_eq!(attacks.len(), LINES);
        assert_eq!(hits.len(), LINES);
        // Same order as sent: the sequence numbers and the damage both climb.
        for (n, (attack, hit)) in attacks.iter().zip(&hits).enumerate() {
            assert_eq!(attack.seq, n as i64 + 1);
            assert_eq!(hit.damage, n as i64 + 1);
            assert_eq!(hit.attack_id, attack.id);
        }
    }

    #[test]
    fn a_session_relay_feeds_the_recorder_through_a_broadcast_channel() {
        let session = hydra_session::command::SessionHandle::new(Desk::new(None, "Testchar"));
        let events = session.watch();
        let (tx, rx) = mpsc::channel(SENT_QUEUE);
        let runtime = runtime().expect("runtime");
        let handle = std::thread::spawn(move || run(Store::new(), who(), 1_000_000, rx));
        let relay_session = session.clone();
        runtime.block_on(async move {
            let relay = tokio::spawn(relay(events, tx));
            relay_session.fold_in(vec![
                String::from("Testchar jabs a kobold for 5 points of damage"),
                String::from("a kobold jabs you for 7 points of damage"),
            ]);
            relay_session.stop("the session ended");
            relay.await.expect("relay joins").expect("relay ran");
        });
        let finished = handle.join().expect("joins").expect("ran");
        assert!(finished.reported.is_empty());
        let attacks = finished.store.attacks_of(1).expect("query");
        assert_eq!(attacks.len(), 2);
        // The recorder keeps the line's own verb spelling (`jabs`), as before.
        assert_eq!(attacks[0].name, "jabs");
        assert!(attacks[0].ours);
        assert!(attacks[1].inbound);
        // The relay stamped both lines from the Eastern clock and closed the
        // session when the channel said it was over.
        assert!(attacks[0].occurred_at >= 1_577_836_800);
        let hunt = finished.store.hunt(1).expect("query").expect("hunt");
        assert!(hunt.ended_at.is_some());
    }
}
