//! The hotbar: a grid of buttons that send commands, run macros or count down.
//!
//! Recovered field names (serde pool at 0x103993400 / 0x103994dea):
//!
//! ```text
//! struct Bar { title columns button_width button_height icon_size buttons }
//! struct Style { label fg bg dim bar corner run }
//! struct Button { id command macro hotkey tooltip countdown states
//!                 default_style icon_mode }
//! struct HotbarFile { recipe names prompt shape frame }   // theme reference
//! struct ButtonState with 4 elements
//! struct Countdown …
//! struct variant Countdown::Effect
//! hotbars.toml
//! variant identifier  roundtime casttime effect  variant index 0 <= i < 3
//! ```
//!
//! Phase 4 added [`Bar::ui`] / [`Bar::ui_with`]: a real grid of cells at
//! `column * button_width`, `row * button_height`, painted from the same
//! `Button::look` styles the model recovers.  See [`Look`], [`Grid`], [`Cell`].

pub mod icon;

use std::collections::BTreeMap;

use hydra_protocol::runs::Runs;

use crate::layer::Over;
use crate::paint;
use crate::theme::file::Theme;
use crate::theme::{Palette, Rgb, Token};
use crate::toml::{self, Table, Value};

use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2};

/// What a button's readout counts down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Roundtime,
    Casttime,
    Effect,
}

impl ButtonState {
    /// The wire name.
    pub fn name(self) -> &'static str {
        match self {
            ButtonState::Roundtime => "roundtime",
            ButtonState::Casttime => "casttime",
            ButtonState::Effect => "effect",
        }
    }

    pub fn named(name: &str) -> Option<ButtonState> {
        Some(match name {
            "roundtime" => ButtonState::Roundtime,
            "casttime" => ButtonState::Casttime,
            "effect" => ButtonState::Effect,
            _ => return None,
        })
    }
}

/// What a button shows when it is idle.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Countdown {
    /// The roundtime left.
    #[default]
    Roundtime,
    /// The casttime left.
    Casttime,
    /// The time left on a named effect (a struct variant with 3 fields).
    Effect {
        name: String,
        category: String,
        seconds: u32,
    },
}

impl Countdown {
    /// `@0x1018d5e10` — the seconds left, given the game state's clocks.
    ///
    /// 78 KB in the binary: it drives the countdown readout *and* the button's
    /// repaint.  `// UNKNOWN:` only the clock arithmetic is reproduced.
    pub fn left(
        &self,
        roundtime: Option<u32>,
        casttime: Option<u32>,
        effects: &BTreeMap<String, u32>,
    ) -> u32 {
        match self {
            Countdown::Roundtime => roundtime.unwrap_or(0),
            Countdown::Casttime => casttime.unwrap_or(0),
            Countdown::Effect { name, .. } => effects.get(name).copied().unwrap_or(0),
        }
    }
}

/// The look of a button.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub label: String,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    /// Draw the button dimmed.
    pub dim: bool,
    /// The bar measure this style is laid out with.
    pub bar: Option<f32>,
    /// The corner radius.
    pub corner: Option<f32>,
    /// The run length.
    pub run: Option<f32>,
}

/// One hotbar button.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Button {
    pub id: String,
    pub command: Option<String>,
    pub r#macro: Option<String>,
    pub hotkey: Option<String>,
    pub tooltip: Option<String>,
    pub countdown: Option<Countdown>,
    /// Per-state styles (`roundtime`, `casttime`, `effect`).
    pub states: BTreeMap<String, Style>,
    /// The style when no state applies.
    pub default_style: Style,
    /// How the icon is drawn.
    pub icon_mode: Option<String>,
}

impl Button {
    /// `@0x1018d5810` — the style to draw the button with.
    pub fn look(&self, state: Option<ButtonState>, palette: &Palette) -> Style {
        let mut style = self.default_style.clone();
        if let Some(state) = state {
            if let Some(s) = self.states.get(state.name()) {
                style = style.over(s);
            }
        }
        if style.fg.is_none() {
            style.fg = Some(palette.get(Token::Text));
        }
        if style.bg.is_none() {
            style.bg = Some(palette.get(Token::Raised));
        }
        if style.label.is_empty() {
            style.label = self
                .tooltip
                .clone()
                .or_else(|| self.command.clone())
                .unwrap_or_else(|| self.id.clone());
        }
        style
    }

