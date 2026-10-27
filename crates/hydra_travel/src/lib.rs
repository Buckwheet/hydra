//! Hydra's travel system: routes, sailing, swimming, walking and the puzzle
//! routines that get a walker through a room the map alone cannot cross.
//!
//! Reconstructed from the arm64 binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
//! (no DWARF). See `NOTES.md` for the legend, the per-function table and every
//! remaining departure. The largest departure is that the original talked to
//! `cena_map::map::Map` (an external crate that is not part of this workspace)
//! through helpers such as `Map::room`, `Map::neighbours` and
//! `cena_map::exit::Cost::price`; this build carries the minimal room index it
//! needs in [`map`] instead, with the same accessor shapes.
//!
//! Phase 1 reconnected the file format itself: `travel.json` is read and written
//! by `serde_json` through `serde` derives on [`file::Shared`], [`file::Spot`]
//! and [`walk::instability::Seen`] again, and [`json::Value`] is
//! `serde_json::Value`. See `NOTES.md`, "Phase 1 reconnection".
#![allow(dead_code, unused)]

pub mod command;
pub mod coverage;
pub mod facts;
pub mod file;
pub mod json;
pub mod knows;
pub mod map;
pub mod route;
pub mod sail;
pub mod settings;
pub mod swim;
pub mod walk;

/// A room's map id (`cena_map::room::RoomId` in the original: a bare `u32`,
/// which is what `Map::room`'s second argument is in every call site).
pub type RoomId = u32;

/// Re-exported so the modules below can talk about a walker's state without
/// repeating the dependency path.
pub use hydra_model::state::GameState;

/// The crate's own version string, as the binary reported it.
pub const VERSION: &str = "hydra/0.1.0-beta.7";

/* unresolved symbols (trait impls, closures, generics), all covered here:
 *
 * `hydra_travel::file::<impl From<file::Refused> for std::io::Error>::from`
 *     -> [`file::Refused`] + `impl From<Refused> for std::io::Error`
 * `<impl Deserialize for file::Spot>..Visitor::expecting`
 *     -> [`file::Spot`] (`struct Spot` = the 13-byte literal at `0x10241bc8b`);
 *        Phase 1: this is `serde`'s derive, not a hand-written visitor.
 * `<impl Deserialize for file::Shared>..Visitor::expecting`
 *     -> [`file::Shared`] (`struct Shared` = the 13-byte literal at
 *        `0x10241bc96`; the field-count literal `struct Shared with 4 elements`
 *        is at `0x10241dfac`); Phase 1: `serde`'s derive.
 * `<impl Deserialize for walk::instability::Seen>..Visitor::expecting`
 *     -> [`walk::instability::Seen`] (`struct Seen with 3 elements` at
 *        `0x10241dfd6`, fields `room`, `element`, `seen` from `0x10241f4ee`);
 *        Phase 1: `serde`'s derive.
 */
