//! The learned-command table (the game's own menu of verbs).
//!
//! Recovered from `menu_store::{load, merge_and_save, underlay}` at
//! `0x101a4f16c`, `0x101a4e960`, `0x101a4f984`.
//!
//! **ESTABLISHED**:
//! * the file is **text**, not JSON: `merge_and_save` formats with
//!   `core::fmt::write` and the template at `0x103e7bf2d`, which decodes to
//!   `{}\t{}\t{}\t{}\n` — four tab-separated columns and a newline.  So the
//!   learned commands are one line per entry with four fields.
//! * `merge_and_save` (`0x101a4e960`) calls `store::changing`
//!   (`0x101aa2ac4`, the "only write when it changed" helper), then
//!   `menu_store::load`, then `hydra_model::state::menu::LearnedCommands::
//!   {absorb, set_version, novel}`, then `store::save_bytes`; a failed save
//!   calls `remove_file` (it does not leave a partial file behind).
//! * `underlay` (`0x101a4f984`) returns the entries the file knows about that
//!   the game has not confirmed yet.
//!
//! **UNKNOWN** what the four columns are.  The recovered model calls
//! (`LearnedCommands::{absorb, set_version, novel}`) prove there is a version
//! plus a command list, so we write `version`, `verb`, `noun`, `confirmed` —
//! the minimal set that keeps the version, the command and its confirmation
//! state.

use crate::store;
use std::path::{Path, PathBuf};

/// The learned-command file's name.
pub const MENU_FILE: &str = "menu.tsv";
/// The schema this build writes.
pub const SCHEMA_VERSION: u32 = 1;

/// One learned command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Learned {
    /// the verb (`look`, `stance`, …)
    pub verb: String,
    /// the noun it applies to, if any
    pub noun: String,
    /// whether the game has confirmed it
    pub confirmed: bool,
}

impl Learned {
    /// A command with no noun.
    pub fn verb(verb: &str) -> Self {
        Learned {
            verb: verb.to_string(),
            noun: String::new(),
            confirmed: false,
        }
    }

    /// A command with a noun.
    pub fn of(verb: &str, noun: &str) -> Self {
        Learned {
            verb: verb.to_string(),
            noun: noun.to_string(),
            confirmed: false,
        }
    }

    /// The recovered line form (`{}\t{}\t{}\t{}\n`).
    pub fn line(&self, version: u32) -> String {
        format!(
            "{version}\t{}\t{}\t{}\n",
            self.verb,
            self.noun,
            if self.confirmed { "yes" } else { "no" }
        )
    }

    /// Read one line of the recovered form.
    pub fn parse(line: &str) -> Option<(u32, Learned)> {
        let mut parts = line.split('\t');
        let version = parts.next()?.trim().parse().ok()?;
        let verb = parts.next()?.to_string();
        let noun = parts.next().unwrap_or("").to_string();
        let confirmed = matches!(parts.next().unwrap_or(""), "yes" | "true" | "1");
        if verb.is_empty() {
            return None;
        }
        Some((
            version,
            Learned {
                verb,
                noun,
                confirmed,
            },
        ))
    }
}

/// The learned-command table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuFile {
    /// the game's version of the menu when these were learned
    pub version: u32,
    /// the entries, in file order
    pub commands: Vec<Learned>,
}

impl MenuFile {
    /// An empty table at the current version.
    pub fn new() -> Self {
        MenuFile {
            version: SCHEMA_VERSION,
            commands: Vec::new(),
        }
    }

    /// Render the file.
    pub fn to_text(&self) -> String {
        self.commands
            .iter()
            .map(|c| c.line(self.version))
            .collect()
    }

    /// Read the file's text.
    pub fn parse(text: &str) -> MenuFile {
        let mut file = MenuFile::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some((version, command)) = Learned::parse(line) {
                file.version = version;
                file.commands.push(command);
            }
        }
        file
    }

    /// Merge `other` in: version wins if newer, commands are added once.
    ///
    /// **ESTABLISHED** shape (`LearnedCommands::absorb` + `novel`): absorb the
    /// version and only add commands that are new.
    pub fn merge(&mut self, other: &MenuFile) {
        self.version = self.version.max(other.version);
        for command in &other.commands {
            if !self
                .commands
                .iter()
                .any(|c| c.verb == command.verb && c.noun == command.noun)
            {
                self.commands.push(command.clone());
            }
        }
    }
}