    /// The command the button sends.
    pub fn command(&self) -> Option<&str> {
        self.command.as_deref().or(self.r#macro.as_deref())
    }
}

impl Over for Style {
    fn over(&self, under: &Self) -> Self {
        Style {
            label: if self.label.is_empty() {
                under.label.clone()
            } else {
                self.label.clone()
            },
            fg: self.fg.or(under.fg),
            bg: self.bg.or(under.bg),
            dim: self.dim || under.dim,
            bar: self.bar.or(under.bar),
            corner: self.corner.or(under.corner),
            run: self.run.or(under.run),
        }
    }
    fn differing(&self, base: &Self) -> Self {
        if self == base {
            Style::default()
        } else {
            self.clone()
        }
    }
    fn is_empty(&self) -> bool {
        *self == Style::default()
    }
    fn from_value(v: &Value) -> Option<Self> {
        let t = v.as_table()?;
        Some(Style {
            label: t.get("label").and_then(|v| v.as_str()).unwrap_or("").into(),
            fg: t.get("fg").and_then(|v| v.as_str()).and_then(crate::theme::parse_hex),
            bg: t.get("bg").and_then(|v| v.as_str()).and_then(crate::theme::parse_hex),
            dim: t.get("dim").and_then(|v| v.as_bool()).unwrap_or(false),
            bar: t.get("bar").and_then(|v| v.as_f64()).map(|v| v as f32),
            corner: t.get("corner").and_then(|v| v.as_f64()).map(|v| v as f32),
            run: t.get("run").and_then(|v| v.as_f64()).map(|v| v as f32),
        })
    }
    fn to_value(&self) -> Value {
        Value::Table(Table::new())
    }
}

/// One hotbar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    pub title: String,
    pub columns: u32,
    pub button_width: f32,
    pub button_height: f32,
    pub icon_size: f32,
    pub buttons: Vec<Button>,
    /// The grid style.
    pub style: Style,
}

impl Bar {
    /// The default geometry, recovered from the `bars.toml` writer's literals.
    pub const DEFAULT_COLUMNS: u32 = 12;
    pub const DEFAULT_BUTTON: f32 = 32.0;
    pub const DEFAULT_ICON: f32 = 24.0;

    /// `@0x1018d5370` — the button at a grid position.
    pub fn button(&self, row: u32, column: u32) -> Option<&Button> {
        if column >= self.columns {
            return None;
        }
        self.buttons.get((row * self.columns + column) as usize)
    }

    /// `@0x1018d4fa4` — the grid: how many rows and the cell size.
    ///
    /// Returns `(rows, cell_width, cell_height)`.
    pub fn grid(&self) -> (u32, f32, f32) {
        let columns = self.columns.max(1);
        let rows = (self.buttons.len() as u32).div_ceil(columns);
        (rows, self.button_width, self.button_height)
    }

    /// `@0x1018d4e14` — move a button to another slot, keeping the grid compact.
    pub fn move_button(&mut self, from: usize, to: usize) -> bool {
        if from >= self.buttons.len() || to >= self.buttons.len() || from == to {
            return false;
        }
        let button = self.buttons.remove(from);
        self.buttons.insert(to, button);
        true
    }

    /// The index of the button with this id.
    pub fn find(&self, id: &str) -> Option<usize> {
        self.buttons.iter().position(|b| b.id == id)
    }

    /// Phase 4 — the rectangle of a grid cell.
    ///
    /// Straight from `grid()` @0x1018d4fa4: cell `(row, column)` sits at
    /// `origin + (column * button_width, row * button_height)` and is
    /// `button_width × button_height` big.
    pub fn cell_rect(&self, origin: Pos2, row: u32, column: u32) -> Rect {
        let (_, w, h) = self.grid();
        Rect::from_min_size(
            Pos2::new(
                origin.x + column as f32 * w,
                origin.y + row as f32 * h,
            ),
            Vec2::new(w, h),
        )
    }

