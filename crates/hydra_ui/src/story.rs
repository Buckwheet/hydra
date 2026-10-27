//! The story window: scrollback, streams, highlights and prompts.
//!
//! Recovered symbol sizes: `Story::painted` 9.3 KB, `Story::hear` 8.1 KB,
//! `Story::alert` 4.4 KB, `Story::world` 3.8 KB, `Story::push` 3.2 KB,
//! `Story::alerts_within` 1.4 KB, `Story::settle` 696 B, `Story::see_at` 488 B,
//! `Story::typing` 488 B, `Story::tell` 452 B, `Story::highlight_with` 576 B,
//! `Story::scrollback` 276 B, `Story::missed` 112 B, `Story::see` 108 B.
//! `Prompts::{line, draws}` are 100 B and 148 B.
//!
//! The drop glue names `Shown`, `Heard` and `Call`, and the literals mention
//! `"lookout"` and `"combat"` (0x10399299e) — the two side panels a line can be
//! copied to.
//!
//! Phase 4 added the drawing layer: [`Story::paint`] / [`Story::paint_with`] /
//! [`Story::paint_into`] lay the scrollback out with `epaint` and paint every run
//! in its token's recovered colour.  See [`Ink`] and [`Painted`].

pub mod scrollback;
pub mod stamp;
pub mod streams;

use std::collections::VecDeque;
use std::sync::Arc;

use egui::text::{Galley, LayoutJob, TextFormat};
use egui::{Color32, FontId, Painter, Pos2, Rect, Ui};
use hydra_protocol::runs::Runs;

use crate::highlight::matcher::Matcher;
use crate::paint;
use crate::run::{StyledRun, RunLink};
use crate::theme::file::Theme;
use crate::theme::{Palette, Token};

/// A line as the window shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    /// The styled runs.
    pub runs: Vec<StyledRun>,
    /// When the line arrived (game seconds, when known).
    pub at: Option<u32>,
    /// The stream it belongs to, if any.
    pub stream: Option<String>,
}

impl Shown {
    /// The plain text of the line.
    pub fn plain(&self) -> String {
        StyledRun::plain(&self.runs)
    }

    /// The first link on the line.
    pub fn link(&self) -> Option<&RunLink> {
        self.runs.iter().find_map(|r| r.link.as_ref())
    }
}

/// A line the highlighter caught.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heard {
    /// The rule that fired.
    pub rule: String,
    /// The text that matched.
    pub text: String,
    /// The `sound` the rule asked for, if any.
    pub sound: Option<String>,
}

/// One of the side panels a line can be copied to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// The lookout panel.
    Lookout,
    /// The combat panel.
    Combat,
}

impl Call {
    pub fn name(self) -> &'static str {
        match self {
            Call::Lookout => "lookout",
            Call::Combat => "combat",
        }
    }

    pub fn named(name: &str) -> Option<Call> {
        Some(match name {
            "lookout" => Call::Lookout,
            "combat" => Call::Combat,
            _ => return None,
        })
    }
}

/// The prompt the game is waiting at.
///
/// `Prompts::draws` @0x10185af30 compares the trimmed prompt against the one it
/// last drew and returns `true` when it changed; `Prompts::line` @0x10185aecc
/// notes whether any prompt line had content, which suppresses the redraw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prompts {
    last: Option<String>,
    drawn: bool,
}

impl Prompts {
    pub fn new() -> Prompts {
        Prompts::default()
    }

    /// `@0x10185aecc` — note whether any of `lines` has content.
    pub fn line(&mut self, lines: &[String]) {
        self.drawn = lines.iter().any(|l| !l.trim().is_empty()) || self.drawn;
    }

    /// `@0x10185af30` — does `prompt` need drawing?
    ///
    /// `false` when the trimmed prompt equals the last one and nothing new
    /// arrived; otherwise the prompt is remembered and `true` is returned.
    pub fn draws(&mut self, prompt: &str) -> bool {
        let trimmed = prompt.trim();
        let same = self
            .last
            .as_deref()
            .map(|last| last == trimmed)
            .unwrap_or(false);
        let needs = !(same && !self.drawn);
        if needs {
            self.last = Some(trimmed.to_string());
        }
        self.drawn = false;
        needs
    }

