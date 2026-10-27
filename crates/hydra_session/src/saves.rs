//! What the game "heard": the saves/notes the client records from game text.
//!
//! Recovered from `saves::Saves::heard` at `0x101a6a688` (32 KB of inlined
//! matching).  The recovered calls are
//! `hydra_model::state::menu::LearnedCommands::entry`,
//! `std::path::{Path::components, compare_components}`,
//! `core::slice::sort::stable::quicksort` and `String::clone`, so `heard`
//! recognises a line against a table, and one of the things it recognises is a
//! **path** (a saved route, compared component by component).
//!
//! **INFERRED**: the recognised forms are the "you have saved" family of
//! messages; the exact texts are inlined and not literals of this crate.
//! **UNKNOWN**: what every branch stores.
//!
//! The reconstruction keeps the observable contract: `heard` tells the caller
//! whether the line was a save, and what was saved.

use std::path::{Component, Path, PathBuf};

/// One thing the game told us it saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Save {
    /// a named route the game will replay for this character
    Path(String, PathBuf),
    /// a note or alias the game keeps for this character
    Named(String, String),
}

/// The saves recognised from the game stream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saves {
    /// the saves seen so far, in order
    pub saves: Vec<Save>,
}

impl Saves {
    /// An empty record.
    pub fn new() -> Self {
        Saves::default()
    }

    /// Read one game line.
    ///
    /// **ESTABLISHED** shape: returns `Option<Save>`-like information, matches
    /// case-insensitively, and normalises a path by its components (the
    /// recovered `Path::components` + `compare_components`).
    pub fn heard(&mut self, line: &str) -> Option<Save> {
        let lower = line.to_lowercase();
        let save = if let Some(rest) = lower.strip_prefix("saved command: ") {
            Save::Named("command".to_string(), rest.trim().to_string())
        } else if let Some(rest) = lower.strip_prefix("saved note: ") {
            Save::Named("note".to_string(), rest.trim().to_string())
        } else if let Some(rest) = lower.strip_prefix("saved path: ") {
            Save::Path(rest.trim().to_string(), normalise(rest.trim()))
        } else {
            return None;
        };
        if !self.saves.contains(&save) {
            self.saves.push(save.clone());
        }
        Some(save)
    }

    /// Was this line a save?
    pub fn is_save(line: &str) -> bool {
        let lower = line.to_lowercase();
        lower.starts_with("saved command: ")
            || lower.starts_with("saved note: ")
            || lower.starts_with("saved path: ")
    }

    /// The paths saved so far, in order.
    pub fn paths(&self) -> Vec<&PathBuf> {
        self.saves
            .iter()
            .filter_map(|s| match s {
                Save::Path(_, p) => Some(p),
                _ => None,
            })
            .collect()
    }
}

/// Normalise a path by its components, the way `Path::components` +
/// `compare_components` do: `.` components disappear and repeated separators
/// collapse.
pub fn normalise(path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_are_recognised_case_insensitively() {
        let mut saves = Saves::new();
        assert_eq!(
            saves.heard("Saved command: kill rat"),
            Some(Save::Named("command".to_string(), "kill rat".to_string()))
        );
        assert_eq!(saves.saves.len(), 1);
        assert_eq!(saves.heard("nothing here"), None);
        assert!(Saves::is_save("SAVED NOTE: the cart"));
        assert!(!Saves::is_save("you saved nothing"));
    }

    #[test]
    fn paths_are_normalised_by_component() {
        let mut saves = Saves::new();
        assert_eq!(
            saves.heard("saved path: ./town/./square"),
            Some(Save::Path(
                "./town/./square".to_string(),
                PathBuf::from("town/square")
            ))
        );
        assert_eq!(saves.paths(), vec![&PathBuf::from("town/square")]);
        assert_eq!(normalise("a//b/./c"), PathBuf::from("a/b/c"));
    }

    #[test]
    fn a_repeated_save_is_recorded_once() {
        let mut saves = Saves::new();
        saves.heard("saved path: town/square");
        saves.heard("saved path: town/square");
        assert_eq!(saves.saves.len(), 1);
    }
}
