//! The actions a keyboard bind or a controller button can do, and macros.
//!
//! **ESTABLISHED**:
//!
//!  * The action names and their human labels are two parallel literal tables
//!    (`0x102418f2f` for the names, `0x1024190ec` for the labels); `Action::name`
//!    is a jump table over the enum (`0x10147fa90`) and `Action::label` the
//!    matching one.
//!  * `typed` rewrites the two characters `\r` into a real carriage return
//!    (`0x10147fa68`, the literals `0x102418ebe`/`0x102418ec0`).
//!  * `shown` is `str::replace` with the caller's own pair (`0x10147fa58`).
//!  * Macros are `macros.toml`, keyed by name, each with `text` and `steps`.

use std::collections::BTreeMap;

use crate::text::{self, Value};

/// The file macros live in.
///
/// **ESTABLISHED** (`0x102418e5c`): `macros.toml`.
pub const FILE: &str = "macros.toml";

/// Turn the written escape `\r` into a carriage return.
///
/// **ESTABLISHED** (`0x10147fa68`): `replace("\\r", "\r")`.
pub fn typed(text: &str) -> String {
    text.replace("\\r", "\r")
}

/// Show text with one pair of characters replaced by another.
///
/// **ESTABLISHED** (`0x10147fa58`): `str::replace` with the pair the caller
/// passes, so a window can show `⏎` where the file wrote `\r`.
pub fn shown(text: &str, from: &str, to: &str) -> String {
    text.replace(from, to)
}

/// One thing a key or button can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SendOrRepeat,
    RepeatLast,
    RepeatSecondLast,
    HistoryBack,
    HistoryForward,
    ClearInput,
    ScrollPageUp,
    ScrollPageDown,
    ScrollLineUp,
    ScrollLineDown,
    ScrollTop,
    ScrollBottom,
    NextWindow,
    PreviousWindow,
    NextTab,
    PreviousTab,
    NextUnreadTab,
    Find,
    FindNext,
    FindPrevious,
    TargetNext,
    TargetPrevious,
    TargetClear,
    DrawerTop,
    DrawerBottom,
    DrawerLeft,
    DrawerRight,
    Lock,
    Arrange,
    /// `character_1` … `character_9`.
    Character(u8),
    /// `macro_set_0` … `macro_set_9`.
    MacroSet(u8),
}

/// The names of the actions that are not numbered, in the binary's order.
///
/// **ESTABLISHED** (`0x102418f2f`).
pub const NAMES: [&str; 29] = [
    "send_or_repeat",
    "repeat_last",
    "repeat_second_last",
    "history_back",
    "history_forward",
    "clear_input",
    "scroll_page_up",
    "scroll_page_down",
    "scroll_line_up",
    "scroll_line_down",
    "scroll_top",
    "scroll_bottom",
    "next_window",
    "previous_window",
    "next_tab",
    "previous_tab",
    "next_unread_tab",
    "find",
    "find_next",
    "find_previous",
    "target_next",
    "target_previous",
    "target_clear",
    "drawer_top",
    "drawer_bottom",
    "drawer_left",
    "drawer_right",
    "lock",
    "arrange",
];

/// The labels of those actions, in the same order.
///
/// **ESTABLISHED** (`0x1024190ec`).
pub const LABELS: [&str; 29] = [
    "Send, or on an empty line repeat",
    "Repeat the last command",
    "Repeat the one before it",
    "Back through what was typed",
    "Forward through what was typed",
    "Clear the command input",
    "Scroll back a page",
    "Scroll forward a page",
    "Scroll back a line",
    "Scroll forward a line",
    "Scroll to the oldest line",
    "Scroll to the newest line",
    "Use the next window",
    "Use the window before",
    "Show the next tab",
    "Show the tab before",
    "Show a tab with lines unread",
    "Find in the window in use",
    "Find the one before",
    "Find the one after",
    "Target the next creature",
    "Target the creature before",
    "Target nothing",
    "Open or shut the top drawer",
    "Open or shut the bottom drawer",
    "Open or shut the left drawer",
    "Open or shut the right drawer",
    "Lock the windows, or free them",
    "Arrange the windows",
];

/// The first label of each numbered run.
///
/// **ESTABLISHED** (`0x1024190ec`): `Use set 0 alone`, then `Use macro set 1`
/// through `Use macro set 9`, and `The first character's window` onwards.
pub const MACRO_SET_LABEL_0: &str = "Use set 0 alone";
pub const MACRO_SET_LABEL: &str = "Use macro set";
pub const CHARACTER_LABELS: [&str; 9] = [
    "The first character's window",
    "The second character's window",
    "The third character's window",
    "The fourth character's window",
    "The fifth character's window",
    "The sixth character's window",
    "The seventh character's window",
    "The eighth character's window",
    "The ninth character's window",
];

