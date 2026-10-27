//! The chat pane.
//!
//! `hydra_gui::chat::Kept` (its serde visitor is at `0x100679684`, its `Default`
//! at `0x1006a7d20`) was not stubbed, so nothing is reconstructed here.

/// The chat history kept in the layout file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kept {
    pub lines: Vec<String>,
}
