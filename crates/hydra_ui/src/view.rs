//! The play view: turning the model into something drawable.
//!
//! Recovered literals: `"obvious exits"`, `"obvious paths"` and the ten
//! direction words `north south east west northeast northwest southeast
//! southwest up down` (0x103980fe8 / 0x10398106a).  `PlayView::project`
//! @0x1019385e4 builds a `PlayView` from the game state and `PlayView::ui_with`
//! draws it (Phase 4): heading, description, exit row, prompt.

use hydra_model::state::GameState;

use crate::paint;
use crate::theme::file::Theme;
use crate::theme::Token;

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

    /// The description lines the window shows.
    ///
    /// The `Obvious exits:` / `Obvious paths:` caption is *not* repeated as body
    /// text: [`exits_in`] has already turned it into the exit row, which is what
    /// [`PlayView::ui`] draws instead.
    pub fn described(&self) -> Vec<&str> {
        self.description
            .lines()
            .filter(|line| !is_exit_line(line))
            .collect()
    }

    /// Phase 4 — draw the play view into `ui` with the built-in theme.
    pub fn ui(&self, ui: &mut egui::Ui) -> Laid {
        self.ui_with(ui, &Theme::default())
    }

    /// Phase 4 — draw the play view with `theme`'s palette.
    ///
    /// The widgets and their colours, all from the recovered data:
    ///
    /// * the room name is a `ui.heading` in [`Token::RoomName`] (`#d7ad63`);
    /// * the description is one `ui.colored_label` per line in
    ///   [`Token::RoomDesc`] (`#b4b4b4`), the exit caption removed — see
    ///   [`PlayView::described`];
    /// * the exit words are a `ui.horizontal` row of buttons in [`Token::Link`]
    ///   (`#477ab3`); they are the abbreviations [`exit_word`] gives, and a
    ///   button is what makes them clickable (`run::RunLink` carries the command);
    /// * the prompt is a `ui.colored_label` in [`Token::Command`] (`#477ab3`).
    ///
    /// Returns where each part landed, so a caller (or a test) can hit-test it.
    pub fn ui_with(&self, ui: &mut egui::Ui, theme: &Theme) -> Laid {
        let palette = &theme.palette;
        let mut laid = Laid::default();

        if !self.room.is_empty() {
            let title = ui.heading(
                egui::RichText::new(self.room.as_str())
                    .color(paint::ink(palette, Token::RoomName)),
            );
            laid.title = Some(title.rect);
        }
        for line in self.described() {
            // A blank line in the description is still a line: an empty label
            // would collapse and pull the paragraphs together, so it is drawn as
            // a single space.
            let label = ui.colored_label(
                paint::ink(palette, Token::RoomDesc),
                if line.is_empty() { " " } else { line },
            );
            laid.description.push(label.rect);
        }
        if !self.exits.is_empty() {
            let link = paint::ink(palette, Token::Link);
            ui.horizontal(|ui| {
                for word in &self.exits {
                    let response =
                        ui.add(egui::Button::new(egui::RichText::new(word.as_str()).color(link)));
                    laid.exits.push((word.clone(), response.rect));
                }
            });
        }
        if !self.prompt.is_empty() {
            let prompt =
                ui.colored_label(paint::ink(palette, Token::Command), self.prompt.as_str());
            laid.prompt = Some(prompt.rect);
        }
        laid
    }
}

/// Where [`PlayView::ui_with`] put each part of the view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    /// The room-name heading.
    pub title: Option<egui::Rect>,
    /// One rectangle per description line.
    pub description: Vec<egui::Rect>,
    /// The exit row: the word and the button it was drawn in.
    pub exits: Vec<(String, egui::Rect)>,
    /// The prompt line.
    pub prompt: Option<egui::Rect>,
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

/// Phase 4 — the play view is drawn with real widgets.  One headless frame with a
/// fixed `RawInput`; the assertions are on the galleys, their colours and the
/// buttons the exit row made.
#[cfg(test)]
mod drawing {
    use super::*;
    use egui::{Color32, Shape};