    /// The prompt last drawn.
    pub fn shown(&self) -> Option<&str> {
        self.last.as_deref()
    }
}

/// The main story scrollback.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Story {
    lines: VecDeque<Shown>,
    /// How many lines to keep.
    pub keep: usize,
    /// Lines the highlighter caught.
    pub heard: Vec<Heard>,
    /// Lines that arrived while the window was scrolled back.
    pub missed: usize,
    /// Whether the window is following the tail.
    pub typing: bool,
}

impl Story {
    /// The default scrollback depth.
    pub const KEEP: usize = 5000;

    pub fn new() -> Story {
        Story {
            lines: VecDeque::new(),
            keep: Story::KEEP,
            heard: Vec::new(),
            missed: 0,
            typing: true,
        }
    }

    /// `true` when nothing has been seen.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The lines, oldest first.
    pub fn lines(&self) -> Vec<&Shown> {
        self.lines.iter().collect()
    }

    /// The last `n` lines as plain text.
    pub fn text(&self, n: usize) -> Vec<String> {
        let start = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(start).map(|l| l.plain()).collect()
    }

    /// `@0x101852c80` — the scrollback as a whole.
    pub fn scrollback(&self) -> &VecDeque<Shown> {
        &self.lines
    }

    /// `@0x101853548` — append a plain line.
    pub fn see(&mut self, text: impl Into<String>) -> &Shown {
        let runs = vec![StyledRun {
            text: text.into(),
            token: Some(Token::Text),
            color: Token::Text.bare(),
            bold: false,
            link: None,
        }];
        self.push(Shown {
            runs,
            at: None,
            stream: None,
        })
    }

    /// `@0x101855568` — append a line, trimming to [`Story::keep`].
    pub fn push(&mut self, line: Shown) -> &Shown {
        self.lines.push_back(line);
        if self.keep > 0 && self.lines.len() > self.keep {
            let drop = self.lines.len() - self.keep;
            for _ in 0..drop {
                self.lines.pop_front();
            }
        }
        if !self.typing {
            self.missed += 1;
        }
        self.lines.back().unwrap()
    }

    /// `@0x1018535b4` — append a protocol `Runs` line.
    pub fn hear(&mut self, runs: &Runs, palette: &Palette) -> &Shown {
        let line = Shown {
            runs: crate::run::game_line(runs, palette),
            at: None,
            stream: None,
        };
        self.push(line)
    }

    /// `@0x101853308` — apply the highlight rules to the newest line.
    pub fn highlight_with(&mut self, matcher: &Matcher) -> Vec<Heard> {
        let Some(line) = self.lines.back() else {
            return Vec::new();
        };
        let text = line.plain();
        let caught = matcher.check(&text);
        self.heard.extend(caught.iter().cloned());
        caught
    }

    /// `@0x1018561ec` — send a line to a side panel.
    pub fn tell(&mut self, which: Call, text: impl Into<String>) -> &Shown {
        let runs = vec![StyledRun {
            text: text.into(),
            token: Some(match which {
                Call::Lookout => Token::Lookout,
                Call::Combat => Token::Creature,
            }),
            color: match which {
                Call::Lookout => Token::Lookout.bare(),
                Call::Combat => Token::Creature.bare(),
            },
            bold: false,
            link: None,
        }];
        self.push(Shown {
            runs,
            at: None,
            stream: None,
        })
    }

    /// `@0x1018563b0` — raise an alert line.
    pub fn alert(&mut self, text: impl Into<String>) -> &Shown {
        let runs = vec![StyledRun {
            text: text.into(),
            token: Some(Token::Warning),
            color: Token::Warning.bare(),
            bold: true,
            link: None,
        }];
        self.push(Shown {
            runs,
            at: None,
            stream: None,
        })
    }

