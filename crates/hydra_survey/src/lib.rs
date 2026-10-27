//! The room survey: the log a survey walk writes, the graph it builds out of
//! that log, and the two probes that ask a room whether magic works and whether
//! it can be splashed.
//!
//! Reconstructed from the arm64 binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
//! (no DWARF). `NOTES.md` holds the legend, the per-function evidence and every
//! departure from the original; the two that shape the crate:
//!
//! * the original deserialised the log with `serde`/`serde_json` over nine
//!   derived types (`Record`, `Happening`, `Body`, `Via`, `EdgeRecord`,
//!   `EdgeResult`, `RoomRecord`, `VerbRecord`, `EventRecord`). This build has no
//!   dependencies, so [`json`] is a small reader and [`log`] carries the
//!   hand-written field readers with the *recovered* names, order and types.
//! * the graph itself is `std` only (`BTreeMap`, `BTreeSet`, `HashMap`), which
//!   is what the original used too.

pub mod graph;
pub mod json;
pub mod log;
pub mod probes;
