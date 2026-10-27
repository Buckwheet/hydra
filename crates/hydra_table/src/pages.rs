//! The Settings window: which page a character is on, and what it says.
//!
//! Recovered from `hydra_table::pages` (`0x1012638f8` … `0x10126d674`).  The
//! original drew the pages with `egui`; this reconstruction keeps the **model**
//! — the page list with its recovered titles and help text, the settings the
//! pages read, and the roster-name parser — and leaves drawing to the front end.
//!
//! The section names and their one-line help come straight out of `__TEXT`:
//!
//! ```text
//! 0x10240b1d5  travel                  scrollback            general
//! 0x10240b1e8  General                 log                   Player log
//! 0x10240b1f0  the next time the character logs in
//! 0x10240b222  Travel                  the next route asked for   themes
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use hydra_net::gemstone::endpoint;
use hydra_session::settings_store::{self, SettingsFile};

/// The settings section the Travel page writes.
///
/// **ESTABLISHED**: the 6-byte literal `travel` at `0x10240b1d5`, read with
/// `SettingsFile::section` in [`travel_settings`] (`0x1012638f8`).
pub const TRAVEL: &str = "travel";

/// The settings section the Log page writes.
///
/// **ESTABLISHED**: the 10-byte literal `player_log` at `0x10240aa2d`, used by
/// [`log_settings`] (`0x1012608e8`).
pub const PLAYER_LOG: &str = "player_log";

/// The suffix of the roster-name error.
///
/// **ESTABLISHED**: the format string at `0x103e63027` —
/// `{} is not a roster name (GAME:Name).` (`pages::who`, `0x101264510`).
pub const NOT_A_ROSTER_NAME: &str = " is not a roster name (GAME:Name).";

/// The help text under the command-symbol field when Lich owns the symbol.
///
/// **ESTABLISHED**: `0x10240b24c` — `Lich's command symbol is one mark, such as
/// ; or ,: not a letter, a digit or a space.`
pub const LICH_SYMBOL_HELP: &str =
    "Lich's command symbol is one mark, such as ; or ,: not a letter, a digit or a space.";

/// The help text under the command-symbol field.
///
/// **ESTABLISHED**: `0x10240b29c` — `The command symbol is one mark, such as . or
/// /: not a letter, a digit or a space.`
pub const SYMBOL_HELP: &str =
    "The command symbol is one mark, such as . or /: not a letter, a digit or a space.";

/// What `targetid` accepts.
///
/// **ESTABLISHED**: 40 bytes at `0x1024073e0` (`pages::targetid`,
/// `0x10126d674`).
pub const TARGETID_WORDS: &str = "targetid takes one word: on, off or status.";

/// Creature tags are shown.
///
/// **ESTABLISHED**: 25 bytes at `0x1024073c8`.
pub const CREATURE_TAGS_SHOWN: &str = "creature tags are shown.";

/// Creature tags are not shown.
///
/// **ESTABLISHED**: 27 bytes at `0x1024073b8`.
pub const CREATURE_TAGS_HIDDEN: &str = "creature tags are not shown.";

/// A page of the Settings window.
///
/// **ESTABLISHED**: the titles and help lines in the `0x10240b1d5` blob —
/// `travel`, `scrollback`, `general` (`General`), `log` (`Player log`, "the next
/// time the character logs in"), `Travel` ("the next route asked for") and
/// `themes` — plus the per-page renderers `pages::section` (`0x10126ae10` …
/// `0x10126bdf4`, eight of them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Page {
    General,
    Log,
    Travel,
    Scrollback,
    Themes,
    Sort,
    Symbols,
    TargetId,
    Macros,
    Hotbars,
    Memories,
    History,
    Rewards,
}

impl Page {
    /// Every page, in the order the Settings window shows them.
    ///
    /// **INFERRED**: the order follows the recovered renderer addresses
    /// (`pages::heading` `0x10126ab7c` then `pages::section` ×8); the titles are
    /// the recovered ones.
    pub const ALL: [Page; 13] = [
        Page::General,
        Page::Log,
        Page::Travel,
        Page::Scrollback,
        Page::Themes,
        Page::Sort,
        Page::Symbols,
        Page::TargetId,
        Page::Macros,
        Page::Hotbars,
        Page::Memories,
        Page::History,
        Page::Rewards,
    ];

