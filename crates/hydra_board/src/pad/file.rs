//! The controller binds file: the default `controller.toml`, and reading a
//! folder's own.
//!
//! **ESTABLISHED**:
//!
//!  * [`ControllerFile::hydras`] is the default file's text, reproduced byte for
//!    byte from `0x10207f45a` (2453 bytes).  It is the only place the button
//!    names, the chord names and the built-in wheel are written down.
//!  * [`ControllerFile::read`] parses `controller`, `controller_tuning`,
//!    `controller_overlay` and `controller_wheel` (`0x1013d9fc0`), reporting a
//!    bad file as `format!("not read: {}", error)` (`0x102418f5a`).
//!  * [`ControllerFile::load`] reports a missing or unreadable file as
//!    `format!("{}: {}", path, error)`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::binding::{self, Action};
use crate::pad::names::BindKey;
use crate::pad::wheel::Slice;
use crate::text::{self, Value};

/// The file a folder's binds live in.
///
/// **ESTABLISHED** (`0x10207f363`): `controller.toml`.
pub const FILE: &str = "controller.toml";

/// The section the buttons are bound in.
pub const SECTION: &str = "controller";

/// The section the legend reads.
pub const OVERLAY: &str = "controller_overlay";

/// The section the wheel is described in.
pub const WHEEL: &str = "controller_wheel";

/// One bound button.
///
/// **ESTABLISHED** (`0x1013dbec0`): either an action's name, or a
/// `{ macro_text = "..." }` table whose text is what gets typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// An action the board knows, by name.
    Action(String),
    /// Text to type, exactly as the file gave it.
    Macro(String),
    /// A wheel to hold open, with an optional slice path.
    Wheel(Option<String>),
    /// Nothing this build understands.
    Unknown(String),
}

impl Bound {
    /// Read a value from the file.
    pub fn read(value: &Value) -> Bound {
        match value {
            Value::Str(text) => Bound::of_name(text),
            Value::Table(_) => match value.get("macro_text").and_then(Value::as_str) {
                Some(macro_text) => Bound::Macro(macro_text.to_string()),
                None => Bound::Unknown(String::new()),
            },
            _ => Bound::Unknown(String::new()),
        }
    }

    /// Classify an action name.
    ///
    /// **ESTABLISHED** (`0x102418e9c`): `interact_mode`, `interact_select`,
    /// `controller_modifier` and `controller_wheel[:slice]` are the names the pad
    /// itself acts on; the rest are actions or typing.
    pub fn of_name(name: &str) -> Bound {
        if let Some(rest) = name.strip_prefix("controller_wheel") {
            if let Some(slice) = rest.strip_prefix(':') {
                return Bound::Wheel(Some(slice.to_string()));
            }
            return Bound::Wheel(None);
        }
        if Action::named(name).is_some()
            || matches!(
                name,
                "interact_mode" | "interact_select" | "controller_modifier" | "controller_overlay"
            )
        {
            Bound::Action(name.to_string())
        } else {
            Bound::Unknown(name.to_string())
        }
    }

    /// What this bound says when it is pressed.
    ///
    /// **PARTLY** (`0x1013dbec0`): a macro's text goes through
    /// [`binding::typed`], so the two characters `\r` become a real carriage
    /// return; an action says nothing here because the board does it instead.
    pub fn said(&self) -> Option<String> {
        match self {
            Bound::Macro(text) => Some(binding::typed(text)),
            _ => None,
        }
    }

    /// The value the file would carry for this bound.
    pub fn written(&self) -> Value {
        match self {
            Bound::Macro(text) => {
                let mut map = BTreeMap::new();
                map.insert("macro_text".to_string(), Value::Str(text.clone()));
                Value::Table(map)
            }
            Bound::Action(name) | Bound::Unknown(name) => Value::Str(name.clone()),
            Bound::Wheel(Some(slice)) => Value::Str(format!("controller_wheel:{slice}")),
            Bound::Wheel(None) => Value::Str("controller_wheel".to_string()),
        }
    }
}

/// A data folder's controller file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ControllerFile {
    /// The button binds, by chord.
    pub buttons: BTreeMap<String, Bound>,
    /// The chords the legend lists.
    pub overlay: Vec<String>,
    /// The wheel, as the file describes it.
    pub wheel: Vec<Slice>,
    /// The tuning section, read by [`crate::pad::tuning::Tuning`].
    pub tuning: Value,
}

