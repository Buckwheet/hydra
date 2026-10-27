//! The macro file, read and written per character.
//!
//! **ESTABLISHED**:
//!
//!  * The file is `macros.toml` (`0x102418e5c`), in the character's folder.
//!  * `macro_chain` (`0x10141f31c`) says `There is no macro {}.` and `The macro
//!    {} runs nothi…`.
//!  * `keep` and `remove` go through `hydra_session::store::changing`
//!    (`0x10141f9f4`, `0x10141fbe8`).

use std::path::{Path, PathBuf};

use crate::binding::{Macro, Macros};
use crate::text;

/// The file macros live in.
pub const FILE: &str = "macros.toml";

/// Where a character's macros live.
///
/// **ESTABLISHED** (`0x10141f978`): the folder's `macros.toml`.
pub fn character_path(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

/// The places macros may be read from.
///
/// **ESTABLISHED** (`0x10141f7f4`): the character's own file, then `layouts`.
pub fn macros_paths(folder: &Path) -> Vec<PathBuf> {
    vec![character_path(folder), folder.join("layouts").join(FILE)]
}

/// The macros in a document.
///
/// **ESTABLISHED** (`0x10141f0b8`): the `macros` table.
pub fn macros_for(doc: &text::Value) -> Macros {
    Macros::read(doc)
}

/// The file's macros.
pub fn read(folder: &Path) -> Result<Macros, String> {
    let path = character_path(folder);
    let doc = text::read_file(&path)?;
    Ok(macros_for(&doc))
}

/// The lines pressing a macro sends.
///
/// **ESTABLISHED** (`0x10141f31c`): a macro that is not there is `There is no
/// macro {}.`, one with no text is `The macro {} runs nothing.`, and otherwise
/// the lines are sent one at a time.
pub fn macro_chain(macros: &Macros, name: &str) -> Result<Vec<String>, String> {
    let entry = macros
        .of(name)
        .ok_or_else(|| format!("There is no macro {name}."))?;
    let steps = entry.steps();
    if steps.is_empty() {
        return Err(format!("The macro {name} runs nothing."));
    }
    Ok(steps)
}

/// Keep a macro in the file.
///
/// **ESTABLISHED** (`0x10141f9f4`): the write goes through the session's store,
/// which rewrites the file from under a lock; this does the same synchronously.
pub fn keep(folder: &Path, entry: Macro) -> Result<(), String> {
    let mut macros = read(folder).unwrap_or_default();
    macros.macros.insert(entry.name.clone(), entry);
    text::write_file(&character_path(folder), &macros.write())
}

/// Take a macro out of the file.
///
/// **ESTABLISHED** (`0x10141fbe8`): the write goes through the session's store.
pub fn remove(folder: &Path, name: &str) -> Result<bool, String> {
    let mut macros = read(folder).unwrap_or_default();
    let found = macros.macros.remove(name).is_some();
    if found {
        text::write_file(&character_path(folder), &macros.write())?;
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paths_put_layouts_second() {
        let paths = macros_paths(Path::new("/data/Al"));
        assert_eq!(paths[0], PathBuf::from("/data/Al/macros.toml"));
        assert_eq!(paths[1], PathBuf::from("/data/Al/layouts/macros.toml"));
    }

    #[test]
    fn a_chain_is_the_macros_steps_or_a_complaint() {
        let doc = text::parse_toml("[macros.look]\ntext = \"look\\r\"\n").unwrap();
        let macros = macros_for(&doc);
        assert_eq!(macro_chain(&macros, "look").unwrap(), vec!["look"]);
        assert_eq!(
            macro_chain(&macros, "gone").unwrap_err(),
            "There is no macro gone."
        );
        let empty = Macros::default();
        assert!(macro_chain(&empty, "look").is_err());
    }
}