impl Action {
    /// The action `name` names.
    ///
    /// **ESTABLISHED** (`0x10147ff04`): the plain names, then `character_1` …
    /// `character_9` and `macro_set_0` … `macro_set_9`.
    pub fn named(name: &str) -> Option<Action> {
        let name = name.trim();
        if let Some(index) = NAMES.iter().position(|n| *n == name) {
            return Some(Action::all()[index].clone());
        }
        if let Some(rest) = name.strip_prefix("character_") {
            if let Ok(n) = rest.parse::<u8>() {
                if (1..=9).contains(&n) {
                    return Some(Action::Character(n));
                }
            }
        }
        if let Some(rest) = name.strip_prefix("macro_set_") {
            if let Ok(n) = rest.parse::<u8>() {
                if n <= 9 {
                    return Some(Action::MacroSet(n));
                }
            }
        }
        None
    }

    /// Every action, in the order the names are listed.
    pub fn all() -> Vec<Action> {
        let mut out = vec![
            Action::SendOrRepeat,
            Action::RepeatLast,
            Action::RepeatSecondLast,
            Action::HistoryBack,
            Action::HistoryForward,
            Action::ClearInput,
            Action::ScrollPageUp,
            Action::ScrollPageDown,
            Action::ScrollLineUp,
            Action::ScrollLineDown,
            Action::ScrollTop,
            Action::ScrollBottom,
            Action::NextWindow,
            Action::PreviousWindow,
            Action::NextTab,
            Action::PreviousTab,
            Action::NextUnreadTab,
            Action::Find,
            Action::FindNext,
            Action::FindPrevious,
            Action::TargetNext,
            Action::TargetPrevious,
            Action::TargetClear,
            Action::DrawerTop,
            Action::DrawerBottom,
            Action::DrawerLeft,
            Action::DrawerRight,
            Action::Lock,
            Action::Arrange,
        ];
        for n in 1..=9 {
            out.push(Action::Character(n));
        }
        for n in 0..=9 {
            out.push(Action::MacroSet(n));
        }
        out
    }

    /// The action's own name, as a bind writes it.
    ///
    /// **ESTABLISHED** (`0x10147fa90`).
    pub fn name(&self) -> String {
        match self {
            Action::Character(n) => format!("character_{n}"),
            Action::MacroSet(n) => format!("macro_set_{n}"),
            other => NAMES[Action::all()
                .iter()
                .position(|a| a == other)
                .unwrap_or(0)]
            .to_string(),
        }
    }

    /// The words a window shows for the action.
    ///
    /// **ESTABLISHED** (`0x10147fcc0`).
    pub fn label(&self) -> String {
        match self {
            Action::Character(n) => CHARACTER_LABELS
                .get(*n as usize - 1)
                .copied()
                .unwrap_or("The character's window")
                .to_string(),
            Action::MacroSet(0) => MACRO_SET_LABEL_0.to_string(),
            Action::MacroSet(n) => format!("{MACRO_SET_LABEL} {n}"),
            other => LABELS[Action::all()
                .iter()
                .position(|a| a == other)
                .unwrap_or(0)]
            .to_string(),
        }
    }
}

/// One macro: a name and the lines it types.
///
/// **ESTABLISHED** (`0x10147e978` for `read`, `0x10147f510` for `written`): the
/// file's `macros` table maps a name to `{ text = "…" }`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Macro {
    pub name: String,
    /// The text as the file wrote it, escapes and all.
    pub text: String,
}

impl Macro {
    /// A macro called `name` typing `text`.
    pub fn new(name: &str, text: &str) -> Macro {
        Macro {
            name: name.to_string(),
            text: text.to_string(),
        }
    }

    /// The lines this macro types, in order, with the escapes resolved.
    ///
    /// **PARTLY** (`0x10147f2f4`): the binary splits on the newlines and resolves
    /// `\r` in each; what it does with a line that ends in `\r` mid-way is
    /// UNKNOWN.
    pub fn steps(&self) -> Vec<String> {
        // Each `\r` ends a step, whether it was written as an escape or as a
        // real carriage return; a plain newline ends one too.
        self.text
            .lines()
            .flat_map(|line| line.split("\\r"))
            .flat_map(|step| step.split('\r'))
            .map(str::trim)
            .filter(|step| !step.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// What one press of this macro sends.
    ///
    /// **ESTABLISHED** (`0x10147ecd4`): the lines joined with `\r`, which is what
    /// the game takes as "send this line".
    pub fn said(&self) -> String {
        let steps = self.steps();
        if steps.is_empty() {
            String::new()
        } else {
            let mut out = String::new();
            for step in steps {
                out.push_str(step.trim_end_matches('\r'));
                out.push('\r');
            }
            out
        }
    }

    /// Is there anything to send?
    ///
    /// **ESTABLISHED** (`0x10147f05c`): a macro with no text, or one whose steps
    /// come to nothing, is empty.
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a macro needs a name".to_string());
        }
        if self.said().is_empty() {
            return Err(format!("The macro {} runs nothi", self.name));
        }
        Ok(())
    }

