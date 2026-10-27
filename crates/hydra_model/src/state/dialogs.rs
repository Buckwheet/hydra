//! Dialog data (`<dialogData>` blocks).
//!
//! `Dialogs::read` (3176 B) parses `dialogData`/`id` blocks; the recovered
//! literals `Debuffs`, `Cooldowns`, `Buffs`, `Active Spells` are the dialog ids.
//! `Dialog::location` (100 B), `Dialogs::get` (668 B) and `Dialogs::scripted`
//! (2704 B) are the accessors.

use std::collections::BTreeMap;

/// One dialog's payload.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    pub id: String,
    /// `// UNKNOWN:` the payload is a list of key/value rows; kept as text.
    pub body: String,
    /// The screen location hint, when the dialog carries one.
    pub location: Option<String>,
}

impl Dialog {
    /// @0x0000000101b? (100 B)
    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }
}

/// All dialogs, keyed by id.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Dialogs {
    pub dialogs: BTreeMap<String, Dialog>,
}

/// The dialog ids recovered from the literals.
pub const KNOWN_IDS: [&str; 4] = ["Buffs", "Debuffs", "Cooldowns", "Active Spells"];

impl Dialogs {
    /// @0x0000000101ba0a70 (3176 B)
    pub fn read(&mut self, line: &str) -> bool {
        let Some(rest) = line.split_once("dialogData") else {
            return false;
        };
        let Some(id) = attribute(rest.1, "id") else {
            return false;
        };
        let body = line
            .split_once('>')
            .map(|(_, body)| body.trim_end_matches("</dialogData>").trim().to_string())
            .unwrap_or_default();
        match self.dialogs.get(&id) {
            Some(existing) if existing.body == body => false,
            _ => {
                self.dialogs.insert(
                    id.clone(),
                    Dialog {
                        id,
                        body,
                        location: None,
                    },
                );
                true
            }
        }
    }

    /// @0x0000000101b? (668 B)
    pub fn get(&self, id: &str) -> Option<&Dialog> {
        self.dialogs.get(id)
    }

    /// @0x0000000101b? (2704 B) — dialogs opened by a script command.
    pub fn scripted(&mut self, id: &str) -> bool {
        if self.dialogs.contains_key(id) {
            return false;
        }
        self.dialogs.insert(
            id.to_string(),
            Dialog {
                id: id.to_string(),
                body: String::new(),
                location: None,
            },
        );
        true
    }
}

fn attribute(text: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = text.find(&needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_data_is_stored_by_id() {
        let mut dialogs = Dialogs::default();
        assert!(dialogs.read("<dialogData id=\"Buffs\">haste</dialogData>"));
        let dialog = dialogs.get("Buffs").unwrap();
        assert_eq!(dialog.body, "haste");
        assert!(!dialogs.read("<dialogData id=\"Buffs\">haste</dialogData>"));
    }

    #[test]
    fn scripted_dialogs_are_created_once() {
        let mut dialogs = Dialogs::default();
        assert!(dialogs.scripted("Cooldowns"));
        assert!(!dialogs.scripted("Cooldowns"));
        assert!(dialogs.get("Cooldowns").unwrap().location().is_none());
    }

    #[test]
    fn unrelated_lines_change_nothing() {
        let mut dialogs = Dialogs::default();
        assert!(!dialogs.read("You stand still."));
    }
}
