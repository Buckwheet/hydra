//! The controller pad: which button does what, and which modifiers are held.
//!
//! **ESTABLISHED**:
//!
//!  * [`character_path`] is the folder's `controller.toml` (the literal
//!    `controller.toml` at `0x10207f363`).
//!  * `Binds::of` (`0x101489768`) reads the `[controller]` section into a map
//!    from chord to [`Bound`], `pressed` (`0x10148a05c`) finds the bind for the
//!    buttons held, `held_modifiers` returns the held modifier chords and
//!    `legend` the chords the overlay lists.
//!  * The legend's text comes from the action labels (`binding::Action::label`).

pub mod file;
pub mod interact;
pub mod names;
pub mod rumble;
pub mod touch;
pub mod tuning;
pub mod wheel;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::binding::Action;
use file::{Bound, ControllerFile};
use names::BindKey;
use tuning::Tuning;

/// Where a character's controller file lives.
///
/// **ESTABLISHED** (`0x101489608`): the folder's `controller.toml`.
pub fn character_path(folder: &Path) -> PathBuf {
    folder.join(file::FILE)
}

/// Everything the pad knows: the binds, the wheel and the tuning.
///
/// **ESTABLISHED** shapes: `Binds` (`0x101489684`) holds the button map, the
/// overlay list and the tuning; `held_modifiers` and `legend` read them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Binds {
    /// The chord to bound action, as the file spells it.
    pub buttons: BTreeMap<String, Bound>,
    /// The chords the legend lists, in file order.
    pub overlay: Vec<String>,
    /// The tuning of the sticks and triggers.
    pub tuning: Tuning,
}

impl Binds {
    /// The binds of a controller file.
    pub fn of(file: &ControllerFile) -> Binds {
        Binds {
            buttons: file.buttons.clone(),
            overlay: file.overlay.clone(),
            tuning: Tuning::read(&file.tuning),
        }
    }

    /// The binds Hydra ships with.
    pub fn built_in() -> Binds {
        Binds::of(&ControllerFile::built_in())
    }

    /// The modifier chords among the binds: those that are only ever used with
    /// another button.
    ///
    /// **ESTABLISHED** (`0x101489684`): a bind whose action is
    /// `controller_modifier` is a modifier, and a chord that starts with it is a
    /// chord.
    pub fn held_modifiers(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (chord, bound) in &self.buttons {
            if matches!(bound, Bound::Action(name) if name == "controller_modifier") {
                out.push(chord.clone());
            }
        }
        out.sort();
        out
    }

    /// The binds shown in the legend, with the words for each.
    ///
    /// **ESTABLISHED** (`0x101489f98`): for each chord the overlay lists, the
    /// bound's label — an action's own label, or the macro's text.
    pub fn legend(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for chord in &self.overlay {
            let Some(bound) = self.buttons.get(chord) else {
                continue;
            };
            let words = match bound {
                Bound::Action(name) => Action::named(name)
                    .map(|action| action.label())
                    .unwrap_or_else(|| name.clone()),
                Bound::Macro(text) => text.clone(),
                Bound::Wheel(Some(slice)) => format!("The {slice} wheel"),
                Bound::Wheel(None) => "The command wheel".to_string(),
                Bound::Unknown(name) => name.clone(),
            };
            out.push((chord.clone(), words));
        }
        out
    }

    /// The bind for the buttons that are held.
    ///
    /// **ESTABLISHED** (`0x10148a05c`): only a chord with exactly the same
    /// modifiers is done, so `l2+south` does not answer for a bare `south` while
    /// `l2` is held.  A bare button answers when no modifier is held.
    pub fn pressed(&self, button: &str, held: &[String]) -> Option<&Bound> {
        let mut chord: Vec<String> = held.iter().map(|m| m.trim().to_string()).collect();
        chord.sort();
        chord.push(button.trim().to_string());
        let wanted = chord.join("+");
        self.buttons.iter().find_map(|(key, bound)| {
            let parsed = BindKey::parse(key)?;
            let mut pieces = parsed.modifiers.clone();
            pieces.sort();
            pieces.push(parsed.key.clone());
            (pieces.join("+") == wanted).then_some(bound)
        })
    }

    /// The bind for a chord as written.
    pub fn bound(&self, chord: &str) -> Option<&Bound> {
        self.buttons.get(chord)
    }

    /// The buttons, in legend order.
    pub fn buttons(&self) -> Vec<&str> {
        let mut names: Vec<(u8, &str)> = self
            .buttons
            .keys()
            .map(|key| (names::rank(key.split('+').next_back().unwrap_or(key)), key.as_str()))
            .collect();
        names.sort();
        names.into_iter().map(|(_, key)| key).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_the_folders_controller_toml() {
        assert_eq!(
            character_path(Path::new("/data/Hydra")),
            PathBuf::from("/data/Hydra/controller.toml")
        );
    }

    #[test]
    fn the_built_in_binds_have_a_modifier_and_a_legend() {
        let binds = Binds::built_in();
        assert_eq!(binds.held_modifiers(), vec!["l2".to_string()]);
        let legend = binds.legend();
        assert_eq!(legend.len(), 4);
        assert_eq!(legend[0].0, "start");
        // `interact_mode` is the pad's own action, not one of the board's, so
        // its name is its legend text.
        assert_eq!(legend[0].1, "interact_mode");
        assert!(legend.iter().any(|(chord, _)| chord == "l2+south"));
    }

    #[test]
    fn a_chord_only_answers_with_its_modifiers_held() {
        let binds = Binds::built_in();
        // Bare south is the look macro.
        assert_eq!(
            binds.pressed("south", &[]),
            Some(&Bound::Macro("look\r".into()))
        );
        // With l2 held it is the stand macro.
        assert_eq!(
            binds.pressed("south", &["l2".to_string()]),
            Some(&Bound::Macro("stand\r".into()))
        );
        // A modifier order the file does not use still finds nothing.
        assert_eq!(binds.pressed("south", &["r2".to_string()]), None);
    }

    #[test]
    fn the_tuning_comes_along() {
        let binds = Binds::built_in();
        assert_eq!(binds.tuning.trigger_open_pct, tuning::DEFAULT_TRIGGER_OPEN_PCT);
        // The legend order puts the d-pad and face buttons first.
        assert_eq!(binds.buttons().first().copied(), Some("l2+south"));
    }
}