impl ControllerFile {
    /// The binds Hydra uses while a folder has no `controller.toml`.
    ///
    /// **ESTABLISHED** (`0x1013d9fc0`): this exact text, byte for byte.
    pub fn hydras() -> &'static str {
        DEFAULT_CONTROLLER
    }

    /// Read the text of a controller file.
    ///
    /// **ESTABLISHED** (`0x1013d9fc0`): the four sections; a bad one is
    /// `format!("not read: {}", error)`.
    pub fn read(text: &str) -> Result<ControllerFile, String> {
        let doc = text::parse_toml(text).map_err(|e| format!("not read: {e}"))?;
        ControllerFile::of_doc(&doc)
    }

    /// The same, from an already-parsed document.
    pub fn of_doc(doc: &Value) -> Result<ControllerFile, String> {
        let mut file = ControllerFile::default();
        if let Some(buttons) = doc.get(SECTION).and_then(Value::as_table) {
            for (chord, value) in buttons {
                let key = BindKey::parse(chord)
                    .map(|k| k.canonical())
                    .unwrap_or_else(|| chord.clone());
                file.buttons.insert(key, Bound::read(value));
            }
        }
        if let Some(overlay) = doc.get(OVERLAY) {
            file.overlay = overlay.strings_of("buttons");
        }
        if let Some(wheel) = doc.get(WHEEL) {
            file.wheel = match wheel {
                Value::Array(items) => items.iter().map(Slice::read).collect(),
                other => vec![Slice::read(other)],
            };
        }
        file.tuning = doc
            .get(crate::pad::tuning::FILE)
            .cloned()
            .unwrap_or(Value::Null);
        Ok(file)
    }

    /// Read a folder's `controller.toml`.
    ///
    /// **ESTABLISHED** (`0x1013d9c98`): a file that will not read is reported as
    /// `format!("{}: {}", path, error)`; the caller keeps the defaults.
    pub fn load(path: &Path) -> Result<ControllerFile, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        ControllerFile::read(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The binds Hydra has before a folder has any.
    pub fn built_in() -> ControllerFile {
        ControllerFile::read(ControllerFile::hydras()).unwrap_or_default()
    }

    /// The bound for a chord, whatever order its modifiers are written in.
    pub fn bound(&self, chord: &str) -> Option<&Bound> {
        let wanted = BindKey::parse(chord)?.sorted().canonical();
        self.buttons.iter().find_map(|(key, bound)| {
            let key = BindKey::parse(key)?;
            (key.sorted().canonical() == wanted).then_some(bound)
        })
    }

    /// Where a folder's controller file lives.
    pub fn path(folder: &Path) -> PathBuf {
        folder.join(FILE)
    }
}

/// The default `controller.toml`, recovered from `0x10207f45a`.
pub const DEFAULT_CONTROLLER: &str = r##"# Hydra's controller binds, used while the data folder has no
# controller.toml. Copy this there to change them, or put a controller.toml
# written for VellumFE there: its binds work as they are.
#
# Buttons: south, east, north, west, dpad_up, dpad_down, dpad_left,
# dpad_right, l1, r1, l2, r2, l3, r3, select, start, guide.
# A value is an action's name, or { macro_text = "..." }: commands sent.

[controller]
start = "interact_mode"                  # Interact mode, begun or left
dpad_up = { macro_text = "up\r" }
dpad_down = { macro_text = "down\r" }
dpad_right = { macro_text = "out\r" }
south = { macro_text = "look\r" }
l2 = "controller_modifier"               # Held: chords another button (l2+south)
r2 = "controller_wheel"                  # Held: the command wheel
r3 = "controller_wheel:portals"          # Held: the room's ways out
select = "controller_overlay"            # The legend, shown or put away

# Chords: l2 held, another button pressed. Only the bind with exactly the
# modifiers held is done.
"l2+dpad_up" = "scroll_page_up"
"l2+dpad_down" = "scroll_page_down"
"l2+south" = { macro_text = "stand\r" }

# The binds the legend lists.
[controller_overlay]
buttons = ["start", "l2", "r2", "l2+south"]

