//! The chips: small read-outs of one game value, drawn as a bar or a number.
//!
//! **ESTABLISHED** shapes:
//!
//!  * `Chip` has eleven fields (`struct Chip with 11 elements`, `0x102418fd8`);
//!    the `__FieldVisitor` at `0x101478080` compares `name`, `list`, `color`,
//!    `empty`, `critical` and `order`, and the literal blob at `0x102417ef3`
//!    carries `color empty order bar height percent thresholds current shown`.
//!  * `Threshold` has two (`struct Threshold with 2 elements`).
//!  * The file is `chips.toml`, keyed by chip name under `chips`.

use std::collections::BTreeMap;

use crate::text::{self, Value};

/// The key the chips live under.
pub const FILE: &str = "chips";

/// One step of a bar's colouring.
///
/// **ESTABLISHED** (`0x102418fd8`): `struct Threshold with 2 elements`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Threshold {
    /// The percentage this colour starts at.
    pub at: f32,
    /// The colour from there on.
    pub color: String,
}

impl Threshold {
    pub fn read(value: &Value) -> Threshold {
        Threshold {
            at: value.f32_of("at").unwrap_or(0.0),
            color: value.str_of("color"),
        }
    }

    pub fn write(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("at".to_string(), Value::Float(self.at as f64));
        map.insert("color".to_string(), Value::Str(self.color.clone()));
        Value::Table(map)
    }
}

/// One chip.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chip {
    /// The value the chip watches, e.g. `health`.
    pub name: String,
    /// UNKNOWN: `list` is only ever read, never written; kept as text.
    pub list: String,
    pub color: String,
    /// What to show when the value is not known.
    pub empty: String,
    /// The value at which the chip is urgent.
    pub critical: f32,
    /// Where the chip sits on its bar.
    pub order: u32,
    /// Draw a bar rather than a number.
    pub bar: bool,
    /// The bar's height.
    pub height: f32,
    /// Show the value as a percentage.
    pub percent: bool,
    pub thresholds: Vec<Threshold>,
    /// Whether the chip is on screen at all.
    pub shown: bool,
}

impl Chip {
    /// The chips a data folder gets before it has a `chips.toml`.
    ///
    /// **PARTLY** (`0x101476b58`): the file's own `defaults` key wins, then
    /// [`Chip::built_in`] is the fallback.  UNKNOWN: the binary's built-in list
    /// was not recovered field by field, so this is the set of values the
    /// chrome is known to read — health, mana, stamina, spirit.
    pub fn defaults() -> Vec<Chip> {
        ["health", "mana", "stamina", "spirit"]
            .iter()
            .enumerate()
            .map(|(i, name)| Chip::bar(name, i as u32))
            .collect()
    }

    /// A plain bar chip for `name`.
    pub fn bar(name: &str, order: u32) -> Chip {
        Chip {
            name: name.to_string(),
            list: String::new(),
            color: String::new(),
            empty: String::new(),
            critical: 20.0,
            order,
            bar: true,
            height: 10.0,
            percent: true,
            thresholds: Vec::new(),
            shown: true,
        }
    }

    /// Read a chip out of a parsed document.
    pub fn read(name: &str, value: &Value) -> Chip {
        Chip {
            name: if value.str_of("name").is_empty() {
                name.to_string()
            } else {
                value.str_of("name")
            },
            list: value.str_of("list"),
            color: value.str_of("color"),
            empty: value.str_of("empty"),
            critical: value.f32_of("critical").unwrap_or(0.0),
            order: value.u64_of("order").unwrap_or(0) as u32,
            bar: value.bool_of("bar"),
            height: value.f32_of("height").unwrap_or(0.0),
            percent: value.bool_of("percent"),
            thresholds: value
                .get("thresholds")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(Threshold::read).collect())
                .unwrap_or_default(),
            shown: value.get("shown").map(|_| value.bool_of("shown")).unwrap_or(true),
        }
    }

    /// Write a chip back out.
    pub fn write(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), Value::Str(self.name.clone()));
        if !self.list.is_empty() {
            map.insert("list".to_string(), Value::Str(self.list.clone()));
        }
        if !self.color.is_empty() {
            map.insert("color".to_string(), Value::Str(self.color.clone()));
        }
        if !self.empty.is_empty() {
            map.insert("empty".to_string(), Value::Str(self.empty.clone()));
        }
        map.insert("critical".to_string(), Value::Float(self.critical as f64));
        map.insert("order".to_string(), Value::Int(self.order as i64));
        map.insert("bar".to_string(), Value::Bool(self.bar));
        map.insert("height".to_string(), Value::Float(self.height as f64));
        map.insert("percent".to_string(), Value::Bool(self.percent));
        if !self.thresholds.is_empty() {
            map.insert(
                "thresholds".to_string(),
                Value::Array(self.thresholds.iter().map(Threshold::write).collect()),
            );
        }
        map.insert("shown".to_string(), Value::Bool(self.shown));
        Value::Table(map)
    }

    /// The colour for a percentage: the highest threshold at or below it.
    ///
    /// **PARTLY** (`0x10147548c`): the thresholds are searched in file order and
    /// the chip's own `color` answers when none of them applies.  UNKNOWN:
    /// whether the binary takes the first or the highest match; the highest is
    /// the one that reads sensibly when a file lists them out of order.
    pub fn color_at(&self, percent: f32) -> &str {
        let mut found: Option<&Threshold> = None;
        for threshold in &self.thresholds {
            if percent >= threshold.at {
                match found {
                    Some(best) if best.at >= threshold.at => {}
                    _ => found = Some(threshold),
                }
            }
        }
        found.map(|t| t.color.as_str()).unwrap_or(self.color.as_str())
    }

    /// The text a chip shows for a value.
    pub fn words(&self, value: Option<f32>) -> String {
        match value {
            None => self.empty.clone(),
            Some(v) if self.percent => format!("{v:.0}%"),
            Some(v) => format!("{v:.0}"),
        }
    }
}

