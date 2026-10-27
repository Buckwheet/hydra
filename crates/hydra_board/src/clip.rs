//! Taking a clip out of the story and keeping it against the room.
//!
//! **ESTABLISHED** (`0x1014710e0`, `0x10147193c`): the story is locked, trimmed,
//! turned into a `hydra_session::notes::Clip` with
//! `notes::Clip::of_room(room, title, text, exits, day, line)` and handed to the
//! session with `command::SessionHandle::set_clip`.

use hydra_session::command::SessionHandle;
use hydra_session::notes::Clip;


/// Keep the lines just copied out of the story as a clip for this room.
///
/// **PARTLY** (`0x1014710e0`): the binary trims the copied text, builds the clip
/// for the room and stores it with `set_clip`.  The room's exits and the log
/// position come from the session; UNKNOWN how a partially copied line is
/// worded, so the text is trimmed and any trailing space dropped.
pub fn keep_copied(
    session: &SessionHandle,
    room: &str,
    title: &str,
    copied: &str,
    exits: &[String],
) -> Option<Clip> {
    let text = copied
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    if text.is_empty() || room.is_empty() {
        return None;
    }
    let clip = Clip::of_room(room, title, text, exits, &day(), line_of(copied, &text));
    session.set_clip(Some(clip.clone()));
    Some(clip)
}

/// The clip a set of story lines makes for this room.
///
/// **ESTABLISHED** (`0x10147193c`): the clip is built from the lines and its
/// page link is the folder's web address; UNKNOWN how the binary decides which
/// lines are "the story", so all of them are used.
pub fn of_story(
    room: &str,
    title: &str,
    lines: &[String],
    exits: &[String],
    day: &str,
    line: u64,
) -> Option<Clip> {
    let text = lines
        .iter()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    if text.is_empty() || room.is_empty() {
        return None;
    }
    Some(Clip::of_room(room, title, text, exits, day, line))
}

/// Today's day key.
fn day() -> String {
    hydra_session::notes::stamp()
}

/// How many lines into the story the clip starts.
fn line_of(copied: &str, text: &str) -> u64 {
    copied
        .lines()
        .position(|line| line.contains(text.lines().next().unwrap_or("")))
        .map(|at| at as u64 + 1)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clip_trims_the_copied_text() {
        let clip = of_story(
            "room-1",
            "The Inn",
            &["  a line  ".to_string(), String::new(), "another".to_string()],
            &["out".to_string()],
            "2024-05-01",
            7,
        )
        .unwrap();
        assert_eq!(clip.room, "room-1");
        assert_eq!(clip.title, "The Inn");
        // The exits line is appended by `Clip::of_room`.
        assert_eq!(clip.note.text(), "a line\n\nanother\nObvious exits: out");
        assert_eq!(
            clip.note.clip,
            Some(hydra_session::notes::Clipped {
                day: "2024-05-01".into(),
                line: 7
            })
        );
    }

    #[test]
    fn an_empty_clip_is_not_made() {
        assert!(of_story("room-1", "The Inn", &[], &[], "2024-05-01", 1).is_none());
        assert!(of_story("", "The Inn", &["x".to_string()], &[], "2024-05-01", 1).is_none());
    }
}