    /// The value the file would carry for this macro.
    ///
    /// **ESTABLISHED** (`0x10147f510`): `{ text = "…" }`.
    pub fn written(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("text".to_string(), Value::Str(self.text.clone()));
        Value::Table(map)
    }

    /// Read a macro out of a parsed document.
    pub fn read(name: &str, value: &Value) -> Macro {
        Macro {
            name: name.to_string(),
            text: match value {
                Value::Str(text) => text.clone(),
                other => other.str_of("text"),
            },
        }
    }
}

/// Every macro a folder has.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Macros {
    pub macros: BTreeMap<String, Macro>,
}

impl Macros {
    /// **ESTABLISHED** (`0x10147e978`): the `macros` table.
    pub fn read(doc: &Value) -> Macros {
        let mut macros = BTreeMap::new();
        if let Some(table) = doc.get("macros").and_then(Value::as_table) {
            for (name, value) in table {
                macros.insert(name.clone(), Macro::read(name, value));
            }
        }
        Macros { macros }
    }

    /// The same, from the file's text.
    pub fn parse(text: &str) -> Result<Macros, String> {
        Ok(Macros::read(&text::parse_toml(text)?))
    }

    /// The macro called `name`.
    pub fn of(&self, name: &str) -> Option<&Macro> {
        self.macros.get(name)
    }

    /// Write the macros back out.
    pub fn write(&self) -> Value {
        let mut table = BTreeMap::new();
        for (name, entry) in &self.macros {
            table.insert(name.clone(), entry.written());
        }
        let mut doc = BTreeMap::new();
        doc.insert("macros".to_string(), Value::Table(table));
        Value::Table(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_become_a_carriage_return() {
        assert_eq!(typed("up\\r"), "up\r");
        assert_eq!(typed("no escape"), "no escape");
        assert_eq!(typed("two\\rlines\\r"), "two\rlines\r");
        // The forward-only form is what a window shows.
        assert_eq!(shown("up\\r", "\\r", "⏎"), "up⏎");
    }

    #[test]
    fn every_name_maps_back_to_its_action() {
        for action in Action::all() {
            let name = action.name();
            assert_eq!(
                Action::named(&name),
                Some(action.clone()),
                "{name} did not round trip"
            );
            assert!(!action.label().is_empty(), "{name} has no label");
        }
        assert_eq!(Action::named("nonsense"), None);
        assert_eq!(Action::named("character_0"), None);
        assert_eq!(Action::named("macro_set_9"), Some(Action::MacroSet(9)));
        assert_eq!(Action::named("macro_set_10"), None);
    }

    #[test]
    fn names_and_labels_are_the_recovered_ones() {
        assert_eq!(Action::SendOrRepeat.name(), "send_or_repeat");
        assert_eq!(
            Action::SendOrRepeat.label(),
            "Send, or on an empty line repeat"
        );
        assert_eq!(Action::Lock.name(), "lock");
        assert_eq!(Action::Lock.label(), "Lock the windows, or free them");
        assert_eq!(Action::MacroSet(0).label(), "Use set 0 alone");
        assert_eq!(Action::MacroSet(3).label(), "Use macro set 3");
        assert_eq!(
            Action::Character(2).label(),
            "The second character's window"
        );
        assert_eq!(Action::Character(2).name(), "character_2");
        assert_eq!(NAMES.len(), LABELS.len());
    }

    #[test]
    fn a_macro_sends_one_line_per_step() {
        let entry = Macro::new("look", "look\\rglance\\r");
        assert_eq!(entry.steps(), vec!["look", "glance"]);
        assert_eq!(entry.said(), "look\rglance\r");
        assert_eq!(Macro::new("two", "a\\rb").steps(), vec!["a", "b"]);
        assert!(entry.check().is_ok());
        assert!(Macro::new("empty", "").check().is_err());
        assert!(Macro::new("  ", "x").check().is_err());
    }

    #[test]
    fn macros_round_trip_through_the_file() {
        let text = "\
[macros.look]
text = \"look\\\\r\"
[macros.glance]
text = \"glance\\\\r\"
";
        let macros = Macros::parse(text).unwrap();
        assert_eq!(macros.macros.len(), 2);
        assert_eq!(macros.of("look").unwrap().said(), "look\r");
        let again = Macros::read(&macros.write());
        assert_eq!(again, macros);
    }
}
