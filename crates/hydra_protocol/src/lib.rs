//! Streaming parser and wire-format helpers for the GemStone IV text protocol.
//!
//! The crate was recovered from the arm64 Hydra binary.  It is deliberately
//! `std`-only: the original used a few small helper crates for the parser
//! internals, but none of that leaks into the public behaviour implemented
//! here.
//!
//! Layout notes (from the recovered binary, arm64 ABI):
//!   * `Vec<T>` is `(cap, ptr, len)` on this toolchain.
//!   * `Option<String>` uses the `String` capacity word as a niche;
//!     `None` is `isize::MIN`.
//!   * `Run` has a stride of `0xf8` bytes.
//!   * `Frame` has a stride of `0x118` bytes.

#![allow(dead_code)]

pub mod frame;
pub mod numbers;
pub mod parser;
pub mod runs;
pub mod tags;
pub mod text;
pub mod write;

pub use frame::payload::{Capacity, InventoryItem, Link, LinkTarget, MerchantStatus, TextFrame};
pub use frame::Frame;
pub use numbers::{parse_amount, parse_duration_secs};
pub use parser::Parser;
pub use runs::{Run, Runs};
