//! Hotbars: rows of buttons, each of which sends a command.
//!
//! **ESTABLISHED**:
//!
//!  * The file is `hotbars.toml` (`0x102418e4d`) and lives in the character's
//!    folder, beside `macros.toml` and `layouts`.
//!  * The complaints are, word for word: `Not sent: the button sends nothing.`,
//!    `Not sent: the button names a thing in focus, and the page has none.`,
//!    `Not sent: a hotbar button never presses another.`,
//!    `Not sent: the button's command has characters that cannot be typed.` and
//!    `Not sent: the button's command is too long.` (`0x102418e4d` onwards).
//!  * Pressing says `There is no hotbar {}.`, `The hotbar {} has no bu…` and
//!    `The button {} sends not…` (`0x1014213cc`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::binding::{self};
use crate::text::{self, Value};

/// The file hotbars live in.
pub const FILE: &str = "hotbars.toml";

/// The longest command a button may send.
///
/// **ESTABLISHED** (`0x102418e4d`, "the button's command is too long"): the
/// limit is the game's line length; the exact number is UNKNOWN, so this is the
/// figure the messages sit beside.
pub const MAX_COMMAND: usize = 512;

/// A button sends nothing.
pub const NOTHING: &str = "Not sent: the button sends nothing.";
/// A button names something in focus and the page has none.
pub const NO_FOCUS: &str = "Not sent: the button names a thing in focus, and the page has none.";
/// A hotbar button may not press another.
pub const NO_NESTING: &str = "Not sent: a hotbar button never presses another.";
/// A command with characters that cannot be typed.
pub const UNTYPABLE: &str = "Not sent: the button's command has characters that cannot be typed.";
/// A command that is too long.
pub const TOO_LONG: &str = "Not sent: the button's command is too long.";

/// The binds the hotbar page is drawn from.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::hotbars`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hotbars {
    /// Bar name to its buttons, in order.
    pub bars: BTreeMap<String, Vec<Button>>,
}

/// One button.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Button {
    /// The words on the button.
    pub label: String,
    /// The command it sends.
    pub command: String,
}

impl Button {
    /// The lines a button sends, or why it sends nothing.
    ///
    /// **ESTABLISHED** (`0x1014207ec`): an empty command is [`NOTHING`], one
    /// that names a hotbar is [`NO_NESTING`], one with a character that cannot be
    /// typed is [`UNTYPABLE`] and one past [`MAX_COMMAND`] is [`TOO_LONG`]; the
    /// rest are split into one line per `\r`.
    pub fn lines(&self) -> Result<Vec<String>, String> {
        let command = self.command.trim();
        if command.is_empty() {
            return Err(NOTHING.to_string());
        }
        if command.contains(".hotbar") || command.starts_with(HOTBAR) {
            return Err(NO_NESTING.to_string());
        }
        if command.chars().any(|c| c.is_control() && c != '\r' && c != '\n') {
            return Err(UNTYPABLE.to_string());
        }
        if command.len() > MAX_COMMAND {
            return Err(TOO_LONG.to_string());
        }
        Ok(binding::typed(command)
            .split('\r')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }
}

/// The word that presses a hotbar.
///
/// **ESTABLISHED** (`0x102418e80`): `hotbar`.
pub const HOTBAR: &str = "hotbar";

/// Where a character's hotbars live.
///
/// **ESTABLISHED** (`0x101420da4`): the character folder's `hotbars.toml`.
pub fn character_path(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

/// The places hotbars may be read from, in order.
///
/// **ESTABLISHED** (`0x101420c20`): the character's own file, then the `layouts`
/// folder the client keeps beside it.
pub fn hotbars_paths(folder: &Path) -> Vec<PathBuf> {
    vec![character_path(folder), folder.join("layouts").join(FILE)]
}

/// The command a button press turns into.
///
/// **ESTABLISHED** (`0x101420e20`): `hotbar <bar> <button>`.
pub fn hotbar_command(bar: &str, button: &str) -> String {
    format!("{HOTBAR} {bar} {button}")
}

/// The bar called `name`.
///
/// **ESTABLISHED** (`0x10142125c`): the name is trimmed of whitespace and
/// compared case-sensitively.
pub fn named<'a>(hotbars: &'a Hotbars, name: &str) -> Option<&'a [Button]> {
    hotbars.bars.get(name.trim()).map(Vec::as_slice)
}