    /// The size the whole grid takes: `columns * button_width` by
    /// `rows * button_height`.
    pub fn size(&self) -> Vec2 {
        let (rows, w, h) = self.grid();
        Vec2::new(w * self.columns.max(1) as f32, h * rows as f32)
    }

    /// Phase 4 — draw the bar into `ui` with the built-in palette.
    pub fn ui(&self, ui: &mut Ui) -> Grid {
        let palette = Palette::bare();
        self.ui_with(ui, &Look::new(&palette))
    }

    /// Phase 4 — draw the bar into `ui`.
    ///
    /// A real grid, from the recovered numbers: `columns` cells per row, cell
    /// `(row, column)` at `col * button_width, row * button_height`
    /// ([`Bar::cell_rect`]), every cell filled with the button's own style —
    /// `Button::look` @0x1018d5810 resolves `fg`/`bg` from the `[states]` table
    /// or the palette ([`Token::Text`] and [`Token::Raised`] when nothing is
    /// set) — and its `label` painted in `fg`.
    ///
    /// The icon is a placeholder box of `icon_size` points centred in the cell,
    /// filled with [`Token::Inset`]: there are no textures in this crate
    /// (`hotbar::icon::Sheet`/`source` name the PNG, `hydra_gui` loads it), so
    /// the label is painted inside that box instead — which is what the task's
    /// "rect_filled + label when no texture" asks for.
    ///
    /// A cell whose button has a countdown shows `countdown_text(seconds)` in
    /// [`Token::Warning`] at its bottom right.
    ///
    /// `// UNKNOWN:` `Bar::title` is *not* drawn here: the title bar belongs to
    /// the window frame (`frame::titled`), and the binary's hotbar draws its
    /// caption in `hydra_gui::bar`.
    pub fn ui_with(&self, ui: &mut Ui, look: &Look<'_>) -> Grid {
        let size = self.size();
        if size.x <= 0.0 || size.y <= 0.0 {
            // A bar with no cell size draws nothing at all.
            return Grid {
                rect: Rect::NOTHING,
                cells: Vec::new(),
            };
        }
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let (rows, w, h) = self.grid();
        let font = match &look.font {
            Some(font) => font.clone(),
            None => TextStyle::Button.resolve(ui.style()),
        };
        let painter = ui.painter();
        let mut grid = Grid {
            rect,
            cells: Vec::new(),
        };
        for row in 0..rows {
            for column in 0..self.columns.max(1) {
                let Some(button) = self.button(row, column) else {
                    continue;
                };
                let cell = self.cell_rect(rect.min, row, column);
                let style = button.look(look.state, look.palette);
                let mut bg = paint::color(style.bg.unwrap_or(look.palette.get(Token::Raised)));
                let fg = paint::color(style.fg.unwrap_or(look.palette.get(Token::Text)));
                if style.dim {
                    bg = bg.gamma_multiply(0.5);
                }
                painter.rect_filled(cell, look.corner, bg);
                if look.stroke > 0.0 {
                    painter.rect_stroke(
                        cell,
                        look.corner,
                        Stroke::new(look.stroke, paint::ink(look.palette, Token::Line)),
                        StrokeKind::Inside,
                    );
                }
                let icon_size = self.icon_size.min(w).min(h).max(0.0);
                let icon = (icon_size > 0.0).then(|| {
                    let box_ = Rect::from_center_size(cell.center(), Vec2::splat(icon_size));
                    painter.rect_filled(box_, 0.0, paint::ink(look.palette, Token::Inset));
                    box_
                });
                let label_at = icon.map(|i| i.center()).unwrap_or(cell.center());
                if !style.label.is_empty() {
                    painter.text(
                        label_at,
                        Align2::CENTER_CENTER,
                        &style.label,
                        font.clone(),
                        fg,
                    );
                }
                let count = if button.countdown.is_some() {
                    countdown_text(look.seconds)
                } else {
                    String::new()
                };
                let countdown = (!count.is_empty()).then(|| {
                    painter.text(
                        Pos2::new(cell.right() - 1.0, cell.bottom() - 1.0),
                        Align2::RIGHT_BOTTOM,
                        &count,
                        font.clone(),
                        paint::ink(look.palette, Token::Warning),
                    )
                });
                let id = ui.id().with(("hotbar", &self.title, row, column));
                let response = ui.interact(cell, id, Sense::click());
                grid.cells.push(Cell {
                    row,
                    column,
                    rect: cell,
                    icon,
                    label: style.label,
                    countdown,
                    id: button.id.clone(),
                    command: button.command().map(String::from),
                    clicked: response.clicked(),
                });
            }
        }
        grid
    }
}