/// `<root>/<character>/menu.tsv`.
pub fn path(root: &Path, character: &str) -> PathBuf {
    root.join(store::safe_component(character)).join(MENU_FILE)
}

/// Read the table.  A missing file is an empty one.
///
/// **ESTABLISHED** shape (`0x101a4f16c`): read and parse.
pub fn load(root: &Path, character: &str) -> Result<MenuFile, String> {
    match std::fs::read_to_string(path(root, character)) {
        Ok(text) => Ok(MenuFile::parse(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(MenuFile::new()),
        Err(e) => Err(format!("{}: {e}", path(root, character).display())),
    }
}

/// Merge `file` into the file on disk and write it.
///
/// **ESTABLISHED** shape (`0x101a4e960`): `store::changing` (only when it
/// changed), load, merge, `save_bytes`.
pub fn merge_and_save(
    root: &Path,
    character: &str,
    file: &MenuFile,
) -> Result<MenuFile, String> {
    let mut on_disk = load(root, character)?;
    on_disk.merge(file);
    let path = path(root, character);
    let dir = path.parent().unwrap_or(Path::new("."));
    store::save_bytes(dir, &path, on_disk.to_text().as_bytes())
        .map_err(|e| format!("{} was not saved: {e}", path.display()))?;
    Ok(on_disk)
}

/// The entries the file knows that the game has not confirmed.
///
/// **ESTABLISHED** shape (`0x101a4f984`): the difference between the stored
/// table and the game's, reported for the "underlay" hint.
pub fn underlay(stored: &MenuFile, known_confirmed: &[Learned]) -> Vec<Learned> {
    stored
        .commands
        .iter()
        .filter(|c| {
            !known_confirmed
                .iter()
                .any(|k| k.verb == c.verb && k.noun == c.noun && k.confirmed)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_format_is_four_tab_separated_columns() {
        let command = Learned::of("stance", "offensive");
        assert_eq!(command.line(3), "3\tstance\toffensive\tno\n");
        let (version, back) = Learned::parse("7\tlook\tcart\tyes").unwrap();
        assert_eq!(version, 7);
        assert_eq!(back.verb, "look");
        assert_eq!(back.noun, "cart");
        assert!(back.confirmed);
        assert_eq!(Learned::parse("nonsense"), None);
        assert_eq!(Learned::parse("1\t\tx\tyes"), None);
    }

    #[test]
    fn parse_and_render_round_trip() {
        let mut file = MenuFile::new();
        file.version = 4;
        file.commands.push(Learned::of("kill", "rat"));
        file.commands.push(Learned::verb("stance"));
        let text = file.to_text();
        assert_eq!(text.lines().count(), 2);
        assert_eq!(MenuFile::parse(&text), file);
    }

    #[test]
    fn merge_keeps_the_newer_version_and_does_not_duplicate() {
        let mut a = MenuFile::new();
        a.version = 2;
        a.commands.push(Learned::of("kill", "rat"));
        let mut b = MenuFile::new();
        b.version = 5;
        b.commands.push(Learned::of("kill", "rat"));
        b.commands.push(Learned::verb("stance"));
        a.merge(&b);
        assert_eq!(a.version, 5);
        assert_eq!(a.commands.len(), 2);
    }

    #[test]
    fn underlay_reports_what_the_game_has_not_confirmed() {
        let mut stored = MenuFile::new();
        stored.commands.push(Learned::of("kill", "rat"));
        stored.commands.push(Learned::verb("stance"));
        let mut confirmed = Learned::of("kill", "rat");
        confirmed.confirmed = true;
        let missing = underlay(&stored, &[confirmed]);
        assert_eq!(missing, vec![Learned::verb("stance")]);
    }

    #[test]
    fn merge_and_save_round_trips_through_disk() {
        let root = std::env::temp_dir().join(format!("hydra-menu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(load(&root, "Aelith").unwrap(), MenuFile::new());
        let mut file = MenuFile::new();
        file.commands.push(Learned::of("kill", "rat"));
        let saved = merge_and_save(&root, "Aelith", &file).unwrap();
        assert_eq!(saved.commands.len(), 1);
        let back = load(&root, "Aelith").unwrap();
        assert_eq!(back, saved);
        let _ = std::fs::remove_dir_all(&root);
    }
}