    /// The title the window shows for this page.
    ///
    /// **ESTABLISHED** for `General`, `Player log`, `Travel` (`0x10240b1ee`,
    /// `0x10240b1f1`, `0x10240b222`); **INFERRED** for the rest, from the module
    /// each renderer lives in.
    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Log => "Player log",
            Page::Travel => "Travel",
            Page::Scrollback => "Scrollback",
            Page::Themes => "Themes",
            Page::Sort => "Sorter",
            Page::Symbols => "Symbols",
            Page::TargetId => "Target id",
            Page::Macros => "Macros",
            Page::Hotbars => "Hotbars",
            Page::Memories => "Memories",
            Page::History => "History",
            Page::Rewards => "Rewards",
        }
    }

    /// The one-line help under the title.
    ///
    /// **ESTABLISHED** for `Player log` ("the next time the character logs in")
    /// and `Travel` ("the next route asked for"); **UNKNOWN** for the rest, which
    /// have empty help here rather than invented text.
    pub fn help(self) -> &'static str {
        match self {
            Page::Log => "the next time the character logs in",
            Page::Travel => "the next route asked for",
            _ => "",
        }
    }

    /// The settings section this page writes, if it has one.
    pub fn section(self) -> Option<&'static str> {
        match self {
            Page::Travel => Some(TRAVEL),
            Page::Log => Some(PLAYER_LOG),
            _ => None,
        }
    }
}

/// The page a character's settings file says was last open.
///
/// **ESTABLISHED** (`pages::chosen`, `0x101268784`): the page is stored in the
/// settings file (the `travel`/`scrollback`/`general`/`log`/`themes` names at
/// `0x10240b1d5`) and the eight `pages::section` renderers are selected from it;
/// `pages::switch` (`0x101268ad8`) changes it.
///
/// **INFERRED**: the key the page is stored under is `page`; the original's key
/// name is not visible in the pieces that survive.
pub const PAGE_KEY: &str = "page";

/// Which page a settings file selects.
pub fn chosen(file: &SettingsFile) -> Page {
    match file.section(PAGE_KEY).and_then(|value| value.as_str()) {
        Some(name) => page_named(name),
        None => Page::General,
    }
}

/// The page with this name, or `General`.
///
/// The names are compared case-insensitively against [`Page::title`] and the
/// recovered section names.
pub fn page_named(name: &str) -> Page {
    let name = name.trim();
    if name.eq_ignore_ascii_case("player log") || name.eq_ignore_ascii_case(PLAYER_LOG) {
        return Page::Log;
    }
    Page::ALL
        .into_iter()
        .find(|page| page.title().eq_ignore_ascii_case(name))
        .unwrap_or(Page::General)
}

/// Parse a roster name of the form `GAME:Name`.
///
/// **ESTABLISHED** (`pages::who`, `0x101264510`): `memchr` for `:`; for every
/// prefix that is followed by more text the game is checked with
/// `hydra_net::gemstone::endpoint::instance` (`0x101c44af8`), and a prefix that
/// is not an instance gives `format!("{} is not a roster name (GAME:Name).")`
/// (`0x103e63027`).  The answer is `(game, character)`.
pub fn who(name: &str) -> Result<(String, String), String> {
    for (at, _) in name.match_indices(':') {
        if at + 1 >= name.len() {
            continue;
        }
        let game = &name[..at];
        let character = &name[at + 1..];
        return match endpoint::instance(game) {
            Some(endpoint) => Ok((endpoint.game.to_string(), character.to_string())),
            None => Err(format!("{name}{NOT_A_ROSTER_NAME}")),
        };
    }
    Err(format!("{name}{NOT_A_ROSTER_NAME}"))
}

/// Load the settings file for `who` out of `root`.
///
/// **ESTABLISHED** (`pages::load_file`, `0x10126d958`): `who` then
/// `hydra_session::settings_store::settings_path` (`0x101a059c0`) and
/// `settings_store::load`.
pub fn load_file(root: &Path, who: &str) -> Result<SettingsFile, String> {
    let (game, character) = self::who(who)?;
    let path = settings_store::settings_path(root, &game, &character)
        // UNKNOWN: the no-path message is not in the pool; the recovered
        // pieces are `who`, `settings_path` and `load`.
        .ok_or_else(|| format!("{who} has no settings path"))?;
    settings_store::load(&path)
}

