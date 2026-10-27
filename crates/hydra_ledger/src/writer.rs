//! `hydra_ledger::writer` — the write half of the ledger.
//!
//! The original `Ledger::writer` (`0x10168e9e0`) opens a read-write connection
//! and hands it to a dedicated thread named `hydra-ledger`, which owns the
//! database from then on; the handle it returns only has a
//! `tokio::sync::mpsc::Sender<Sent>` in it. `Writer::try_record`
//! (`0x10169398c`) is the non-blocking door — it copies the 208-byte [`Row`]
//! into a [`Sent`] and `try_send`s it — and `Writer::{record, flush}` are the
//! two `async fn`s (their `{{closure}}` symbols are in `nm -an`).
//!
//! **Reconnected in Phase 1**: the plumbing is `tokio` again, at the workspace
//! pin (1.53.1, all features):
//!
//! | recovered | this crate |
//! |---|---|
//! | `tokio::sync::mpsc::channel(1024)` + `try_send` | the same, [`QUEUE_CAPACITY`] slots |
//! | `tokio::sync::oneshot` flush ack | [`Sent::Flush`] carries one |
//! | `async fn record` / `async fn flush` | [`Writer::record_async`] / [`Writer::flush_async`], with blocking [`Writer::record`] / [`Writer::flush`] wrappers on the crate's runtime |
//! | `std::thread::Builder::spawn_unchecked` of the writer task | `std::thread::Builder::name("hydra-ledger")` of a thread that drives [`run_async`] on the crate's `tokio` runtime, so `format!("ledger thread: {io_error}")` survives |
//!
//! `run_async` keeps the original's shape: drain what is queued, write it as
//! one transaction, `COMMIT`, then answer the flushes that arrived with it.

use std::fmt;
use std::sync::Mutex;
use std::thread::JoinHandle;

use tokio::sync::mpsc::{channel, error::TrySendError, Receiver, Sender};
use tokio::sync::oneshot;

use crate::row::Row;
use crate::schema;
use crate::store::Connection;
use crate::{runtime, Error, Result};

/// How many rows the queue holds — `Semaphore::new(1024)` and
/// `stur x8(#0x400)` in `Ledger::writer` (`0x10168ea88`/`0x10168ea94`).
pub const QUEUE_CAPACITY: usize = 1024;

/// What the writer thread is told to do.
///
/// The binary's `Sent` is a 208-byte enum with the same width as [`Row`] (its
/// `drop_in_place` at `0x101699634` tests the first word against
/// `0x8000_0000_0000_0003` before dropping a sender from `+0x8`), i.e. one row
/// variant and one "flush/stop, here is how to answer" variant.
#[derive(Debug)]
pub enum Sent {
    /// Write this row.
    Row(Row),
    /// Write everything queued, commit, then answer.
    Flush(oneshot::Sender<()>),
}

/// The writer's "the ledger is gone" error — `<hydra_ledger::writer::Closed as
/// core::fmt::Display>::fmt` at `0x1016946f8`, one 43-byte literal at
/// `0x1039861b2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Closed;

impl fmt::Display for Closed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(crate::CLOSED_TEXT)
    }
}

impl std::error::Error for Closed {}

/// The write handle: a bounded `tokio` queue into the writer thread.
#[derive(Debug)]
pub struct Writer {
    tx: Option<Sender<Sent>>,
    thread: Option<JoinHandle<()>>,
    /// How many rows `try_record` could not queue because the queue was full.
    dropped: Mutex<usize>,
}

impl Writer {
    /// `Ledger::writer(&self) -> Result<Writer>` — `@0x10168e9e0`
    /// (**full for the setup**).
    ///
    /// `connect(false)`, a 1024-slot `tokio::sync::mpsc::channel`,
    /// `thread::Builder::new().name("hydra-ledger")` around [`run`], and — when
    /// the thread cannot be started — `format!("ledger thread: {}", io_error)`
    /// (blob `0x103e6e5d5`) as the `Err` text.
    pub fn start(conn: Connection) -> Result<Writer> {
        let (tx, rx) = channel(QUEUE_CAPACITY);
        let thread = std::thread::Builder::new()
            .name(schema::WRITER_THREAD.to_string())
            .spawn(move || run(conn, rx))
            .map_err(|error| Error::Message(format!("ledger thread: {error}")))?;
        Ok(Writer {
            tx: Some(tx),
            thread: Some(thread),
            dropped: Mutex::new(0),
        })
    }