/// The whole `chips.toml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chips {
    pub chips: Vec<Chip>,
}

impl Chips {
    /// **ESTABLISHED** (`0x101474618`): the `chips` table, one chip per key, a
    /// bad one reported as `format!("the {} chip: {}", name, error)`.
    pub fn read(doc: &Value) -> Result<Chips, String> {
        let mut chips = Chips::default();
        if let Some(table) = doc.get(FILE).and_then(Value::as_table) {
            for (name, value) in table {
                chips.chips.push(Chip::read(name, value));
            }
        } else if let Some(list) = doc.get("defaults").and_then(Value::as_array) {
            chips.chips = list
                .iter()
                .map(|v| Chip::read(&v.str_of("name"), v))
                .collect();
        }
        chips.chips.sort_by_key(|chip| chip.order);
        Ok(chips)
    }

    /// The same, from the file's text.
    pub fn parse(text: &str) -> Result<Chips, String> {
        Chips::read(&text::parse_toml(text)?)
    }

    /// The chips that are on screen, in order.
    pub fn shown(&self) -> Vec<&Chip> {
        self.chips.iter().filter(|chip| chip.shown).collect()
    }

    /// The chip called `name`.
    pub fn of(&self, name: &str) -> Option<&Chip> {
        self.chips.iter().find(|chip| chip.name == name)
    }

    /// Write the chips back out.
    pub fn write(&self) -> Value {
        let mut table = BTreeMap::new();
        for chip in &self.chips {
            table.insert(chip.name.clone(), chip.write());
        }
        let mut doc = BTreeMap::new();
        doc.insert(FILE.to_string(), Value::Table(table));
        Value::Table(doc)
    }
}

/// A row of chips as drawn: what `gather` collects.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::chips::Row`,
/// `hydra_board::chips::Gathered`, `hydra_board::chips::Says`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    /// The chip being drawn.
    pub chip: Chip,
    /// The value, when it is known.
    pub value: Option<f32>,
    /// The text the chip shows.
    pub says: String,
    /// The colour it shows in.
    pub color: String,
}

/// The time the chips were last gathered, so they are not gathered twice.
///
/// **ESTABLISHED** (`0x101477190`): a plain `Instant` comparison.
pub fn clock(last: &mut Option<std::time::Instant>, now: std::time::Instant, every: std::time::Duration) -> bool {
    match last {
        Some(at) if now.saturating_duration_since(*at) < every => false,
        _ => {
            *last = Some(now);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chip_round_trips() {
        let text = "\
[chips.health]
color = \"#b03030\"
empty = \"--\"
critical = 25.0
order = 1
bar = true
height = 12.0
percent = true
shown = true
thresholds = [ { at = 75.0, color = \"#2e8b57\" }, { at = 25.0, color = \"#b03030\" } ]
";
        let chips = Chips::parse(text).unwrap();
        let chip = chips.of("health").unwrap();
        assert_eq!(chip.color, "#b03030");
        assert_eq!(chip.empty, "--");
        assert_eq!(chip.critical, 25.0);
        assert_eq!(chip.order, 1);
        assert!(chip.bar);
        assert_eq!(chip.height, 12.0);
        assert!(chip.percent);
        assert_eq!(chip.thresholds.len(), 2);
        assert_eq!(Chips::read(&chips.write()).unwrap(), chips);
    }

    #[test]
    fn thresholds_pick_the_colour() {
        let chip = Chip {
            color: "#000000".into(),
            thresholds: vec![
                Threshold { at: 75.0, color: "green".into() },
                Threshold { at: 25.0, color: "red".into() },
            ],
            ..Chip::default()
        };
        assert_eq!(chip.color_at(10.0), "#000000");
        assert_eq!(chip.color_at(30.0), "red");
        assert_eq!(chip.color_at(80.0), "green");
        assert_eq!(Chip::default().color_at(50.0), "");
    }

    #[test]
    fn words_show_the_empty_text_or_the_value() {
        let mut chip = Chip {
            empty: "--".into(),
            percent: true,
            ..Chip::default()
        };
        assert_eq!(chip.words(None), "--");
        assert_eq!(chip.words(Some(42.4)), "42%");
        chip.percent = false;
        assert_eq!(chip.words(Some(42.4)), "42");
    }

    #[test]
    fn chips_are_ordered_and_the_shown_ones_selected() {
        let text = "\
[chips.a]
order = 2
[chips.b]
order = 1
shown = false
[chips.c]
order = 3
shown = true
";
        let chips = Chips::parse(text).unwrap();
        let names: Vec<&str> = chips.chips.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        assert_eq!(chips.shown().len(), 2);
    }

    #[test]
    fn the_defaults_are_bars_in_order() {
        let chips = Chip::defaults();
        assert_eq!(chips.len(), 4);
        assert_eq!(chips[0].name, "health");
        assert_eq!(chips[3].order, 3);
        assert!(chips.iter().all(|c| c.bar && c.shown));
    }

    #[test]
    fn clock_gathers_at_most_once_per_period() {
        let start = std::time::Instant::now();
        let mut last = None;
        assert!(clock(&mut last, start, std::time::Duration::from_millis(100)));
        assert!(!clock(&mut last, start, std::time::Duration::from_millis(100)));
        let later = start + std::time::Duration::from_millis(150);
        assert!(clock(&mut last, later, std::time::Duration::from_millis(100)));
    }
}