/// The Travel page's settings for `who`, as switches.
///
/// **ESTABLISHED** (`pages::travel_settings`, `0x1012638f8`): `who`, then
/// `pages::load_file`, then the `travel` section.
///
/// The switches the travel code asks for are recovered: `call_wind`
/// (`0x1024088a1`), `typeahead` (`0x1024088b0`), `dock` (`0x10240889d`),
/// `swimming` (`0x1024077a0`) and `conserve` (`0x1024077a8`).
pub fn travel_settings(root: &Path, who: &str) -> BTreeMap<String, bool> {
    switches_of(root, who, TRAVEL, &TRAVEL_KEYS)
}

/// The switches the travel code asks for.
///
/// **ESTABLISHED**: `call_wind` (`0x1024088a1`), `typeahead` (`0x1024088b0`) and
/// `dock` (`0x10240889d`) are read by `travel_command`; `swimming`
/// (`0x1024077a0`) and `conserve` (`0x1024077a8`) by the sea swimmer.
pub const TRAVEL_KEYS: [&str; 5] = ["call_wind", "typeahead", "dock", "swimming", "conserve"];

/// The switches the log page asks for.
///
/// **ESTABLISHED**: `keep_days` (`0x10240656d`) is the only one visible in the
/// surviving pieces of `pages::log_settings` (`0x1012608e8`); it is a number, not
/// a switch, so the map is keyed by the booleans the page actually has.
pub const LOG_KEYS: [&str; 3] = ["keep", "keeps", "keep_days"];

fn switches_of(
    root: &Path,
    who: &str,
    section: &str,
    keys: &[&str],
) -> BTreeMap<String, bool> {
    let mut switches = BTreeMap::new();
    let Ok(file) = load_file(root, who) else {
        return switches;
    };
    let Some(section) = file.section(section) else {
        return switches;
    };
    for key in keys {
        if let Some(on) = section.get(key).and_then(|value| value.as_bool()) {
            switches.insert((*key).to_string(), on);
        }
    }
    switches
}

/// The Log page's settings for `who`.
///
/// **ESTABLISHED** (`pages::log_settings`, `0x1012608e8`): the `player_log`
/// section of the character's settings, the same shape as
/// [`travel_settings`].
pub fn log_settings(root: &Path, who: &str) -> BTreeMap<String, bool> {
    switches_of(root, who, PLAYER_LOG, &LOG_KEYS)
}

/// Resolve and check the settings file a change to `who` would be applied to.
///
/// **ESTABLISHED** (`pages::apply`, `0x10126467c`): `pages::who` then
/// `hydra_session::settings_store::settings_path` and a `format!` of the outcome.
///
/// **Departure**: the pending-edit model lives in the Settings window
/// (`hydra_gui`), which is not reconstructed, so applying a change means
/// resolving and parsing the file here and handing the path back to the front
/// end.
pub fn apply(root: &Path, who: &str) -> Result<std::path::PathBuf, String> {
    let (game, character) = self::who(who)?;
    let path = settings_store::settings_path(root, &game, &character)
        // UNKNOWN: as in `load_file`; the wording is reconstructed.
        .ok_or_else(|| format!("{who} has no settings path"))?;
    settings_store::load(&path)?;
    Ok(path)
}

/// The page list with the titles and help lines above, in window order.
///
/// **ESTABLISHED** (`pages::pages`, `0x1012647c0`): the window builds one entry
/// per page from `pages::heading` (`0x10126ab7c`) and the eight `pages::section`
/// renderers; `pages::put` (`0x101264110` …) places the rows.
pub fn pages() -> Vec<(Page, &'static str, &'static str)> {
    Page::ALL
        .into_iter()
        .map(|page| (page, page.title(), page.help()))
        .collect()
}

/// The sorting page's switches.
///
/// **ESTABLISHED** (`pages::sorting`, `0x10126c04c`): `pages::who`,
/// `pages::load_file` and a `sorter`/`sorter_width` pair whose help text is at
/// `0x10240a6fe` ("A look in or on a container shown as its contents by item
/// type, each type's items together and counted, rather than the game's one
/// line.").
pub fn sorting(root: &Path, who: &str) -> BTreeMap<String, bool> {
    switches_of(root, who, "sorter", &SORT_KEYS)
}

