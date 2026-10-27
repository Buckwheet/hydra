//! The settings page model.
//!
//! The recovered symbol set is `settings::{size, Column::*, Row::*}` plus the
//! types `Page`, `Value`, `RowKind` and `Change` (drop glue spells them out).
//! `Column` builds one cell of the settings table; `Row` builds a line.
//!
//! Sizes from the binary: `Column::blank` 132 B, `Column::words` 192 B,
//! `Column::choice` 180 B, `Column::colour` 196 B, `Column::all_words` 360 B,
//! `Row::fold` 232 B, `Row::heading` 232 B, `settings::size` 456 B — i.e. small
//! constructors over a shared `Value` enum, which is what is modelled here.

use crate::theme::{hex, Rgb};

/// One value the settings page can show or edit.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Nothing at all (a spacer).
    None,
    /// Free text.
    Text(String),
    /// A number.
    Number(f64),
    /// A switch.
    Bool(bool),
    /// One of a fixed set of words.
    Word(String),
    /// A colour.
    Colour(Rgb),
}

impl Value {
    /// The text a cell shows.
    pub fn shown(&self) -> String {
        match self {
            Value::None => String::new(),
            Value::Text(t) => t.clone(),
            Value::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{n:.0}")
                } else {
                    format!("{n}")
                }
            }
            Value::Bool(b) => (if *b { "on" } else { "off" }).to_string(),
            Value::Word(w) => w.clone(),
            Value::Colour(c) => hex(*c),
        }
    }
}

/// One cell of a settings row.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Column {
    /// An empty cell.
    #[default]
    Blank,
    /// Prose that wraps.
    Words(String),
    /// Every word of a space-separated list, one per line.
    AllWords(Vec<String>),
    /// A drop-down over these words.
    Choice(Vec<String>),
    /// A colour swatch.
    Colour(Rgb),
    /// A colour swatch that may be unset.
    ColourOrNone(Option<Rgb>),
}

impl Column {
    /// `@0x1018733ac` — an empty cell.
    pub fn blank() -> Column {
        Column::Blank
    }

    /// `@0x101873430` — a prose cell.
    pub fn words(text: impl Into<String>) -> Column {
        Column::Words(text.into())
    }

    /// `@0x101873668` — split `text` on whitespace and list every word.
    pub fn all_words(text: &str) -> Column {
        Column::AllWords(text.split_whitespace().map(|w| w.to_string()).collect())
    }

    /// `@0x1018734f0` — a drop-down cell.
    pub fn choice(words: &[&str]) -> Column {
        Column::Choice(words.iter().map(|w| w.to_string()).collect())
    }

    /// `@0x1018735a4` — a colour swatch.
    pub fn colour(c: Rgb) -> Column {
        Column::Colour(c)
    }

    /// `@0x1018732e4` — a colour swatch that may be unset.
    pub fn colour_or_none(c: Option<Rgb>) -> Column {
        Column::ColourOrNone(c)
    }

    /// Is the cell empty?
    pub fn is_blank(&self) -> bool {
        matches!(self, Column::Blank)
    }
}

/// The shape of a row.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum RowKind {
    /// A plain value row.
    #[default]
    Value,
    /// A section heading.
    Heading,
    /// A collapsible group; `open` is the initial state.
    Fold { open: bool },
    /// A row whose value is picked from a list.
    Choice,
}

/// One settings row: a label and its columns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub label: String,
    pub kind: RowKind,
    pub columns: Vec<Column>,
}

impl Row {
    /// A plain row.
    pub fn new(label: impl Into<String>, columns: Vec<Column>) -> Row {
        Row {
            label: label.into(),
            kind: RowKind::Value,
            columns,
        }
    }

    /// `@0x101872f4c` — a collapsible group, closed by default.
    pub fn fold(label: impl Into<String>) -> Row {
        Row {
            label: label.into(),
            kind: RowKind::Fold { open: false },
            columns: Vec::new(),
        }
    }

    /// `@0x101873034` — a section heading.
    pub fn heading(label: impl Into<String>) -> Row {
        Row {
            label: label.into(),
            kind: RowKind::Heading,
            columns: Vec::new(),
        }
    }

    /// Is this row a heading?
    pub fn is_heading(&self) -> bool {
        matches!(self.kind, RowKind::Heading)
    }
}

/// One settings page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub title: String,
    pub rows: Vec<Row>,
}

/// A pending edit.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// Dotted path into the settings tree, e.g. `shape.stroke`.
    pub path: String,
    pub value: Value,
}

/// `@0x10187311c` — how wide a value's cell should be.
///
/// The recovered function measures the shown text and clamps into the row
/// width; with no font metrics available here it falls back to a character
/// count scaled by `per_char`.
pub fn size(value: &Value, per_char: f32) -> f32 {
    let shown = value.shown();
    let n = shown.chars().count() as f32;
    (n * per_char).clamp(0.0, 4096.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::rgb;

    #[test]
    fn value_rendering() {
        assert_eq!(Value::None.shown(), "");
        assert_eq!(Value::Text("abc".into()).shown(), "abc");
        assert_eq!(Value::Number(3.0).shown(), "3");
        assert_eq!(Value::Number(3.5).shown(), "3.5");
        assert_eq!(Value::Bool(true).shown(), "on");
        assert_eq!(Value::Bool(false).shown(), "off");
        assert_eq!(Value::Word("boxed".into()).shown(), "boxed");
        assert_eq!(Value::Colour(rgb(0x63, 0xad, 0xd7)).shown(), "#63add7");
    }

    #[test]
    fn column_constructors() {
        assert_eq!(Column::blank(), Column::Blank);
        assert!(Column::blank().is_blank());
        assert_eq!(Column::words("hi"), Column::Words("hi".into()));
        assert!(!Column::words("hi").is_blank());
        assert_eq!(
            Column::all_words("a b  c"),
            Column::AllWords(vec!["a".into(), "b".into(), "c".into()])
        );
        assert_eq!(
            Column::choice(&["flat", "filled"]),
            Column::Choice(vec!["flat".into(), "filled".into()])
        );
        assert_eq!(Column::colour(1), Column::Colour(1));
        assert_eq!(Column::colour_or_none(None), Column::ColourOrNone(None));
    }

    #[test]
    fn row_constructors() {
        let h = Row::heading("Shape");
        assert!(h.is_heading());
        assert!(h.columns.is_empty());
        let f = Row::fold("Corners");
        assert_eq!(f.kind, RowKind::Fold { open: false });
        assert!(!f.is_heading());
        let r = Row::new("Stroke", vec![Column::words("1.0")]);
        assert_eq!(r.kind, RowKind::Value);
        assert_eq!(r.columns.len(), 1);
    }

    #[test]
    fn size_scales_with_text() {
        assert_eq!(size(&Value::None, 8.0), 0.0);
        assert_eq!(size(&Value::Text("abcd".into()), 8.0), 32.0);
        assert_eq!(size(&Value::Bool(true), 8.0), 16.0);
        // A very long value is clamped.
        let long = Value::Text("x".repeat(2000));
        assert_eq!(size(&long, 8.0), 4096.0);
    }
}