/// What a bar is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct Look<'a> {
    /// The palette `Button::look` resolves its `fg`/`bg` from.
    pub palette: &'a Palette,
    /// The state whose per-state style the buttons show (`roundtime`,
    /// `casttime` or `effect`), if any.
    pub state: Option<ButtonState>,
    /// The seconds left, from [`Countdown::left`].
    pub seconds: u32,
    /// The corner radius (`[shape] corners` / a button's `corner`).
    pub corner: f32,
    /// The outline width; `0.0` draws no outline.
    pub stroke: f32,
    /// The caption face, defaulting to egui's `TextStyle::Button`.
    pub font: Option<FontId>,
}

impl<'a> Look<'a> {
    /// The look a palette gives on its own.
    pub fn new(palette: &'a Palette) -> Look<'a> {
        Look {
            palette,
            state: None,
            seconds: 0,
            corner: 0.0,
            stroke: 1.0,
            font: None,
        }
    }

    /// The same look showing `state`'s styles with `seconds` to go.
    pub fn showing(mut self, state: ButtonState, seconds: u32) -> Look<'a> {
        self.state = Some(state);
        self.seconds = seconds;
        self
    }
}

/// One painted hotbar cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Grid position.
    pub row: u32,
    /// Grid position.
    pub column: u32,
    /// The cell's rectangle, [`Bar::cell_rect`].
    pub rect: Rect,
    /// The icon placeholder box, when the bar sets an `icon_size`.
    pub icon: Option<Rect>,
    /// The caption that was painted.
    pub label: String,
    /// Where the countdown readout went, when there was one.
    pub countdown: Option<Rect>,
    /// The button's `id`.
    pub id: String,
    /// The command the button sends (`command` or `macro`).
    pub command: Option<String>,
    /// Did the user click this cell this frame?
    pub clicked: bool,
}

/// What [`Bar::ui_with`] drew.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    /// The whole grid's rectangle.
    pub rect: Rect,
    /// One cell per button, row-major.
    pub cells: Vec<Cell>,
}

impl Default for Grid {
    fn default() -> Grid {
        Grid {
            rect: Rect::NOTHING,
            cells: Vec::new(),
        }
    }
}

/// A whole `hotbars.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HotbarFile {
    /// The bars, by name.
    pub bars: BTreeMap<String, Bar>,
    /// Which theme recipe the bars were made for.
    pub recipe: Option<String>,
}

impl HotbarFile {
    /// `@0x1018d44d4` — parse a `hotbars.toml`.
    pub fn parse(src: &str) -> Result<HotbarFile, String> {
        let doc = toml::parse(src).map_err(|e| e.to_string())?;
        Ok(HotbarFile::from_doc(&doc))
    }

    fn from_doc(doc: &Table) -> HotbarFile {
        let mut file = HotbarFile::default();
        for (name, v) in doc {
            if name == "recipe" {
                file.recipe = v.as_str().map(String::from);
                continue;
            }
            if let Some(t) = v.as_table() {
                file.bars.insert(name.clone(), bar_from(t));
            }
        }
        file
    }

    /// `@0x1018d4150` — one bar by name.
    pub fn bar(&self, name: &str) -> Option<&Bar> {
        self.bars.get(name)
    }

    /// `@0x1018d41fc` — read `hotbars.toml` from a folder.
    pub fn load(dir: &std::path::Path) -> Result<HotbarFile, String> {
        let path = dir.join("hotbars.toml");
        match std::fs::read_to_string(&path) {
            Ok(text) => HotbarFile::parse(&text),
            Err(_) => Ok(HotbarFile::default()),
        }
    }

