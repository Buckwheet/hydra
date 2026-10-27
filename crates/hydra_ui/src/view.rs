//! The play view: turning the model into something drawable.
//!
//! Recovered literals: `"obvious exits"`, `"obvious paths"` and the ten
//! direction words `north south east west northeast northwest southeast
//! southwest up down` (0x103980fe8 / 0x10398106a).  `PlayView::project`
//! @0x1019385e4 builds a `PlayView` from the game state; the original hands
//! `egui` a ready-made projection, which is why there is no drawing here.

use hydra_model::state::GameState;

/// The short form of a direction word.
///
/// `@0x101938a58` — the room description's exit list uses the abbreviated
/// forms; the recovered vocabulary is exactly the ten words above.  Anything
/// else comes back unchanged (`@0x1019385b4` is a 48-byte trampoline that
/// returns its argument).
pub fn exit_word(direction: &str) -> &str {
    let trimmed = direction.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "north" => "n",
        "south" => "s",
        "east" => "e",
        "west" => "w",
        "northeast" => "ne",
        "northwest" => "nw",
        "southeast" => "se",
        "southwest" => "sw",
        "up" => "u",
        "down" => "d",
        _ => trimmed,
    }
}

/// `true` for a line that announces the room's exits.
pub fn is_exit_line(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with("Obvious exits:") || l.starts_with("Obvious paths:")
}

/// The exit words in a room description, abbreviated and sorted.
///
/// The description holds a single `Obvious exits: a, b, c` caption; this pulls
/// the words out of it.
pub fn exits_in(description: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in description.lines() {
        let line = line.trim();
        let rest = match line
            .strip_prefix("Obvious exits:")
            .or_else(|| line.strip_prefix("Obvious paths:"))
        {
            Some(rest) => rest,
            None => continue,
        };
        for word in rest.split(',') {
            let word = word.trim().trim_end_matches('.').trim();
            if word.is_empty() || word.eq_ignore_ascii_case("none") {
                continue;
            }
            out.push(exit_word(word).to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The room's description, as one paragraph.
///
/// `@0x1019385b4` — a 48-byte wrapper that concatenates the room title and its
/// description.
pub fn room_description(state: &GameState) -> String {
    let mut out = String::new();
    if !state.room.title.is_empty() {
        out.push_str(&state.room.title);
        out.push('\n');
    }
    out.push_str(&state.room.description);
    out
}

/// What the play window draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayView {
    /// The current room's name.
    pub room: String,
    /// The room's description, verbatim.
    pub description: String,
    /// The exit words, abbreviated and sorted.
    pub exits: Vec<String>,
    /// The prompt currently shown, from the `prompt` component.
    pub prompt: String,
}

impl PlayView {
    /// `@0x1019385e4` — project the game state for one frame.
    ///
    /// `// UNKNOWN:` the recovered function fills an `egui`-shaped output; the
    /// text-only projection here is the part that is model logic rather than
    /// drawing.
    pub fn project(state: &GameState) -> PlayView {
        PlayView {
            room: state.room.title.clone(),
            description: state.room.description.clone(),
            exits: exits_in(&state.room.description),
            prompt: state
                .room
                .components
                .get("prompt")
                .cloned()
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_words_abbreviate() {
        assert_eq!(exit_word("north"), "n");
        assert_eq!(exit_word("south"), "s");
        assert_eq!(exit_word("east"), "e");
        assert_eq!(exit_word("west"), "w");
        assert_eq!(exit_word("northeast"), "ne");
        assert_eq!(exit_word("northwest"), "nw");
        assert_eq!(exit_word("southeast"), "se");
        assert_eq!(exit_word("southwest"), "sw");
        assert_eq!(exit_word("up"), "u");
        assert_eq!(exit_word("down"), "d");
        assert_eq!(exit_word(" NORth "), "n");
    }

    #[test]
    fn unknown_directions_pass_through() {
        assert_eq!(exit_word("out"), "out");
        assert_eq!(exit_word("  out  "), "out");
    }

    #[test]
    fn exit_lines_are_recognised() {
        assert!(is_exit_line("Obvious exits: north, south"));
        assert!(is_exit_line("  Obvious paths: none"));
        assert!(!is_exit_line("Obvious exits are not a thing here"));
    }

    #[test]
    fn parses_exit_lists() {
        let d = "You are standing in a dim cave.\nObvious exits: north, southeast, up";
        assert_eq!(exits_in(d), vec!["n", "se", "u"]);
    }

    #[test]
    fn projecting_an_empty_state_is_empty() {
        let view = PlayView::project(&GameState::default());
        assert!(view.exits.is_empty());
        assert_eq!(view.room, "");
        assert_eq!(view.description, "");
    }
}
