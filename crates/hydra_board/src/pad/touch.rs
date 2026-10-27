//! What a touch on the screen sends: a wheel slice, or a plain command.
//!
//! **ESTABLISHED**:
//!
//!  * The messages are `"{} sends nothing."`, `"{} needs a thing in
//!    interact"`, `"No slice was named."` and `"That slice is no longer on the
//!    wheel."` (`0x1024183xx`).
//!  * `TouchWheel` looks its slices up in the wheels by name and falls back to
//!    the `default` wheel (`0x10148a480`); the file is `pad/touch.rs`
//!    (`0x103e6acb3`).

use std::collections::BTreeMap;

use crate::binding::Action;
use crate::pad::wheel::{Wheels, DEFAULT_WHEEL};
use crate::text::{self, Value};

/// One slice a touch can reach.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TouchSlice {
    /// What the slice is called.
    pub label: String,
    /// The command it sends, if it sends one.
    pub command: Option<String>,
    /// The action it does instead of sending, if any.
    pub action: Option<Action>,
    /// The wheel it opens, if it is a folder.
    pub opens: Option<String>,
}

impl TouchSlice {
    /// Read one out of the wheels.
    ///
    /// **ESTABLISHED** (`0x10148a480`): the label is the slice's `label`, and a
    /// slice with `slices` of its own opens a wheel of that name.
    pub fn of(slice: &crate::pad::wheel::Slice) -> TouchSlice {
        TouchSlice {
            label: slice.label.clone().unwrap_or_default(),
            command: slice.command.clone(),
            action: slice
                .command
                .as_deref()
                .and_then(Action::named)
                .or_else(|| slice.label.as_deref().and_then(Action::named)),
            opens: slice.slices.first().map(|_| {
                slice
                    .label
                    .clone()
                    .unwrap_or_else(|| DEFAULT_WHEEL.to_string())
            }),
        }
    }

    /// What pressing this slice sends.
    ///
    /// **PARTLY** (`0x10148a8e0`): a slice with a command sends it; a folder
    /// opens instead; anything else says nothing.
    pub fn said(&self) -> Result<String, String> {
        match &self.command {
            Some(command) if !command.trim().is_empty() => Ok(command.clone()),
            _ if self.opens.is_some() => Ok(String::new()),
            _ => Err(format!("{} sends nothing.", self.label)),
        }
    }
}

/// The wheel a touch opens.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TouchWheel {
    /// The wheel's name.
    pub name: String,
    /// Its slices, in order.
    pub slices: Vec<TouchSlice>,
}

impl TouchWheel {
    /// The wheel called `name`, or the `default` wheel.
    ///
    /// **ESTABLISHED** (`0x10148a480`): the name is looked up in the wheels and
    /// `default` answers when it is missing.
    pub fn of(wheels: &Wheels, name: &str) -> TouchWheel {
        let name = if wheels.wheels.contains_key(name) {
            name
        } else {
            DEFAULT_WHEEL
        };
        TouchWheel {
            name: name.to_string(),
            slices: wheels
                .of(name)
                .map(|slices| slices.iter().map(TouchSlice::of).collect())
                .unwrap_or_default(),
        }
    }

    /// The slice under a touch angle.
    pub fn at(&self, index: usize) -> Option<&TouchSlice> {
        self.slices.get(index)
    }

    /// The slice called `label`.
    ///
    /// **ESTABLISHED** (`0x10148a8e0`): a name that is not on the wheel is
    /// `"That slice is no longer on the wheel."`.
    pub fn named(&self, label: &str) -> Result<&TouchSlice, String> {
        self.slices
            .iter()
            .find(|slice| slice.label == label)
            .ok_or_else(|| {
                if label.trim().is_empty() {
                    "No slice was named.".to_string()
                } else {
                    "That slice is no longer on the wheel.".to_string()
                }
            })
    }
}

/// The command a touch on slice `label` of the wheel `name` sends.
///
/// **ESTABLISHED** (`0x10148a8e0`): the slice is looked up by name and its
/// command returned; the errors are the three messages above.
pub fn touch_command(wheels: &Wheels, name: &str, label: &str) -> Result<String, String> {
    let wheel = TouchWheel::of(wheels, name);
    wheel.named(label)?.said()
}

/// The wheel a touch opens, from the folder's own bindings.
///
/// **PARTLY** (`0x10148a480`): UNKNOWN how the binary names the wheel a touch
/// starts from; the `default` wheel is the conservative answer.
pub fn touch_wheel(wheels: &Wheels, name: &str) -> TouchWheel {
    TouchWheel::of(wheels, name)
}

/// The binds a touch file carries, by button.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::pad::touch::binds`).
pub fn binds(doc: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(table) = doc.get("touch").and_then(Value::as_table) {
        for (button, value) in table {
            if let Some(text) = value.as_str() {
                out.insert(button.clone(), text.to_string());
            }
        }
    }
    out
}

/// Read the touch section of a document.
pub fn read(text: &str) -> Result<BTreeMap<String, String>, String> {
    Ok(binds(&text::parse_toml(text)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheels() -> Wheels {
        Wheels::parse(
            "\
[controller_wheels]
main = []
default = [ { label = \"look\", command = \"look\" }, { label = \"folder\", slices = [ { label = \"in\" } ] } ]
",
        )
        .unwrap()
    }

    #[test]
    fn a_slice_sends_its_command_or_says_it_sends_nothing() {
        let wheels = wheels();
        assert_eq!(touch_command(&wheels, "default", "look").unwrap(), "look");
        assert_eq!(
            touch_command(&wheels, "default", "folder").unwrap(),
            ""
        );
        let named = TouchWheel::of(&wheels, "default");
        assert_eq!(
            named.named("gone").unwrap_err(),
            "That slice is no longer on the wheel."
        );
        assert_eq!(named.named("  ").unwrap_err(), "No slice was named.");
    }

    #[test]
    fn an_unknown_wheel_falls_back_to_default() {
        let wheels = wheels();
        let wheel = TouchWheel::of(&wheels, "nope");
        assert_eq!(wheel.name, DEFAULT_WHEEL);
        assert_eq!(wheel.slices.len(), 2);
        assert_eq!(wheel.at(0).unwrap().label, "look");
    }

    #[test]
    fn a_slice_with_sub_slices_opens_a_wheel() {
        let wheels = wheels();
        let wheel = TouchWheel::of(&wheels, "default");
        assert!(wheel.at(1).unwrap().opens.is_some());
        assert!(wheel.at(0).unwrap().opens.is_none());
    }

    #[test]
    fn touch_binds_are_read_by_button() {
        let map = read("[touch]\nsouth = \"look\"\n").unwrap();
        assert_eq!(map.get("south").map(String::as_str), Some("look"));
    }
}