    /// `Writer::try_record(&self, row: Row) -> bool` — **full**.
    ///
    /// `@0x10169398c` (3,436 bytes, most of it the `try_send` future and the
    /// `TrySendError` unwinding):
    ///
    /// 1. copy the 0xd0-byte argument (`ldp`/`stp` from `[x1]` into the stack
    ///    slot the message lives in);
    /// 2. `tokio::sync::mpsc::bounded::Sender::try_send` (`0x101699904`);
    /// 3. return `cmp x19, #0x2; cset w0, ne` — `false` only for the
    ///    **closed** arm, `true` for a queued row *and* for a full queue.
    ///
    /// So a `true` means "the writer is alive": a row dropped for want of queue
    /// space is not reported. This build keeps that rule exactly and counts the
    /// drops in [`Writer::dropped`].
    pub fn try_record(&self, row: Row) -> bool {
        match &self.tx {
            Some(tx) => match tx.try_send(Sent::Row(row)) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    *self.dropped.lock().unwrap() += 1;
                    true
                }
                Err(TrySendError::Closed(_)) => false,
            },
            None => false,
        }
    }

    /// The recovered `async fn record`
    /// (`hydra_ledger::writer::Writer::record::{{closure}}`), tokio-native
    /// again: `Sender::send(..).await`.
    ///
    /// Waits for queue space instead of dropping the row, which is the whole
    /// point of the async version. [`Writer::record`] is its blocking wrapper.
    pub async fn record_async(&self, row: Row) -> std::result::Result<(), Closed> {
        match &self.tx {
            Some(tx) => tx.send(Sent::Row(row)).await.map_err(|_| Closed),
            None => Err(Closed),
        }
    }

    /// [`Writer::record_async`] for synchronous callers: blocks the calling
    /// thread (on the crate's runtime) until the row is in the queue.
    pub fn record(&self, row: Row) -> std::result::Result<(), Closed> {
        runtime::block_on(self.record_async(row))
    }

    /// The recovered `async fn flush`
    /// (`hydra_ledger::writer::Writer::flush::{{closure}}`), tokio-native
    /// again: queue a marker, then await the `tokio::sync::oneshot` answer,
    /// which arrives only after everything before it has been written and
    /// committed. [`Writer::flush`] is its blocking wrapper.
    pub async fn flush_async(&self) -> std::result::Result<(), Closed> {
        let tx = self.tx.as_ref().ok_or(Closed)?;
        let (ack, wait) = oneshot::channel();
        tx.send(Sent::Flush(ack)).await.map_err(|_| Closed)?;
        wait.await.map_err(|_| Closed)
    }

    /// [`Writer::flush_async`] for synchronous callers: blocks the calling
    /// thread (on the crate's runtime) until the writer has committed.
    pub fn flush(&self) -> std::result::Result<(), Closed> {
        runtime::block_on(self.flush_async())
    }

    /// How many rows `try_record` has had to drop because the queue was full
    /// (the original drops them silently).
    pub fn dropped(&self) -> usize {
        *self.dropped.lock().unwrap()
    }

    /// Close the queue and wait for the thread to write what is left.
    ///
    /// The original gets this from `Writer`'s drop glue: the `Sender` closing
    /// ends `writer::run`'s `while let Some(sent) = rx.recv().await` loop
    /// (`drop_in_place<Writer>` at `0x10008c140` closes the channel and wakes
    /// the receiver).
    pub fn close(&mut self) {
        self.tx = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.close();
    }
}

/// `writer::run(conn, rx)` — the writer's body, on the crate's runtime.
///
/// The blocking form of [`run_async`], for callers that are not already inside
/// a runtime.
pub fn run(conn: Connection, rx: Receiver<Sent>) {
    runtime::runtime().block_on(run_async(conn, rx));
}

