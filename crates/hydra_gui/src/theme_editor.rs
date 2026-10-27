//! The theme editor.
//!
//! `hydra_gui::theme_editor::Editor`'s `Default` is at `0x1004fe07c` and its
//! `shape`/`typeface` pages at `0x1004c...`; none of it was stubbed.

/// The theme currently being edited.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Editor {
    pub name: String,
}
