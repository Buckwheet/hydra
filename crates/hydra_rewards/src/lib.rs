//! `hydra_rewards` — GemStone's daily login reward.
//!
//! The crate watches the lines a session says at login, keeps the reward it
//! announces, replays old logs for rewards, and reports them (`rewards`,
//! `rewards all`, `rewards store`) from chat and from the GUI's report
//! catalogue.  A small TOML file (`rewards.toml`) holds the per-character
//! schedule.
//!
//! Recovered from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
//! (no DWARF).  Every module's `pub` items quote the unslid virtual address
//! they were recovered from; see `NOTES.md` for the evidence, the exact
//! message strings and the `std`-only departures.
//!
//! The shape of the crate, as the binary has it:
//!
//! * [`parse`] — the two line grammars (`You have earned the following
//!   reward: …`, `Thank you for logging into … consecutive day …`).
//! * [`watcher`] — the state machine that turns protocol frames into
//!   [`watcher::Heard`] rows, and the row type itself.
//! * [`catalog`] — the three `rewards …` sub-commands, for the GUI's report
//!   list.
//! * [`report`] — the chat reports.
//! * [`replay`], [`recorder`] — replaying logs and recording live frames.
//! * [`store`] — when the store's SimuCoins come round again.
//! * [`schedule`] — `rewards.toml`.

#![allow(dead_code, unused)]

pub mod catalog;
pub mod toml;
pub mod parse;
pub mod recorder;
pub mod replay;
pub mod report;
pub mod row;
pub mod schedule;
pub mod store;
pub mod watcher;

pub use row::{Stamp, Who};
pub use watcher::{given_before, Heard, Said, Watcher};
