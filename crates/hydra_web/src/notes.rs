//! Room notes for the page.
//!
//! Recovered from `notes::read` (`0x1002c2d30`) and the serde glue of
//! `NoteOut` (`0x1002c298c`) / `NotesOut` (`0x1002c2ae0`, `0x1002c2bf8`).

use std::path::Path;

use crate::json::Value;

/// One note (`NoteOut`): `who, character, name, changed, text, lines`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteOut {
    /// The note key (which room / title it was taken for).
    pub who: String,
    /// The character the note belongs to.
    pub character: String,
    /// The room title or clip name.
    pub name: String,
    /// When it was taken.
    pub changed: String,
    /// The whole note as one string.
    pub text: String,
    /// The note line by line.
    pub lines: Vec<String>,
}

impl NoteOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("who", Value::str(self.who.clone())),
            ("character", Value::str(self.character.clone())),
            ("name", Value::str(self.name.clone())),
            ("changed", Value::str(self.changed.clone())),
            ("text", Value::str(self.text.clone())),
            (
                "lines",
                Value::Arr(self.lines.iter().map(|s| Value::str(s.clone())).collect()),
            ),
        ]
    }
}

/// The answer to a notes request.
///
/// Two `Serialize` impls survive for `notes::NotesOut`: one writing `notes`
/// (`0x1002c2ae0`) and one writing `problem` (`0x1002c2bf8`) — i.e. the original
/// was an untagged enum.
#[derive(Clone, Debug, PartialEq)]
pub enum NotesOut {
    Notes(Vec<NoteOut>),
    Problem(String),
}

impl NotesOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        match self {
            NotesOut::Notes(notes) => vec![(
                "notes",
                Value::Arr(notes.iter().map(|n| crate::wire::object(n.fields())).collect()),
            )],
            NotesOut::Problem(problem) => vec![("problem", Value::str(problem.clone()))],
        }
    }
}

/// Read the notes for the page (`0x1002c2d30`).
///
/// Recovered: one call to `hydra_session::notes::load`, a `Vec<NoteOut>` built
/// with `from_iter`, and — on `Refused` — the refusal's `Display` text carried as
/// the `problem` variant (`<hydra_session::notes::Refused as
/// core::fmt::Display>::fmt` is called on that path).
pub fn read(root: &Path, character: &str) -> NotesOut {
    match hydra_session::notes::load(root) {
        Ok(notes) => NotesOut::Notes(
            notes
                .notes
                .iter()
                .map(|n| NoteOut {
                    who: n.key.clone(),
                    character: character.to_string(),
                    name: n.title.clone(),
                    changed: n.when.clone(),
                    text: n.text(),
                    lines: n.lines.clone(),
                })
                .collect(),
        ),
        Err(refused) => NotesOut::Problem(refused.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_fields_are_in_the_recovered_order() {
        let n = NoteOut {
            who: "room:12345".into(),
            character: "Ima".into(),
            name: "Town Square".into(),
            changed: "2024-05-01 10:00".into(),
            text: "a\nb".into(),
            lines: vec!["a".into(), "b".into()],
        };
        assert_eq!(
            crate::wire::object(n.fields()).to_json(),
            r#"{"who":"room:12345","character":"Ima","name":"Town Square","changed":"2024-05-01 10:00","text":"a\nb","lines":["a","b"]}"#
        );
    }

    #[test]
    fn problem_variant_is_the_untagged_error_shape() {
        let out = NotesOut::Problem("notes.json cannot be read: boom".into());
        assert_eq!(
            crate::wire::object(out.fields()).to_json(),
            r#"{"problem":"notes.json cannot be read: boom"}"#
        );
    }

    #[test]
    fn read_round_trips_a_note_through_hydra_session() {
        let root = std::env::temp_dir().join(format!("hydra-web-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        // Nothing on disk yet: an empty list, not an error.
        assert_eq!(read(&root, "Ima"), NotesOut::Notes(Vec::new()));

        let mut notes = hydra_session::notes::Notes::new();
        notes.notes.push(hydra_session::notes::Note::new(
            "room:12345",
            "Town Square",
            "look up\nthe obelisk",
        ));
        hydra_session::notes::write(&root, &notes).unwrap();

        let page = read(&root, "Ima");
        let NotesOut::Notes(rows) = page else {
            panic!("expected notes, got {page:?}");
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Town Square");
        assert_eq!(rows[0].character, "Ima");
        assert_eq!(rows[0].lines, vec!["look up", "the obelisk"]);
        assert!(rows[0].text.contains("look up"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