    /// `@0x1018d45fc` — overlay `self` on top of `base`.
    pub fn merged(&self, base: &HotbarFile) -> HotbarFile {
        let mut bars = base.bars.clone();
        for (name, bar) in &self.bars {
            bars.insert(name.clone(), bar.clone());
        }
        HotbarFile {
            bars,
            recipe: self.recipe.clone().or_else(|| base.recipe.clone()),
        }
    }
}

fn bar_from(t: &Table) -> Bar {
    let f = |k: &str| t.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
    let buttons = t
        .get("buttons")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(button_from).collect())
        .unwrap_or_default();
    Bar {
        title: t.get("title").and_then(|v| v.as_str()).unwrap_or("").into(),
        columns: t
            .get("columns")
            .and_then(|v| v.as_i64())
            .map(|v| v as u32)
            .unwrap_or(Bar::DEFAULT_COLUMNS),
        button_width: f("button_width").unwrap_or(Bar::DEFAULT_BUTTON),
        button_height: f("button_height").unwrap_or(Bar::DEFAULT_BUTTON),
        icon_size: f("icon_size").unwrap_or(Bar::DEFAULT_ICON),
        buttons,
        style: Style::default(),
    }
}

fn button_from(v: &Value) -> Option<Button> {
    if let Some(id) = v.as_str() {
        return Some(Button {
            id: id.to_string(),
            ..Button::default()
        });
    }
    let t = v.as_table()?;
    let s = |k: &str| t.get(k).and_then(|v| v.as_str()).map(String::from);
    Some(Button {
        id: s("id").unwrap_or_default(),
        command: s("command"),
        r#macro: s("macro"),
        hotkey: s("hotkey"),
        tooltip: s("tooltip"),
        countdown: s("countdown").and_then(|c| Countdown::named(&c)),
        states: BTreeMap::new(),
        default_style: Style::default(),
        icon_mode: s("icon_mode"),
    })
}

impl Countdown {
    /// The wire name of a countdown kind.
    pub fn named(name: &str) -> Option<Countdown> {
        Some(match name {
            "roundtime" => Countdown::Roundtime,
            "casttime" => Countdown::Casttime,
            "effect" => Countdown::Effect {
                name: String::new(),
                category: String::new(),
                seconds: 0,
            },
            _ => return None,
        })
    }
}

/// `@0x1018d5e10` — the text a countdown button shows.
pub fn countdown_text(seconds: u32) -> String {
    if seconds == 0 {
        String::new()
    } else {
        seconds.to_string()
    }
}