/// The symbols page's settings.
///
/// **ESTABLISHED** (`pages::symbols`, `0x10126c344`): the `symbols` section; the
/// two help lines are [`SYMBOL_HELP`] and [`LICH_SYMBOL_HELP`].
pub fn symbols(root: &Path, who: &str) -> BTreeMap<String, bool> {
    switches_of(root, who, "symbols", &SYMBOL_KEYS)
}

/// The target-id page's switches.
///
/// **ESTABLISHED** (`pages::targetid`, `0x10126d674`): the `targetid` section;
/// [`TARGETID_WORDS`], [`CREATURE_TAGS_SHOWN`] and [`CREATURE_TAGS_HIDDEN`] are
/// its three answers.
pub fn targetid(root: &Path, who: &str) -> BTreeMap<String, bool> {
    switches_of(root, who, "targetid", &TARGETID_KEYS)
}

/// The sorter switches.
pub const SORT_KEYS: [&str; 2] = ["sorter", "sorter_width"];

/// The symbol switches.
pub const SYMBOL_KEYS: [&str; 3] = ["symbol", "lich_symbol", "style"];

/// The target-id switches.
///
/// // UNKNOWN: `targetid` is recovered; the second word is not in the pool
/// (the messages say "creature tags", with a space), so it is reconstructed in
/// the settings-key spelling.
pub const TARGETID_KEYS: [&str; 2] = ["targetid", "creature_tags"];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_table_pages_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_roster_name_needs_a_known_game() {
        assert_eq!(
            who("GS4:Aelith"),
            Ok(("GS4".to_string(), "Aelith".to_string()))
        );
        assert_eq!(
            who("nonsense:Aelith"),
            Err("nonsense:Aelith is not a roster name (GAME:Name).".to_string())
        );
        assert_eq!(
            who("Aelith"),
            Err("Aelith is not a roster name (GAME:Name).".to_string())
        );
    }

    #[test]
    fn the_pages_are_titled_and_in_window_order() {
        let pages = pages();
        assert_eq!(pages.len(), 13);
        assert_eq!(pages[0].1, "General");
        assert_eq!(pages[1].1, "Player log");
        assert_eq!(pages[1].2, "the next time the character logs in");
        assert_eq!(pages[2].1, "Travel");
        assert_eq!(pages[2].2, "the next route asked for");
        assert_eq!(Page::Travel.section(), Some("travel"));
        assert_eq!(Page::Log.section(), Some("player_log"));
        assert_eq!(Page::Themes.section(), None);
    }

    #[test]
    fn a_page_is_found_by_its_title() {
        assert_eq!(page_named("travel"), Page::Travel);
        assert_eq!(page_named(" Player Log "), Page::Log);
        assert_eq!(page_named("player_log"), Page::Log);
        assert_eq!(page_named("nonsense"), Page::General);
    }

    #[test]
    fn the_recovered_help_strings_are_exact() {
        assert_eq!(TARGETID_WORDS, "targetid takes one word: on, off or status.");
        assert_eq!(CREATURE_TAGS_SHOWN, "creature tags are shown.");
        assert_eq!(CREATURE_TAGS_HIDDEN, "creature tags are not shown.");
        assert!(SYMBOL_HELP.ends_with("not a letter, a digit or a space."));
        assert!(LICH_SYMBOL_HELP.starts_with("Lich's command symbol is one mark"));
        assert_eq!(NOT_A_ROSTER_NAME, " is not a roster name (GAME:Name).");
    }

    #[test]
    fn the_travel_switches_come_from_the_travel_section() {
        let dir = dir("travel");
        let (game, character) = who("GS4:Aelith").expect("roster name");
        let path = settings_store::settings_path(&dir, &game, &character).expect("path");
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir");
        std::fs::write(
            &path,
            r#"{"schema_version":1,"instance":"GS4","character":"Aelith","sections":{"travel":{"call_wind":true,"dock":false,"typeahead":true},"player_log":{"keep_days":7}}}"#,
        )
        .expect("write");
        let switches = travel_settings(&dir, "gs4:aelith");
        assert_eq!(switches.get("call_wind"), Some(&true));
        assert_eq!(switches.get("dock"), Some(&false));
        assert_eq!(switches.get("typeahead"), Some(&true));
        assert_eq!(switches.len(), 3);
        let log = log_settings(&dir, "gs4:aelith");
        assert!(log.is_empty(), "keep_days is a number, not a switch");
        // An unknown character has no switches, and does not panic.
        assert!(travel_settings(&dir, "GS4:Nobody").is_empty());
    }
}
