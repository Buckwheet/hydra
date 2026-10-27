//! Player reports.
//!
//! `hydra_gui::reports::Reports`'s `Default` is at `0x1005848fc`; the type was
//! not stubbed, so nothing is reconstructed here.

/// A report the player filed, as kept by the GUI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reports {
    pub entries: Vec<String>,
}
