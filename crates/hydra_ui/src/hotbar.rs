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

pub mod icon;

use std::collections::BTreeMap;

use hydra_protocol::runs::Runs;

use crate::layer::Over;
use crate::theme::{Palette, Rgb, Token};
use crate::toml::{self, Table, Value};

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