    /// `@0x101852d94` — the alerts in the last `secs` of game time.
    pub fn alerts_within(&self, now: u32, secs: u32) -> Vec<&Shown> {
        self.lines
            .iter()
            .filter(|l| l.runs.iter().any(|r| r.token == Some(Token::Warning)))
            .filter(|l| match l.at {
                Some(at) => now.saturating_sub(at) <= secs,
                None => false,
            })
            .collect()
    }

    /// `@0x1018574b0` — the world panel's copy of the story.
    pub fn world(&self, _palette: &Palette) -> Vec<(Token, String)> {
        self.lines
            .iter()
            .map(|l| {
                let token = l.runs.first().and_then(|r| r.token).unwrap_or(Token::Text);
                (token, StyledRun::plain(&l.runs))
            })
            .collect()
    }

    /// `@0x1018583d4` — how many lines arrived while scrolled back.
    pub fn missed(&self) -> usize {
        self.missed
    }

    /// `@0x1018583d4` — look at a line by index from the end.
    pub fn see_at(&self, back: usize) -> Option<&Shown> {
        let len = self.lines.len();
        if back >= len {
            None
        } else {
            self.lines.get(len - 1 - back)
        }
    }

    /// `@0x1018585bc` — stop following the tail.
    pub fn settle(&mut self) {
        self.typing = false;
    }

    /// `@0x101858874` — follow the tail again and clear the missed count.
    pub fn typing(&mut self) {
        self.typing = true;
        self.missed = 0;
    }