/// What pressing a button sends.
///
/// **ESTABLISHED** (`0x1014213cc`): the three complaints above, then the
/// buttons's own lines.
pub fn press(hotbars: &Hotbars, bar: &str, button: &str) -> String {
    let Some(buttons) = named(hotbars, bar) else {
        return format!("There is no hotbar {bar}.");
    };
    let Some(button) = buttons
        .iter()
        .find(|candidate| candidate.label.trim() == button.trim())
    else {
        return format!("The hotbar {bar} has no button {button}.");
    };
    match button.lines() {
        Ok(lines) => lines.join("\r"),
        Err(message) => message,
    }
}

/// Read the hotbars out of a document.
///
/// **ESTABLISHED** (`0x101420588`): the `hotbars` table, each bar a list of
/// buttons with `label` and `command`.
pub fn hotbars_for(doc: &Value) -> Hotbars {
    let mut bars = BTreeMap::new();
    if let Some(table) = doc.get("hotbars").and_then(Value::as_table) {
        for (name, value) in table {
            let buttons = match value {
                Value::Array(items) => items
                    .iter()
                    .map(|item| Button {
                        label: item.str_of("label"),
                        command: item.str_of("command"),
                    })
                    .collect(),
                other => vec![Button {
                    label: other.str_of("label"),
                    command: other.str_of("command"),
                }],
            };
            bars.insert(name.clone(), buttons);
        }
    }
    Hotbars { bars }
}

/// The same, from the file's text.
pub fn parse(text: &str) -> Result<Hotbars, String> {
    Ok(hotbars_for(&text::parse_toml(text)?))
}

/// Write the hotbars back out.
pub fn write(hotbars: &Hotbars) -> Value {
    let mut table = BTreeMap::new();
    for (name, buttons) in &hotbars.bars {
        let items = buttons
            .iter()
            .map(|button| {
                let mut map = BTreeMap::new();
                map.insert("label".to_string(), Value::Str(button.label.clone()));
                map.insert("command".to_string(), Value::Str(button.command.clone()));
                Value::Table(map)
            })
            .collect();
        table.insert(name.clone(), Value::Array(items));
    }
    let mut doc = BTreeMap::new();
    doc.insert("hotbars".to_string(), Value::Table(table));
    Value::Table(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hotbars() -> Hotbars {
        parse(
            "\
[hotbars]
main = [ { label = \"look\", command = \"look\" } ]
",
        )
        .unwrap()
    }

    #[test]
    fn a_button_sends_its_lines_or_says_why_not() {
        let plain = Button {
            label: "look".into(),
            command: "look\\rglance\\r".into(),
        };
        assert_eq!(plain.lines().unwrap(), vec!["look", "glance"]);
        assert_eq!(
            Button::default().lines().unwrap_err(),
            NOTHING.to_string()
        );
        assert_eq!(
            Button {
                label: "press".into(),
                command: "hotbar main look".into()
            }
            .lines()
            .unwrap_err(),
            NO_NESTING.to_string()
        );
        assert_eq!(
            Button {
                label: "long".into(),
                command: "x".repeat(MAX_COMMAND + 1)
            }
            .lines()
            .unwrap_err(),
            TOO_LONG.to_string()
        );
        assert_eq!(
            Button {
                label: "odd".into(),
                command: "a\u{7}b".into()
            }
            .lines()
            .unwrap_err(),
            UNTYPABLE.to_string()
        );
    }

    #[test]
    fn the_paths_are_the_character_file_then_layouts() {
        let paths = hotbars_paths(Path::new("/data/Al"));
        assert_eq!(paths[0], PathBuf::from("/data/Al/hotbars.toml"));
        assert_eq!(paths[1], PathBuf::from("/data/Al/layouts/hotbars.toml"));
        assert_eq!(hotbar_command("main", "look"), "hotbar main look");
    }

    #[test]
    fn pressing_finds_the_bar_and_the_button() {
        let bars = hotbars();
        assert_eq!(press(&bars, "main", "look"), "look");
        assert_eq!(press(&bars, "nope", "look"), "There is no hotbar nope.");
        assert_eq!(
            press(&bars, "main", "gone"),
            "The hotbar main has no button gone."
        );
        assert!(named(&bars, " main ").is_some());
    }

    #[test]
    fn hotbars_round_trip() {
        let mut bars = Hotbars::default();
        bars.bars.insert(
            "main".to_string(),
            vec![Button {
                label: "look".into(),
                command: "look".into(),
            }],
        );
        let doc = write(&bars);
        assert_eq!(hotbars_for(&doc), bars);
    }
}