/// `writer::run(conn, rx)` — the writer task — **partly recovered**.
///
/// `@0x101692860` (4,396 bytes). What the instructions establish:
///
/// * it is the `spawn_unchecked` target of `Ledger::writer`, and it takes the
///   connection by value;
/// * it issues the three recovered `BEGIN …` texts and `COMMIT`
///   (`0x103986018`, `0x1039861dd`, `0x1039861e8`), i.e. **one transaction per
///   batch**;
/// * it issues the four `INSERT`s quoted in [`crate::schema`] —
///   `INSERT INTO pulses (…) VALUES (?1 … ?18)`, `INSERT INTO snapshots (…)
///   VALUES (?1 … ?9)`, the `login_rewards` upsert with its
///   `ON CONFLICT … WHERE login_rewards.reward = '' AND excluded.reward <> ''`
///   guard, and `INSERT INTO store_visits (…) VALUES (?1 … ?7)` — with the
///   per-column `ToSql` vtables in `__DATA` at `0x1044429d0`…`0x104442b68`;
/// * on a send failure it drops the connection and reports
///   [`crate::CLOSED_TEXT`].
///
/// The transaction is a `rusqlite::Transaction` (its `BEGIN DEFERRED` is the
/// recovered text), so a failed batch rolls back rather than leaving one open.
///
/// `// UNKNOWN:` the batching rule (how many rows per transaction — this build
/// takes everything already queued) and the `tokio::select!` structure around
/// the channel.
pub async fn run_async(conn: Connection, mut rx: Receiver<Sent>) {
    while let Some(first) = rx.recv().await {
        let mut batch = vec![first];
        // Drain what is already queued: that is the batch the original wraps in
        // one BEGIN … COMMIT.
        while let Ok(next) = rx.try_recv() {
            batch.push(next);
        }
        let mut rows = Vec::new();
        let mut waits = Vec::new();
        for sent in batch {
            match sent {
                Sent::Row(row) => rows.push(row),
                Sent::Flush(ack) => waits.push(ack),
            }
        }
        // One transaction per batch. A failed insert is a lost row, exactly as
        // a dropped `try_record` is: the writer never reports it.
        if !rows.is_empty() {
            if let Ok(tx) = conn.raw().unchecked_transaction() {
                for row in rows {
                    let _ = conn.insert_row_on(&tx, row);
                }
                let _ = tx.commit();
            }
        }
        for ack in waits {
            let _ = ack.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::{Pulse, Source, Who};
    use crate::Ledger;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("hydra-ledger-writer-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn pulse(at: i64) -> Pulse {
        Pulse {
            instance: "prime".to_string(),
            character: "Sindar".to_string(),
            at,
            exp_gained: 100,
            source: Source::Normal,
            ..Pulse::default()
        }
    }

    #[test]
    fn the_closed_text_is_the_recovered_literal() {
        assert_eq!(
            Closed.to_string(),
            "the ledger is not taking rows, or lost some"
        );
        assert_eq!(QUEUE_CAPACITY, 1024);
        assert_eq!(schema::WRITER_THREAD, "hydra-ledger");
    }

    #[test]
    fn rows_reach_the_database_once_the_writer_flushes() {
        let dir = scratch("flush");
        // The writer writes into a ledger `Ledger::open` has already migrated.
        let ledger = Ledger::open(&dir).unwrap();
        {
            let writer = ledger.writer().unwrap();
            assert!(writer.try_record(Row::Pulse(pulse(10))));
            assert!(writer.try_record(Row::Pulse(pulse(20))));
            writer.flush().unwrap();
            assert_eq!(writer.dropped(), 0);
        }
        let reader = ledger.reader().unwrap();
        let rows = reader.last_pulses(&Who::new("prime", "Sindar"), 10).unwrap();
        assert_eq!(rows.iter().map(|p| p.at).collect::<Vec<_>>(), vec![20, 10]);
        drop(reader);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_batch_is_written_in_one_transaction_and_answered_in_order() {
        let dir = scratch("batch");
        let ledger = Ledger::open(&dir).unwrap();
        {
            let writer = ledger.writer().unwrap();
            for at in 1..=100 {
                writer.record(Row::Pulse(pulse(at))).unwrap();
            }
            // A flush behind a hundred rows answers only after all of them.
            writer.flush().unwrap();
            assert_eq!(writer.dropped(), 0);
        }
        let conn = ledger.connect(true).unwrap();
        let db = conn.db();
        assert_eq!(db.pulses.len(), 100);
        // `INTEGER PRIMARY KEY AUTOINCREMENT`, in insertion order.
        assert_eq!(db.pulses[0].id, 1);
        assert_eq!(db.pulses[99].id, 100);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropping_the_writer_closes_the_door() {
        let dir = scratch("closed");
        let ledger = Ledger::open(&dir).unwrap();
        let writer = ledger.writer().unwrap();
        drop(writer);
        // The `tokio` sender is gone, so the next write is the closed arm.
        let mut writer = ledger.writer().unwrap();
        writer.close();
        assert!(!writer.try_record(Row::Pulse(pulse(30))));
        assert_eq!(writer.record(Row::Pulse(pulse(40))), Err(Closed));
        assert_eq!(writer.flush(), Err(Closed));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_full_queue_still_reports_alive_and_counts_the_drops() {
        // A writer whose thread is not draining cannot be built through
        // `Ledger`, so exercise the queue directly with a capacity-1 channel
        // that nobody reads.
        let (tx, mut rx) = channel::<Sent>(1);
        let writer = Writer {
            tx: Some(tx),
            thread: None,
            dropped: Mutex::new(0),
        };
        assert!(writer.try_record(Row::Pulse(pulse(1))));
        assert!(writer.try_record(Row::Pulse(pulse(2))));
        assert!(writer.try_record(Row::Pulse(pulse(3))));
        assert_eq!(writer.dropped(), 2);
        let mut received = 0;
        while rx.try_recv().is_ok() {
            received += 1;
        }
        assert_eq!(received, 1);
    }

    /// The original's `async fn record`/`flush`, awaited instead of blocked on.
    #[tokio::test]
    async fn the_async_record_and_flush_drive_the_same_writer() {
        let dir = scratch("async");
        let ledger = Ledger::open(&dir).unwrap();
        {
            let writer = ledger.writer().unwrap();
            for at in [10, 20, 30] {
                writer.record_async(Row::Pulse(pulse(at))).await.unwrap();
            }
            writer.flush_async().await.unwrap();
        }
        // The rows are on disk, in a real SQLite file.
        let reader = ledger.reader().unwrap();
        let rows = reader.last_pulses(&Who::new("prime", "Sindar"), 10).unwrap();
        assert_eq!(
            rows.iter().map(|p| p.at).collect::<Vec<_>>(),
            vec![30, 20, 10]
        );
        drop(reader);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_row_kind_is_written_by_its_own_insert() {
        let dir = scratch("kinds");
        let ledger = Ledger::open(&dir).unwrap();
        {
            let writer = ledger.writer().unwrap();
            writer
                .record(Row::Pulse(Pulse {
                    instance: "prime".to_string(),
                    character: "Sindar".to_string(),
                    at: 1,
                    ..Pulse::default()
                }))
                .unwrap();
            writer
                .record(Row::Snapshot(crate::row::Snapshot {
                    instance: "prime".to_string(),
                    character: "Sindar".to_string(),
                    at: 2,
                    ..crate::row::Snapshot::default()
                }))
                .unwrap();
            writer
                .record(Row::Reward(crate::row::Reward {
                    instance: "prime".to_string(),
                    character: "Sindar".to_string(),
                    day: "2026-07-20".to_string(),
                    reward: "a trinket".to_string(),
                    at: 3,
                    ..crate::row::Reward::default()
                }))
                .unwrap();
            writer
                .record(Row::StoreVisit(crate::row::StoreVisit {
                    account: "acct".to_string(),
                    game: "prime".to_string(),
                    claimed: true,
                    at: 4,
                    ..crate::row::StoreVisit::default()
                }))
                .unwrap();
            writer.flush().unwrap();
        }
        let conn = ledger.connect(true).unwrap();
        let db = conn.db();
        assert_eq!(db.pulses.len(), 1);
        assert_eq!(db.snapshots.len(), 1);
        assert_eq!(db.rewards.len(), 1);
        assert_eq!(db.store_visits.len(), 1);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_reward_upsert_only_overwrites_an_empty_reward() {
        let dir = scratch("upsert");
        let ledger = Ledger::open(&dir).unwrap();
        let conn = ledger.connect(false).unwrap();
        let mut row = crate::row::Reward {
            instance: "prime".to_string(),
            character: "Sindar".to_string(),
            day: "2026-07-20".to_string(),
            reward: String::new(),
            thanks: "first".to_string(),
            at: 1,
            ..crate::row::Reward::default()
        };
        let first_id = conn.insert_reward(row.clone()).unwrap();
        // An empty candidate does not touch it.
        row.reward = String::new();
        row.thanks = "second".to_string();
        assert_eq!(conn.insert_reward(row.clone()).unwrap(), first_id);
        assert_eq!(conn.db().rewards[0].row.thanks, "first");
        // A filled candidate does, and keeps the row's id.
        row.reward = "a locket".to_string();
        row.thanks = "third".to_string();
        assert_eq!(conn.insert_reward(row).unwrap(), first_id);
        let db = conn.db();
        assert_eq!(db.rewards.len(), 1);
        assert_eq!(db.rewards[0].id, first_id);
        assert_eq!(db.rewards[0].row.thanks, "third");
        assert_eq!(db.rewards[0].row.reward, "a locket");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