/// The runs a button's label is drawn from.
pub fn label_runs(label: &str, palette: &Palette, color: Rgb) -> Runs {
    let _ = palette;
    Runs::from_vec(vec![hydra_protocol::runs::Run {
        text: label.to_string(),
        style: color as u16,
        ..Default::default()
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r##"
[travel]
title = "Travel"
columns = 4
buttons = [
  { id = "n", command = "north", tooltip = "Go north" },
  { id = "s", command = "south" },
]
"##;

    #[test]
    fn parses_a_hotbar_file() {
        let f = HotbarFile::parse(FILE).unwrap();
        let bar = f.bar("travel").unwrap();
        assert_eq!(bar.title, "Travel");
        assert_eq!(bar.columns, 4);
        assert_eq!(bar.buttons.len(), 2);
        assert_eq!(bar.buttons[0].command(), Some("north"));
        assert_eq!(bar.button_width, Bar::DEFAULT_BUTTON);
        assert_eq!(bar.icon_size, Bar::DEFAULT_ICON);
        assert!(f.bar("nope").is_none());
    }

    #[test]
    fn grid_maths() {
        let mut bar = Bar {
            columns: 2,
            button_width: 40.0,
            button_height: 30.0,
            ..Bar::default()
        };
        for i in 0..5 {
            bar.buttons.push(Button {
                id: i.to_string(),
                ..Button::default()
            });
        }
        assert_eq!(bar.grid(), (3, 40.0, 30.0));
        assert_eq!(bar.button(0, 0).unwrap().id, "0");
        assert_eq!(bar.button(0, 1).unwrap().id, "1");
        assert_eq!(bar.button(1, 0).unwrap().id, "2");
        assert!(bar.button(0, 4).is_none());
    }

    #[test]
    fn moving_buttons() {
        let mut bar = Bar {
            columns: 2,
            ..Bar::default()
        };
        for i in 0..3 {
            bar.buttons.push(Button {
                id: i.to_string(),
                ..Button::default()
            });
        }
        assert!(!bar.move_button(0, 9));
        assert!(!bar.move_button(1, 1));
        assert!(bar.move_button(0, 2));
        assert_eq!(bar.buttons[2].id, "0");
        assert_eq!(bar.find("0"), Some(2));
        assert_eq!(bar.find("zzz"), None);
    }

    #[test]
    fn button_state_names() {
        for (name, state) in [
            ("roundtime", ButtonState::Roundtime),
            ("casttime", ButtonState::Casttime),
            ("effect", ButtonState::Effect),
        ] {
            assert_eq!(ButtonState::named(name), Some(state));
            assert_eq!(state.name(), name);
        }
        assert_eq!(ButtonState::named("nope"), None);
    }

    #[test]
    fn countdown_left_reads_the_clocks() {
        assert_eq!(Countdown::Roundtime.left(Some(3), Some(9), &BTreeMap::new()), 3);
        assert_eq!(Countdown::Casttime.left(Some(3), Some(9), &BTreeMap::new()), 9);
        assert_eq!(Countdown::Roundtime.left(None, None, &BTreeMap::new()), 0);
        let mut effects = BTreeMap::new();
        effects.insert("Haste".to_string(), 7);
        let c = Countdown::Effect {
            name: "Haste".into(),
            category: "Buffs".into(),
            seconds: 30,
        };
        assert_eq!(c.left(None, None, &effects), 7);
    }

    #[test]
    fn look_falls_back_to_the_palette() {
        let p = Palette::bare();
        let b = Button {
            id: "n".into(),
            command: Some("north".into()),
            ..Button::default()
        };
        let style = b.look(None, &p);
        assert_eq!(style.fg, Some(p.get(Token::Text)));
        assert_eq!(style.bg, Some(p.get(Token::Raised)));
        assert_eq!(style.label, "north");
        // A state style wins over the default.
        let mut b2 = b.clone();
        b2.states.insert(
            "roundtime".into(),
            Style {
                label: "wait".into(),
                fg: Some(1),
                ..Style::default()
            },
        );
        let style = b2.look(Some(ButtonState::Roundtime), &p);
        assert_eq!(style.label, "wait");
        assert_eq!(style.fg, Some(1));
    }

    #[test]
    fn merging_bars() {
        let base = HotbarFile::parse(FILE).unwrap();
        let mut top = HotbarFile::default();
        top.bars.insert("other".into(), Bar::default());
        let merged = top.merged(&base);
        assert_eq!(merged.bars.len(), 2);
        assert!(merged.bars.contains_key("travel"));
    }

    #[test]
    fn countdown_text_formats_seconds() {
        assert_eq!(countdown_text(0), "");
        assert_eq!(countdown_text(5), "5");
    }

    #[test]
    fn label_runs_carry_the_colour() {
        let p = Palette::bare();
        let runs = label_runs("Go", &p, 0x1234);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs.plain(), "Go");
    }
}

/// Phase 4 — the hotbar is a real grid.  One headless frame per test, with the
/// cell geometry checked against [`Bar::grid`] and the colours against the
/// palette.
#[cfg(test)]
mod drawing {
    use super::*;
    use crate::theme::{rgb, Palette};
    use egui::{Color32, Shape};

    fn frame(mut ui: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)));
        ctx.run_ui(input, ui)
    }

    fn rects(out: &egui::FullOutput) -> Vec<egui::epaint::RectShape> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Rect(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
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

    /// A 2×3 bar of five buttons, 40×30 cells, 24-point icons.
    fn bar() -> Bar {
        let mut bar = Bar {
            title: "Travel".into(),
            columns: 2,
            button_width: 40.0,
            button_height: 30.0,
            icon_size: 24.0,
            ..Bar::default()
        };
        for (i, (id, command)) in [
            ("n", "north"),
            ("s", "south"),
            ("e", "east"),
            ("w", "west"),
            ("u", "up"),
        ]
        .into_iter()
        .enumerate()
        {
            bar.buttons.push(Button {
                id: id.to_string(),
                command: Some(command.to_string()),
                countdown: (i == 0).then_some(Countdown::Roundtime),
                ..Button::default()
            });
        }
        bar
    }

    #[test]
    fn cells_are_placed_by_the_recovered_grid_maths() {
        let bar = bar();
        let palette = Palette::bare();
        let mut seen = Grid::default();
        let out = frame(|ui| {
            seen = bar.ui_with(ui, &Look::new(&palette));
        });

        let (rows, w, h) = bar.grid();
        assert_eq!((rows, w, h), (3, 40.0, 30.0));
        assert_eq!(seen.cells.len(), 5);
        // The grid is `columns * width` by `rows * height` — 80×90.
        assert_eq!(seen.rect.width(), 80.0);
        assert_eq!(seen.rect.height(), 90.0);
        assert_eq!(bar.size(), Vec2::new(80.0, 90.0));

        // Every cell is exactly `cell_rect(row, column)`.
        for cell in &seen.cells {
            assert_eq!(cell.rect, bar.cell_rect(seen.rect.min, cell.row, cell.column));
            assert_eq!(cell.rect.width(), 40.0);
            assert_eq!(cell.rect.height(), 30.0);
        }
        // The fifth button is the first cell of the third row.
        assert_eq!((seen.cells[4].row, seen.cells[4].column), (2, 0));
        assert_eq!(seen.cells[4].rect.min, seen.rect.min + Vec2::new(0.0, 60.0));
        // …and the second cell of the second row is 40 across, 30 down.
        assert_eq!(
            bar.cell_rect(seen.rect.min, 1, 1).min,
            seen.rect.min + Vec2::new(40.0, 30.0)
        );

        // The icon is a centred `icon_size` box inside the cell.
        let icon = seen.cells[1].icon.unwrap();
        assert_eq!(icon.width(), 24.0);
        assert_eq!(icon.center(), seen.cells[1].rect.center());

        // Each cell: a `raised` fill (#464646), an outline in `line`, and an
        // `inset` (#0a0a0a) icon box.
        let rects = rects(&out);
        assert_eq!(rects.len(), 15);
        for (i, cell) in seen.cells.iter().enumerate() {
            assert_eq!(rects[i * 3].rect, cell.rect);
            assert_eq!(rects[i * 3].fill, Color32::from_rgb(0x46, 0x46, 0x46));
            assert_eq!(rects[i * 3 + 1].rect, cell.rect);
            assert_eq!(rects[i * 3 + 1].fill, Color32::TRANSPARENT);
            assert_eq!(rects[i * 3 + 1].stroke.color, Color32::from_rgb(0x3c, 0x3c, 0x3c));
            assert_eq!(rects[i * 3 + 2].rect, cell.icon.unwrap());
            assert_eq!(rects[i * 3 + 2].fill, Color32::from_rgb(0x0a, 0x0a, 0x0a));
        }
    }

    #[test]
    fn every_button_paints_its_recovered_label() {
        let bar = bar();
        let palette = Palette::bare();
        let out = frame(|ui| {
            bar.ui_with(ui, &Look::new(&palette));
        });
        // `Button::look` falls back to the command, so the captions are the
        // commands.
        let text: Vec<String> = texts(&out).into_iter().map(|(t, _)| t).collect();
        assert_eq!(text, vec!["north", "south", "east", "west", "up"]);
        // …painted in `text` (#b4b4b4).
        assert!(texts(&out)
            .iter()
            .all(|(_, c)| *c == Color32::from_rgb(0xb4, 0xb4, 0xb4)));
    }

    #[test]
    fn a_state_style_and_a_countdown_recolour_the_cell() {
        let mut bar = bar();
        // The first button says "wait" and paints red while the roundtime runs.
        bar.buttons[0].states.insert(
            "roundtime".into(),
            Style {
                label: "wait".into(),
                fg: Some(rgb(0xff, 0x00, 0x00)),
                bg: Some(rgb(0x00, 0x00, 0x80)),
                dim: false,
                ..Style::default()
            },
        );
        let palette = Palette::bare();
        let mut seen = Grid::default();
        let out = frame(|ui| {
            seen = bar.ui_with(ui, &Look::new(&palette).showing(ButtonState::Roundtime, 7));
        });
        assert_eq!(seen.cells[0].label, "wait");
        let text = texts(&out);
        assert_eq!(text[0].0, "wait");
        assert_eq!(text[0].1, Color32::from_rgb(0xff, 0x00, 0x00));
        // The state's background wins.
        let rects = rects(&out);
        assert_eq!(rects[0].fill, Color32::from_rgb(0x00, 0x00, 0x80));
        // Only the button that has a countdown shows the readout: `7`, in
        // `warning` (#d7ad63).
        let counts: Vec<&(String, Color32)> = text.iter().filter(|(t, _)| t == "7").collect();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].1, Color32::from_rgb(0xd7, 0xad, 0x63));
        assert!(seen.cells[0].countdown.is_some());
        assert!(seen.cells[1].countdown.is_none());
        // The readout sits at the cell's bottom right.
        let cell = seen.cells[0].rect;
        assert_eq!(seen.cells[0].countdown.unwrap().right(), cell.right() - 1.0);
        assert_eq!(seen.cells[0].countdown.unwrap().bottom(), cell.bottom() - 1.0);
    }

    #[test]
    fn a_dimmed_button_is_the_same_colour_halved() {
        let mut bar = bar();
        bar.buttons[1].default_style.dim = true;
        let palette = Palette::bare();
        let out = frame(|ui| {
            bar.ui_with(ui, &Look::new(&palette));
        });
        let rects = rects(&out);
        // The undimmed cell is `raised` exactly.
        assert_eq!(rects[0].fill, Color32::from_rgb(0x46, 0x46, 0x46));
        // The dimmed one is `raised.gamma_multiply(0.5)`.
        assert_eq!(
            rects[3].fill,
            Color32::from_rgb(0x46, 0x46, 0x46).gamma_multiply(0.5)
        );
    }

    #[test]
    fn a_cell_that_a_button_does_not_fill_is_not_drawn() {
        // `columns = 2` but a button in column 1 of the first row; the second
        // row's empty second cell must not be painted.
        let bar = Bar {
            columns: 2,
            button_width: 10.0,
            button_height: 10.0,
            buttons: vec![Button {
                id: "solo".into(),
                ..Button::default()
            }],
            ..Bar::default()
        };
        let palette = Palette::bare();
        let mut seen = Grid::default();
        let out = frame(|ui| {
            seen = bar.ui_with(ui, &Look::new(&palette));
        });
        assert_eq!(seen.cells.len(), 1);
        // One cell: fill, outline, and no icon (icon_size is 0 here).
        assert_eq!(rects(&out).len(), 2);
        assert!(bar.button(0, 1).is_none());
    }

    #[test]
    fn a_bar_with_no_cell_size_draws_nothing() {
        let bar = Bar {
            columns: 3,
            button_width: 0.0,
            button_height: 10.0,
            buttons: vec![Button::default()],
            ..Bar::default()
        };
        let palette = Palette::bare();
        let mut seen = Some(Grid::default());
        let out = frame(|ui| {
            seen = Some(bar.ui_with(ui, &Look::new(&palette)));
        });
        let seen = seen.unwrap();
        assert!(seen.cells.is_empty());
        assert_eq!(seen.rect, Rect::NOTHING);
        assert!(out.shapes.is_empty());
    }

    #[test]
    fn ui_is_the_built_in_palette() {
        let bar = bar();
        let mut plain = Grid::default();
        frame(|ui| {
            plain = bar.ui(ui);
        });
        let mut themed = Grid::default();
        frame(|ui| {
            themed = bar.ui_with(ui, &Look::new(&Palette::bare()));
        });
        assert_eq!(plain, themed);
        assert_eq!(plain.cells.len(), 5);
        assert_eq!(plain.cells[0].command.as_deref(), Some("north"));
    }
}
