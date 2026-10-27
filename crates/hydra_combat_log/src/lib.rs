//! `hydra_combat_log` — Hydra's combat log: it records fights into the ledger
//! and prints listings, session reports and analytics over them.
//!
//! Recovered from the arm64 Mach-O binary; `NOTES.md` records the evidence for
//! every signature, literal and SQL statement below.
//!
//! # Dependencies (Phase 1 reconnection)
//!
//! The original crate talks to SQLite through `rusqlite` (`Connection`,
//! `Statement`, `StatementCache`, `Row::get`, …), receives session events over
//! `tokio` broadcast channels and reports progress through `tokio::sync::mpsc`.
//! Both are back, and the crates it leans on for the database and the events are
//! the real ones:
//!
//! * **`rusqlite` 0.37.0 (`bundled`)** — [`store::Store`] holds a
//!   [`hydra_ledger::Connection`], and every recovered statement really runs
//!   through `prepare_cached` (rusqlite's `StatementCache`) against the ledger's
//!   recovered schema.
//! * **`tokio` 1.53.1 (`full`)** — [`task::listen`] is the recovered async loop:
//!   a `broadcast::Receiver` of session events on one side, a
//!   `tokio::sync::mpsc::Sender<task::Sent>` on the other.  [`task::start`]
//!   keeps its synchronous shape by building a current-thread runtime.
//! * **`hydra_ledger`** — the database's owner: `Store::open` is
//!   `Ledger::open` + `Ledger::writing`, `Store::open_read_only` is
//!   `Ledger::connect(true)`, and `Store::name_id` is `hydra_ledger::name_id`.
//! * **`hydra_session`** — [`task::relay`] consumes the session's real
//!   `command::Event` broadcast channel.
//! * **`hydra_net`** — `eastern::{offset, date, format, now}` for the stamps.
//!
//! `NOTES.md` §1 keeps the table of what used to stand in for each.

#![allow(dead_code, unused)]

pub mod analytics;
pub mod catalog;
pub mod command;
pub mod detail;
pub mod listing;
pub mod recorder;
pub mod report;
pub mod store;
pub mod task;

pub use store::Store;

/// A report failure.
///
/// The original propagated `rusqlite::Error` — the returned `Result`'s
/// discriminant word compares against `0x8000_0000_0000_0016` for the `Ok` case,
/// which is where `rusqlite::Error`'s niche lives — and printed it with
/// `Display` on the way out.  This type keeps that text: the `rusqlite` and
/// `hydra_ledger` errors convert into it through the [`From`] impls below, so a
/// failed statement lands here instead of unwrapping, and the text a report
/// prints on failure is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl Error {
    pub fn new(message: impl Into<String>) -> Error {
        Error(message.into())
    }

    /// The text the original would hand to `Display`.
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<Error> for String {
    fn from(e: Error) -> String {
        e.0
    }
}

impl From<rusqlite::Error> for Error {
    /// A statement that SQLite refused — the `Display` text rusqlite gives,
    /// which is what the original printed.
    fn from(error: rusqlite::Error) -> Error {
        Error(error.to_string())
    }
}

impl From<hydra_ledger::Error> for Error {
    /// The ledger's own failure (a read-only connection, a schema refusal, an
    /// I/O error), already `Display`-formatted there.
    fn from(error: hydra_ledger::Error) -> Error {
        Error(error.to_string())
    }
}

/// The lines a report produced, in display order.
pub type Lines = Result<Vec<String>, Error>;

/// The Eastern-time helpers the timestamp lines call
/// (`hydra_net::eastern::{now,offset,date,format}` at 0x101c58dc8, 0x101c586c0,
/// 0x101c58d70).
pub(crate) use hydra_net::eastern;