# The command wheel, held open with r2: aim at a slice with the stick that
# does not walk, hold it there, and let go to fire. A slice with slices of
# its own is a folder: hold on it to open it, and on Back to come out.
[[controller_wheel]]
label = "look"
command = "look"

[[controller_wheel]]
label = "search"
command = "search"

[[controller_wheel]]
label = "stand"
command = "stand"

[[controller_wheel]]
label = "hide"
command = "hide"

[[controller_wheel]]
label = "exp"
command = "exp"

[[controller_wheel]]
label = "health"
command = "health"

[[controller_wheel]]
label = "glance"
command = "glance"

[[controller_wheel]]
label = "stance"

  [[controller_wheel.slices]]
  label = "defensive"
  command = "stance defensive"
  color = "#2e8b57"

  [[controller_wheel.slices]]
  label = "neutral"
  command = "stance neutral"
  color = "#4682b4"

  [[controller_wheel.slices]]
  label = "forward"
  command = "stance forward"
  color = "#cc8844"

  [[controller_wheel.slices]]
  label = "offensive"
  command = "stance offensive"
  color = "#b03030"
"##;

#[cfg(test)]
mod tests {
    use super::*;


#[test]
fn tmp_debug() {
    let text = crate::pad::file::ControllerFile::hydras();
    let doc = crate::text::parse_toml(text).unwrap();
    println!("{:#?}", doc.get("controller"));
}

    #[test]
    fn the_default_file_is_the_binarys_own_text() {
        let text = ControllerFile::hydras();
        assert!(text.starts_with("# Hydra's controller binds"));
        // 0x102415fec, length 0x8eb = 2283.
        assert_eq!(text.len(), 2283);
        for button in crate::pad::names::BUTTONS {
            assert!(text.contains(button), "{button} is missing");
        }
    }

    #[test]
    fn the_default_file_reads() {
        let file = ControllerFile::built_in();
        assert_eq!(
            file.buttons.get("start"),
            Some(&Bound::Action("interact_mode".into()))
        );
        // TOML turns the file's \r into a real carriage return.
        assert_eq!(
            file.buttons.get("south"),
            Some(&Bound::Macro("look\r".into()))
        );
        assert_eq!(file.buttons.get("r2"), Some(&Bound::Wheel(None)));
        assert_eq!(
            file.buttons.get("r3"),
            Some(&Bound::Wheel(Some("portals".into())))
        );
        assert!(file.buttons.contains_key("l2+dpad_up"));
        assert_eq!(file.overlay, vec!["start", "l2", "r2", "l2+south"]);
        assert_eq!(file.wheel.len(), 8);
        assert_eq!(file.wheel[0].label.as_deref(), Some("look"));
        assert_eq!(file.wheel[0].command.as_deref(), Some("look"));
        let stance = &file.wheel[7];
        assert_eq!(stance.label.as_deref(), Some("stance"));
        assert_eq!(stance.slices.len(), 4);
        assert_eq!(stance.slices[0].label.as_deref(), Some("defensive"));
        assert_eq!(stance.slices[0].color.as_deref(), Some("#2e8b57"));
    }

    #[test]
    fn a_macro_says_what_it_types() {
        let bound = Bound::Macro("up\\r".into());
        assert_eq!(bound.said().as_deref(), Some("up\r"));
        assert_eq!(Bound::Action("lock".into()).said(), None);
    }

    #[test]
    fn wheel_binds_carry_a_slice() {
        assert_eq!(Bound::of_name("controller_wheel"), Bound::Wheel(None));
        assert_eq!(
            Bound::of_name("controller_wheel:portals"),
            Bound::Wheel(Some("portals".into()))
        );
        assert_eq!(Bound::of_name("lock"), Bound::Action("lock".into()));
        assert_eq!(Bound::of_name("nonsense"), Bound::Unknown("nonsense".into()));
    }

    #[test]
    fn a_folder_file_is_read_with_its_chords() {
        let text = "\n[controller]\nsouth = { macro_text = \"north\\r\" }\nl2+ctrl = \"lock\"\n";
        let file = ControllerFile::read(text).unwrap();
        assert_eq!(file.buttons.len(), 2);
        assert_eq!(file.bound("south"), Some(&Bound::Macro("north\r".into())));
        // The chord is canonicalised.
        assert!(file.buttons.contains_key("l2+ctrl"));
        assert!(file.wheel.is_empty());
    }
}