    /// `@0x101858a5c` — everything the window draws, coloured.
    pub fn painted(&self, palette: &Palette) -> Vec<(Token, String)> {
        let mut out = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            for run in &line.runs {
                let token = run.token.unwrap_or(Token::Text);
                let _ = palette.get(token);
                out.push((token, run.text.clone()));
            }
        }
        out
    }

    /// Phase 4 — `@0x101858a5c` drawn: paint the scrollback into `ui`.
    ///
    /// Uses the built-in theme and no prompt; see [`Story::paint_with`].
    pub fn paint(&self, ui: &mut Ui) -> Painted {
        let theme = Theme::default();
        let ink = Ink::of(&theme, ui);
        self.paint_with(ui, &ink)
    }

    /// Phase 4 — paint the scrollback into `ui` with `ink`'s palette, face and
    /// prompt.
    ///
    /// The whole available rectangle is claimed (the story window is the one that
    /// scrolls) and everything is painted bottom-anchored, newest line at the
    /// bottom, so a growing log does not jump.  See [`Story::paint_into`].
    pub fn paint_with(&self, ui: &mut Ui, ink: &Ink<'_>) -> Painted {
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, egui::Sense::hover());
        self.paint_into(ui.painter(), rect, ink)
    }

    /// Phase 4 — paint the scrollback into `rect` of `painter`.
    ///
    /// Every run is laid out into one [`LayoutJob`] per line, so a line keeps its
    /// per-run colours inside a single galley; the colour of a run is
    /// [`Palette::get`] of its token (the fallback is the run's own resolved
    /// `color` when it carries no token) — the token colours are recovered data
    /// and are used exactly.
    ///
    /// State the model carries and this respects:
    ///
    /// * `typing == false` — the window is scrolled back, so the `missed` newest
    ///   lines are *not* drawn (they are what arrived since) and the count is
    ///   painted in [`Token::Warning`] at the bottom right;
    /// * `missed > 0` — the same count, once, as a number;
    /// * `ink.prompt` — painted as the final line, in [`Token::Command`], which
    ///   is the token `StyledRun::prompt` splits at and `run::prompt_coloured`
    ///   paints the prompt with (`// UNKNOWN:` the accent table
    ///   `Palette::prompt` is not used here: its default index is `2`, i.e.
    ///   [`Token::Raised`], a surface colour and not a text one).
    ///
    /// `// UNKNOWN:` the missed-count badge is a drawing decision — the binary's
    /// `Story::painted` keeps the count (see `Story::missed`) but its body is
    /// 924 bytes of clock arithmetic that does not say where the number goes.
    pub fn paint_into(&self, painter: &Painter, rect: Rect, ink: &Ink<'_>) -> Painted {
        let clip = painter.with_clip_rect(rect);
        let row = ink.row_height(painter);
        let wrap = rect.width().max(1.0);
        let mut out = Painted {
            rect,
            ..Painted::default()
        };

        // Scrolled back: the newest `missed` lines are the ones not on screen.
        let hidden = if self.typing {
            0
        } else {
            self.missed.min(self.lines.len())
        };

        // The sections of every line that may be drawn, oldest first, with the
        // prompt last.
        let mut items: Vec<(bool, Vec<Section>)> = Vec::with_capacity(self.lines.len() + 1);
        for line in self.lines.iter().take(self.lines.len() - hidden) {
            items.push((false, self.sections(line, ink)));
        }
        if let Some(prompt) = ink.prompt {
            if !prompt.trim().is_empty() {
                items.push((
                    true,
                    vec![Section {
                        token: Some(ink.prompt_token),
                        color: paint::ink(ink.palette, ink.prompt_token),
                        text: prompt.to_string(),
                    }],
                ));
            }
        }

        // Bottom-anchored: walk back from the newest item until the box is full.
        let mut laid: Vec<(bool, Rect, Arc<Galley>, Vec<Section>)> = Vec::new();
        let mut bottom = rect.bottom();
        for (is_prompt, sections) in items.into_iter().rev() {
            let galley = clip.layout_job(job(&sections, wrap, &ink.font));
            let height = galley.size().y.max(row);
            let top = bottom - height;
            // The box is full: the newest item is always drawn, even alone.
            if !laid.is_empty() && top < rect.top() {
                break;
            }
            let line = Rect::from_min_max(
                Pos2::new(rect.left(), top),
                Pos2::new(rect.right(), bottom),
            );
            laid.push((is_prompt, line, galley, sections));
            bottom = top;
            if bottom <= rect.top() {
                break;
            }
        }
        laid.reverse();

        for (is_prompt, line, galley, sections) in laid {
            clip.galley(line.min, galley, Color32::PLACEHOLDER);
            let drawn = PaintedLine {
                rect: line,
                runs: sections,
            };
            if is_prompt {
                out.prompt = Some(drawn);
            } else {
                out.lines.push(drawn);
            }
        }

        if hidden > 0 {
            let at = Pos2::new(rect.right() - 2.0, rect.bottom() - 1.0);
            let badge = clip.text(
                at,
                egui::Align2::RIGHT_BOTTOM,
                hidden.to_string(),
                ink.font.clone(),
                paint::ink(ink.palette, Token::Warning),
            );
            out.missed = Some((hidden, badge));
        }
        out
    }

    /// The coloured sections of one line: each run split at the highlight spans
    /// the rule set produces, with `:shortcode:` emoji expanded
    /// (`emoji::expanded` @0x101939150).
    fn sections(&self, shown: &Shown, ink: &Ink<'_>) -> Vec<Section> {
        let plain = shown.plain();
        let spans = match ink.highlight {
            Some(matcher) => crate::highlight::spans(&plain, matcher),
            None => Vec::new(),
        };
        let mut out: Vec<Section> = Vec::new();
        let mut at = 0usize;
        for run in &shown.runs {
            let start = at;
            let end = at + run.text.len();
            at = end;
            let base = match run.token {
                Some(token) => paint::ink(ink.palette, token),
                None => paint::color(run.color),
            };
            // The run's text, cut where a highlight span starts or ends.
            let mut cuts = vec![start, end];
            for span in &spans {
                for edge in [span.start, span.end] {
                    if edge > start && edge < end {
                        cuts.push(edge);
                    }
                }
            }
            cuts.sort_unstable();
            cuts.dedup();
            for pair in cuts.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let text = &plain[a..b];
                // The first span covering this piece recolours it.
                let color = spans
                    .iter()
                    .find(|s| s.start <= a && s.end >= b)
                    .and_then(|s| s.color)
                    .map(paint::color)
                    .unwrap_or(base);
                let text = crate::emoji::expanded(text).unwrap_or_else(|| text.to_string());
                if text.is_empty() {
                    continue;
                }
                out.push(Section {
                    token: run.token,
                    color,
                    text,
                });
            }
        }
        out
    }
}

