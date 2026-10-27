//! `hydra_combat_log` — Hydra's combat log: it records fights into the ledger
//! and prints listings, session reports and analytics over them.
//!
//! Recovered from the arm64 Mach-O binary; `NOTES.md` records the evidence for
//! every signature, literal and SQL statement below.
//!
//! # `std`-only departure
//!
//! The original crate talks to SQLite through `rusqlite` (`Connection`,
//! `Statement`, `StatementCache`, `Row::get`, …), receives session events over
//! `tokio` broadcast channels and reports progress through `tokio::sync::mpsc`.
//! None of that is available here, so this build replaces the database with
//! [`store::Store`] — plain `Vec`s of the recovered row types — and
//! re-implements every query as a scan with the same semantics.  The SQL each
//! function used to run is kept in a `const` next to the Rust equivalent so the
//! recovered text is not lost, and every call site that used to pass a
//! `&Connection` now passes a `&Store`.

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
/// `// UNKNOWN:` the original propagated `rusqlite::Error` — the returned
/// `Result`'s discriminant word compared against `0x8000_0000_0000_0016` for
/// the `Ok` case, which is where `rusqlite::Error`'s niche lives.  `rusqlite` is
/// unavailable, so the failure is a formatted message; the text a report prints
/// on failure is unchanged, because the original printed that error with
/// `Display`.
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

/// The lines a report produced, in display order.
pub type Lines = Result<Vec<String>, Error>;

/// The Eastern-time helpers the timestamp lines call
/// (`hydra_net::eastern::{now,offset,date,format}` at 0x101c58dc8, 0x101c586c0,
/// 0x101c58d70).
pub(crate) use hydra_net::eastern;
