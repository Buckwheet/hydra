//! The play window: the per-character game window and its title bar.
//!
//! Only `play::titlebar::{left_at_first, right_at_first}` were stubbed. The rest
//! of the module — `play::Play` (14 868 bytes at `0x1001ac30`), `play::draw`,
//! `play::holders`, `play::options` — is not reconstructed; see `NOTES.md`.

pub mod titlebar;

pub use titlebar::{left_at_first, right_at_first, Item, TitleBar, Written};