/// One run of the text a line is painted from.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// The token the run came with, when it has one.
    pub token: Option<Token>,
    /// The colour it is painted in (the token's palette colour, or a highlight
    /// rule's `color` when one covers it).
    pub color: Color32,
    /// The text, after `:shortcode:` expansion.
    pub text: String,
}

/// One painted line.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintedLine {
    /// The row the line occupies, full width of the story box.
    pub rect: Rect,
    /// The sections it was painted from, left to right.
    pub runs: Vec<Section>,
}

impl PaintedLine {
    /// The plain text of the line as painted.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// What [`Story::paint_into`] drew.
#[derive(Clone, Debug, PartialEq)]
pub struct Painted {
    /// The box the story was painted into.
    pub rect: Rect,
    /// The scrollback lines, oldest first.
    pub lines: Vec<PaintedLine>,
    /// The prompt line, always the last thing painted.
    pub prompt: Option<PaintedLine>,
    /// How many lines arrived while scrolled back, and where the count went.
    pub missed: Option<(usize, Rect)>,
}

impl Default for Painted {
    fn default() -> Painted {
        Painted {
            rect: Rect::NOTHING,
            lines: Vec::new(),
            prompt: None,
            missed: None,
        }
    }
}

/// What the story window is painted with.
///
/// This is the drawing half only: the palette, the face, the leading, the prompt
/// and the rule set.  All of it comes from recovered data —
/// [`Ink::of`] reads the theme's `[type]` section, and the colours are
/// [`Palette::get`].
#[derive(Clone, Debug, PartialEq)]
pub struct Ink<'a> {
    /// The palette every run's colour is looked up in.
    pub palette: &'a Palette,
    /// The story face: `[type] story_font`/`story_size`, else `TextStyle::Body`.
    pub font: FontId,
    /// Extra leading between lines: `[type] line_spacing` (0 when unset).
    pub line_spacing: f32,
    /// The prompt to paint after the scrollback.
    pub prompt: Option<&'a str>,
    /// The token the prompt is painted in.
    pub prompt_token: Token,
    /// The rules whose matches recolour their span (`highlight::spans`).
    pub highlight: Option<&'a Matcher>,
}