    fn frame(mut ui: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(800.0, 600.0),
        ));
        ctx.run_ui(input, ui)
    }

    fn texts(out: &egui::FullOutput) -> Vec<(String, Color32)> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(t) => {
                    let painted = t
                        .galley
                        .job
                        .sections
                        .first()
                        .map(|s| s.format.color)
                        .unwrap_or(Color32::PLACEHOLDER);
                    let colour = if painted == Color32::PLACEHOLDER {
                        t.fallback_color
                    } else {
                        painted
                    };
                    Some((t.galley.text().to_string(), colour))
                }
                _ => None,
            })
            .collect()
    }

    fn cave() -> PlayView {
        PlayView {
            room: "A Dim Cave".into(),
            description: "You are standing in a dim cave.\nA cold draught comes from below.\nObvious exits: north, southeast, up".into(),
            exits: exits_in("Obvious exits: north, southeast, up"),
            prompt: "s>".into(),
        }
    }

    #[test]
    fn the_description_drops_the_exit_caption() {
        let view = cave();
        assert_eq!(
            view.described(),
            vec!["You are standing in a dim cave.", "A cold draught comes from below."]
        );
        // The model itself keeps the caption verbatim.
        assert!(view.description.contains("Obvious exits: north"));
    }

    #[test]
    fn the_view_draws_heading_description_exits_and_prompt() {
        let view = cave();
        let theme = Theme::default();
        let mut laid = Laid::default();
        let out = frame(|ui| {
            laid = view.ui_with(ui, &theme);
        });
        let text = texts(&out);

        // The room name is a heading in `room_name` (#d7ad63).
        assert_eq!(laid.title.is_some(), true);
        assert_eq!(text[0].0, "A Dim Cave");
        assert_eq!(text[0].1, Color32::from_rgb(0xd7, 0xad, 0x63));

        // Two description lines, in `room_desc` (#b4b4b4), and no third
        // "Obvious exits:" line.
        assert_eq!(laid.description.len(), 2);
        assert_eq!(text[1].0, "You are standing in a dim cave.");
        assert_eq!(text[1].1, Color32::from_rgb(0xb4, 0xb4, 0xb4));
        assert_eq!(text[2].0, "A cold draught comes from below.");
        assert!(!text.iter().any(|(t, _)| t.starts_with("Obvious exits")));

        // Three exit words, abbreviated, in `link` (#477ab3) — one per button.
        assert_eq!(
            laid.exits.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>(),
            vec!["n", "se", "u"]
        );
        for (i, (word, rect)) in laid.exits.iter().enumerate() {
            assert_eq!(&text[3 + i].0, word);
            assert_eq!(text[3 + i].1, Color32::from_rgb(0x47, 0x7a, 0xb3));
            assert!(rect.width() > 0.0 && rect.height() > 0.0);
            // The row is horizontal: the buttons advance to the right.
            if i > 0 {
                assert!(rect.left() >= laid.exits[i - 1].1.left());
                assert!(rect.left() > laid.exits[i - 1].1.right() - rect.width());
            }
        }
        // A button is a filled rectangle behind each word.
        let buttons: Vec<_> = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Rect(r) if r.fill.a() > 0 => Some(r.rect),
                _ => None,
            })
            .collect();
        assert_eq!(buttons.len(), 3);
        for (i, (_, rect)) in laid.exits.iter().enumerate() {
            assert_eq!(buttons[i], *rect);
        }

        // The prompt is last, in `command` (#477ab3).
        let prompt = laid.prompt.unwrap();
        assert_eq!(text.last().unwrap().0, "s>");
        assert_eq!(text.last().unwrap().1, Color32::from_rgb(0x47, 0x7a, 0xb3));
        assert!(prompt.top() > laid.exits[0].1.bottom());
    }

    #[test]
    fn an_empty_view_draws_nothing() {
        let view = PlayView::default();
        let theme = Theme::default();
        let mut laid = Some(Laid::default());
        let out = frame(|ui| {
            laid = Some(view.ui_with(ui, &theme));
        });
        let laid = laid.unwrap();
        assert_eq!(laid, Laid::default());
        assert!(out.shapes.is_empty());
    }

    #[test]
    fn the_theme_palette_is_what_colours_the_view() {
        let view = cave();
        let mut theme = Theme::default();
        // `Palette::get` @0x1018478b8 indexes the 95-colour `bare` array through
        // the token table (the 95-byte identity map at 0x103992028), so writing
        // that slot is what recolours the room name.
        let at = Token::RoomName.index() as usize;
        theme.palette.bare[at] = crate::theme::rgb(0x00, 0xff, 0xff);
        let out = frame(|ui| {
            view.ui_with(ui, &theme);
        });
        let text = texts(&out);
        assert_eq!(text[0].0, "A Dim Cave");
        assert_eq!(text[0].1, Color32::from_rgb(0x00, 0xff, 0xff));
    }

    #[test]
    fn ui_is_the_built_in_theme() {
        let view = cave();
        let mut plain = Laid::default();
        frame(|ui| {
            plain = view.ui(ui);
        });
        let mut themed = Laid::default();
        frame(|ui| {
            themed = view.ui_with(ui, &Theme::default());
        });
        assert_eq!(plain, themed);
        assert_eq!(plain.exits.len(), 3);
    }

    #[test]
    fn blank_description_lines_still_take_a_row() {
        let view = PlayView {
            room: "Nowhere".into(),
            description: "one\n\ntwo".into(),
            ..PlayView::default()
        };
        let mut laid = Laid::default();
        let out = frame(|ui| {
            laid = view.ui(ui);
        });
        assert_eq!(laid.description.len(), 3);
        // The middle empty line is drawn as a space, not skipped.
        assert_eq!(texts(&out)[2].0, " ");
        assert!(laid.description[1].height() > 0.0);
    }
}