impl<'a> Ink<'a> {
    /// The ink a theme gives at the `ui`'s current style.
    pub fn of(theme: &'a Theme, ui: &Ui) -> Ink<'a> {
        Ink {
            palette: &theme.palette,
            font: paint::story_font(theme, ui),
            line_spacing: paint::line_spacing(theme),
            prompt: None,
            prompt_token: Token::Command,
            highlight: None,
        }
    }

    /// The same ink, with a prompt line.
    pub fn prompt(mut self, prompt: &'a str) -> Ink<'a> {
        self.prompt = Some(prompt);
        self
    }

    /// The same ink, recolouring the matches of `matcher`.
    pub fn highlighted(mut self, matcher: &'a Matcher) -> Ink<'a> {
        self.highlight = Some(matcher);
        self
    }

    /// The height of one blank line: the face's row height plus the leading.
    pub fn row_height(&self, painter: &Painter) -> f32 {
        painter.fonts_mut(|f| f.row_height(&self.font)) + self.line_spacing.max(0.0)
    }
}

/// One laid-out line: its sections wrapped to `wrap` in `font`.
fn job(sections: &[Section], wrap: f32, font: &FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    for section in sections {
        job.append(
            &section.text,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: section.color,
                ..Default::default()
            },
        );
    }
    job.wrap.max_width = wrap;
    job
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    #[test]
    fn calls_are_the_two_side_panels() {
        assert_eq!(Call::named("lookout"), Some(Call::Lookout));
        assert_eq!(Call::named("combat"), Some(Call::Combat));
        assert_eq!(Call::named("nope"), None);
        assert_eq!(Call::Combat.name(), "combat");
    }

    #[test]
    fn prompts_only_redraw_when_they_change() {
        let mut p = Prompts::new();
        assert!(p.draws("s>"));
        // Same prompt, nothing new: no redraw.
        assert!(!p.draws(" s> "));
        // A new prompt redraws.
        assert!(p.draws("You have a new prompt."));
        assert_eq!(p.shown(), Some("You have a new prompt."));
        // A new line forces a redraw even when the prompt is unchanged.
        p.line(&["something".to_string()]);
        assert!(p.draws("You have a new prompt."));
    }

    #[test]
    fn prompt_lines_without_content_do_not_force_a_redraw() {
        let mut p = Prompts::new();
        p.draws("s>");
        p.line(&["   ".to_string(), String::new()]);
        assert!(!p.draws("s>"));
    }

    #[test]
    fn pushing_trims_to_keep() {
        let mut s = Story::new();
        s.keep = 3;
        for i in 0..5 {
            s.see(i.to_string());
        }
        assert_eq!(s.text(10), vec!["2", "3", "4"]);
    }

    #[test]
    fn alerts_and_missed_lines() {
        let mut s = Story::new();
        s.alert("danger");
        s.see("plain");
        assert_eq!(s.missed(), 0);
        s.settle();
        s.see("while scrolled");
        assert_eq!(s.missed(), 1);
        s.typing();
        assert_eq!(s.missed(), 0);
    }

    #[test]
    fn see_at_walks_backwards() {
        let mut s = Story::new();
        s.see("a");
        s.see("b");
        assert_eq!(s.see_at(0).unwrap().plain(), "b");
        assert_eq!(s.see_at(1).unwrap().plain(), "a");
        assert!(s.see_at(2).is_none());
    }

    #[test]
    fn telling_uses_the_panel_token() {
        let mut s = Story::new();
        s.tell(Call::Lookout, "invaders");
        assert_eq!(s.lines()[0].runs[0].token, Some(Token::Lookout));
        s.tell(Call::Combat, "a troll");
        assert_eq!(s.lines()[1].runs[0].token, Some(Token::Creature));
    }

    #[test]
    fn hearing_a_protocol_run_list() {
        let p = Palette::bare();
        let runs = Runs::from_vec(vec![hydra_protocol::runs::Run::new("hello")]);
        let mut s = Story::new();
        s.hear(&runs, &p);
        assert_eq!(s.text(1), vec!["hello"]);
        assert_eq!(s.painted(&p).len(), 1);
    }
}

/// Phase 4 — the story is painted, not just modelled.  Each test runs one
/// headless `egui` frame with a fixed `RawInput` and asserts on the galleys and
/// colours that came out.
#[cfg(test)]
mod drawing {
    use super::*;
    use crate::highlight::file::Rule;
    use crate::theme::rgb;
    use egui::Vec2;

    /// Run one frame with an 800×600 screen.
    fn frame(mut ui: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)));
        ctx.run_ui(input, ui)
    }

    fn texts(out: &egui::FullOutput) -> Vec<(String, Color32)> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => {
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

    /// The box every test paints into.
    const BOX: Rect = Rect {
        min: Pos2 { x: 0.0, y: 0.0 },
        max: Pos2 { x: 400.0, y: 100.0 },
    };

    #[test]
    fn a_line_is_painted_in_its_token_colour() {
        let theme = Theme::default();
        let mut story = Story::new();
        story.see("A dark cave.");
        let mut seen = Painted::default();
        let out = frame(|ui| {
            let ink = Ink::of(&theme, ui);
            seen = story.paint_into(ui.painter(), BOX, &ink);
        });
        assert_eq!(seen.rect, BOX);
        assert_eq!(seen.lines.len(), 1);
        assert_eq!(seen.lines[0].text(), "A dark cave.");
        // `Story::see` pushes a `Token::Text` run, and `text` is #b4b4b4.
        assert_eq!(seen.lines[0].runs[0].token, Some(Token::Text));
        assert_eq!(seen.lines[0].runs[0].color, Color32::from_rgb(0xb4, 0xb4, 0xb4));
        // Bottom-anchored: the only line ends at the bottom of the box.
        assert_eq!(seen.lines[0].rect.bottom(), BOX.bottom());
        assert_eq!(seen.lines[0].rect.width(), BOX.width());
        assert!(seen.prompt.is_none());
        assert!(seen.missed.is_none());

        // …and the galley says the same thing.
        let text = texts(&out);
        assert_eq!(text.len(), 1);
        assert_eq!(text[0].0, "A dark cave.");
        assert_eq!(text[0].1, Color32::from_rgb(0xb4, 0xb4, 0xb4));
    }

    #[test]
    fn each_run_keeps_its_own_token_colour_inside_one_galley() {
        let theme = Theme::default();
        let mut story = Story::new();
        // A room name followed by a creature, as `hear` builds them.
        story.push(Shown {
            runs: vec![
                StyledRun {
                    text: "a ".into(),
                    token: Some(Token::Text),
                    color: Token::Text.bare(),
                    bold: false,
                    link: None,
                },
                StyledRun {
                    text: "troll".into(),
                    token: Some(Token::Creature),
                    color: Token::Creature.bare(),
                    bold: false,
                    link: None,
                },
            ],
            at: None,
            stream: None,
        });
        let mut sections = Vec::new();
        frame(|ui| {
            let ink = Ink::of(&theme, ui);
            let painted = story.paint_into(ui.painter(), BOX, &ink);
            sections = painted.lines[0].runs.clone();
        });
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].text, "a ");
        assert_eq!(sections[0].color, Color32::from_rgb(0xb4, 0xb4, 0xb4));
        assert_eq!(sections[1].text, "troll");
        assert_eq!(sections[1].color, Color32::from_rgb(0xbd, 0x8c, 0xff));
    }

    #[test]
    fn the_prompt_is_the_last_line() {
        let theme = Theme::default();
        let mut story = Story::new();
        story.see("You are standing in a dim cave.");
        story.see("Obvious exits: north.");
        let mut seen = Painted::default();
        let out = frame(|ui| {
            let ink = Ink::of(&theme, ui).prompt("s>");
            seen = story.paint_into(ui.painter(), BOX, &ink);
        });
        assert_eq!(seen.lines.len(), 2);
        let prompt = seen.prompt.clone().unwrap();
        assert_eq!(prompt.text(), "s>");
        assert_eq!(prompt.runs[0].token, Some(Token::Command));
        // `Token::Command` is #477ab3.
        assert_eq!(prompt.runs[0].color, Color32::from_rgb(0x47, 0x7a, 0xb3));
        // It is drawn below both story lines…
        assert_eq!(prompt.rect.bottom(), BOX.bottom());
        assert!(seen.lines[1].rect.bottom() <= prompt.rect.top());
        // …and it is the last text shape in the frame.
        let text = texts(&out);
        assert_eq!(text.len(), 3);
        assert_eq!(text[2].0, "s>");
        assert_eq!(text[2].1, Color32::from_rgb(0x47, 0x7a, 0xb3));
    }

    #[test]
    fn scrolled_back_lines_are_not_drawn_and_the_count_is() {
        let theme = Theme::default();
        let mut story = Story::new();
        story.see("first");
        story.settle();
        story.see("second");
        story.see("third");
        assert_eq!(story.missed(), 2);
        let mut seen = Painted::default();
        let out = frame(|ui| {
            let ink = Ink::of(&theme, ui);
            seen = story.paint_into(ui.painter(), BOX, &ink);
        });
        // Only what was on screen when the window was scrolled back.
        assert_eq!(seen.lines.len(), 1);
        assert_eq!(seen.lines[0].text(), "first");
        let (count, badge) = seen.missed.unwrap();
        assert_eq!(count, 2);
        assert_eq!(badge.right(), BOX.right() - 2.0);
        // The badge is the count, in `warning`.
        let text = texts(&out);
        assert_eq!(text.len(), 2);
        assert_eq!(text[1].0, "2");
        assert_eq!(text[1].1, Color32::from_rgb(0xd7, 0xad, 0x63));

        // Following the tail again draws all three and no badge.
        let mut story = story;
        story.typing();
        frame(|ui| {
            let ink = Ink::of(&theme, ui);
            let painted = story.paint_into(ui.painter(), BOX, &ink);
            assert_eq!(painted.lines.len(), 3);
            assert!(painted.missed.is_none());
        });
    }

    #[test]
    fn a_full_box_keeps_the_newest_lines() {
        let theme = Theme::default();
        let mut story = Story::new();
        for i in 0..40 {
            story.see(format!("line {i}"));
        }
        let mut seen = Painted::default();
        let short = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 40.0));
        frame(|ui| {
            let ink = Ink::of(&theme, ui);
            seen = story.paint_into(ui.painter(), short, &ink);
        });
        assert!(seen.lines.len() < 40, "the box only holds a few lines");
        assert!(!seen.lines.is_empty());
        // The newest line is the last one drawn and sits on the bottom edge.
        assert_eq!(seen.lines.last().unwrap().text(), "line 39");
        assert_eq!(seen.lines.last().unwrap().rect.bottom(), short.bottom());
        // Nothing was drawn above the box.
        assert!(seen.lines.iter().all(|l| l.rect.top() >= short.top()));
    }

    #[test]
    fn highlight_spans_are_painted_in_the_rules_colour() {
        let theme = Theme::default();
        let matcher = Matcher::new(&[Rule {
            pattern: "orc".into(),
            color: Some(rgb(0xff, 0x00, 0x00)),
            ..Rule::default()
        }]);
        let mut story = Story::new();
        story.see("an orc arrives");
        let mut sections = Vec::new();
        frame(|ui| {
            let ink = Ink::of(&theme, ui).highlighted(&matcher);
            sections = story.paint_into(ui.painter(), BOX, &ink).lines[0].runs.clone();
        });
        let texts: Vec<&str> = sections.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["an ", "orc", " arrives"]);
        assert_eq!(sections[0].color, Color32::from_rgb(0xb4, 0xb4, 0xb4));
        // The rule's `color` wins over the token colour.
        assert_eq!(sections[1].color, Color32::from_rgb(0xff, 0x00, 0x00));
        assert_eq!(sections[2].color, Color32::from_rgb(0xb4, 0xb4, 0xb4));

        // A rule with no colour leaves the token colour alone.
        let matcher = Matcher::new(&[Rule {
            pattern: "orc".into(),
            ..Rule::default()
        }]);
        frame(|ui| {
            let ink = Ink::of(&theme, ui).highlighted(&matcher);
            let painted = story.paint_into(ui.painter(), BOX, &ink);
            assert!(painted.lines[0]
                .runs
                .iter()
                .all(|s| s.color == Color32::from_rgb(0xb4, 0xb4, 0xb4)));
        });
    }

    #[test]
    fn shortcodes_are_expanded_before_painting() {
        let theme = Theme::default();
        let mut story = Story::new();
        story.see("hi :smile:");
        let mut seen = Painted::default();
        frame(|ui| {
            let ink = Ink::of(&theme, ui);
            seen = story.paint_into(ui.painter(), BOX, &ink);
        });
        // `emoji::lookup("smile")` is U+1F604, the recovered table's glyph.
        assert_eq!(seen.lines[0].text(), "hi \u{1f604}");
        // The model itself is untouched.
        assert_eq!(story.text(1), vec!["hi :smile:"]);
    }

    #[test]
    fn paint_claims_the_available_box() {
        let mut story = Story::new();
        story.see("hello");
        let mut seen = Painted::default();
        frame(|ui| {
            seen = story.paint(ui);
        });
        // `paint` uses the whole available rectangle of an 800×600 frame.
        assert!(seen.rect.width() > 0.0);
        assert_eq!(seen.lines.len(), 1);
        assert_eq!(seen.lines[0].rect.bottom(), seen.rect.bottom());
    }
}

