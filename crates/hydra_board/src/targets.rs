//! How the target window words a creature's name, and how it colours its state.
//!
//! **ESTABLISHED**:
//!
//!  * [`Part`] is one of `name`, `statuses`, `health` (`0x101481b88`, error
//!    `"{} is not a part of the line."`).
//!  * [`NameShown`] is one of `full`, `bare`, `noun`, `fit` (`0x101482c1c`,
//!    error `"{} is not a way to say a name."`).
//!  * [`TargetsLook`] is the `targets.toml` table (`struct TargetsLook`, the
//!    `__FieldVisitor` at `0x101433b80`), and `Threshold` is a
//!    `(at, color)` pair.

use std::collections::BTreeMap;

use crate::text::{self, Value};

/// The key the look lives under.
pub const FILE: &str = "targets";

/// Which part of a target line is being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Part {
    #[default]
    Name,
    Statuses,
    Health,
}

impl Part {
    /// **ESTABLISHED** (`0x101481b88`): `name`, `statuses` or `health`, trimmed.
    pub fn parse(text: &str) -> Result<Part, String> {
        match text.trim() {
            "name" => Ok(Part::Name),
            "statuses" => Ok(Part::Statuses),
            "health" => Ok(Part::Health),
            other => Err(format!("{other} is not a part of the line.")),
        }
    }

    /// **ESTABLISHED** (`0x101481c9c`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Part::Name => "name",
            Part::Statuses => "statuses",
            Part::Health => "health",
        }
    }
}

/// How much of a creature's name to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameShown {
    /// Everything, articles and all.
    #[default]
    Full,
    /// Without articles.
    Bare,
    /// Just the noun.
    Noun,
    /// Whatever fits the window.
    Fit,
}

impl NameShown {
    /// **ESTABLISHED** (`0x101482c1c`): `full`, `bare`, `noun` or `fit`.
    pub fn parse(text: &str) -> Result<NameShown, String> {
        match text.trim() {
            "full" => Ok(NameShown::Full),
            "bare" => Ok(NameShown::Bare),
            "noun" => Ok(NameShown::Noun),
            "fit" => Ok(NameShown::Fit),
            other => Err(format!("{other} is not a way to say a name.")),
        }
    }

    /// **ESTABLISHED** (`0x101482d58`): the four spellings, in this order.
    pub fn as_str(&self) -> &'static str {
        match self {
            NameShown::Full => "full",
            NameShown::Bare => "bare",
            NameShown::Noun => "noun",
            NameShown::Fit => "fit",
        }
    }
}

/// The status words that make a target look urgent.
///
/// **ESTABLISHED** (`0x101480f2c`, the drop glue for
/// `hydra_board::targets::StatusesWritten`): a map of status name to colour.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusesWritten {
    pub statuses: BTreeMap<String, String>,
}

impl StatusesWritten {
    /// The colour for a status word, if the file names one.
    pub fn color_of(&self, status: &str) -> Option<&str> {
        self.statuses.get(status).map(String::as_str)
    }

    pub fn read(value: &Value) -> StatusesWritten {
        let mut statuses = BTreeMap::new();
        if let Some(table) = value.as_table() {
            for (name, colour) in table {
                if let Some(colour) = colour.as_str() {
                    statuses.insert(name.clone(), colour.to_string());
                }
            }
        }
        StatusesWritten { statuses }
    }
}

/// A step of the health bar's colouring.
///
/// **ESTABLISHED** (`0x102418fd8`): `struct Threshold with 2 elements`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Threshold {
    /// The percentage this colour starts at.
    pub at: f32,
    pub color: String,
}

/// One target as the window shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Target {
    /// What the game calls it, e.g. `a grey wolf`.
    pub name: String,
    /// The noun on its own, e.g. `wolf`.
    pub noun: String,
    /// The statuses it is under.
    pub statuses: Vec<String>,
    /// Its health, when the game says.
    pub health: Option<f32>,
}

impl Target {
    /// The name with the articles taken off.
    ///
    /// **ESTABLISHED** (`0x101481db0`): the leading `a `, `an ` or `the ` is
    /// dropped, which is what `bare` shows.
    pub fn bare(&self) -> &str {
        let name = self.name.trim();
        for article in ["the ", "an ", "a "] {
            if let Some(rest) = name.strip_prefix(article) {
                return rest;
            }
        }
        name
    }

    /// The name as [`NameShown`] asks for it.
    ///
    /// **PARTLY** (`0x101482148`): `Fit` narrows the name to the noun when the
    /// bare name is too long; the width the binary used is UNKNOWN, so this
    /// takes a width argument.
    pub fn shown(&self, how: NameShown, width: usize) -> &str {
        match how {
            NameShown::Full => &self.name,
            NameShown::Bare => self.bare(),
            NameShown::Noun => self.noun(),
            NameShown::Fit if self.bare().chars().count() <= width => self.bare(),
            NameShown::Fit => self.noun(),
        }
    }

    /// The noun, or the whole name when there is none.
    ///
    /// **ESTABLISHED** (`0x10148224c`).
    pub fn noun(&self) -> &str {
        if self.noun.is_empty() {
            self.bare()
        } else {
            &self.noun
        }
    }

    /// The hint the window puts beside the name: the statuses, then the health.
    ///
    /// **PARTLY** (`0x101482148`): the binary joins the statuses with `", "` and
    /// appends the health when it knows it; the exact separators are UNKNOWN.
    pub fn hint(&self) -> String {
        let mut parts = self.statuses.clone();
        if let Some(health) = self.health {
            parts.push(format!("{health:.0}%"));
        }
        parts.join(", ")
    }
}

/// The whole look of the targets window.
///
/// **ESTABLISHED** shape (the drop glue for `hydra_board::targets::TargetsLook`):
/// the `NameShown` and the `Vec<Threshold>` the window draws with.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TargetsLook {
    /// How names are said.
    pub name: NameShown,
    /// Which part of the line the name is read from.
    pub from: Part,
    /// The colours the bar steps through.
    pub thresholds: Vec<Threshold>,
    /// The colour each status word takes.
    pub statuses: StatusesWritten,
    /// Only show creatures that are fighting.
    pub fighting_only: bool,
    /// Show the health as a number.
    pub show_health: bool,
}

impl TargetsLook {
    /// **ESTABLISHED** (`0x101480550`): the `targets` table; a bad entry is
    /// reported as `format!("the {} of targets: {}", key, error)`.
    pub fn read(doc: &Value) -> Result<TargetsLook, String> {
        let table = doc.get(FILE).unwrap_or(&Value::Null);
        let name = match table.get("name").and_then(Value::as_str) {
            Some(text) => NameShown::parse(text)?,
            None => NameShown::Full,
        };
        let from = match table.get("from").and_then(Value::as_str) {
            Some(text) => Part::parse(text)?,
            None => Part::Name,
        };
        Ok(TargetsLook {
            name,
            from,
            thresholds: table
                .get("thresholds")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|v| Threshold {
                            at: v.f32_of("at").unwrap_or(0.0),
                            color: v.str_of("color"),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            statuses: table
                .get("statuses")
                .map(StatusesWritten::read)
                .unwrap_or_default(),
            fighting_only: table.bool_of("fighting_only"),
            show_health: table.bool_of("show_health"),
        })
    }

    /// The same, from the file's text.
    pub fn parse(text: &str) -> Result<TargetsLook, String> {
        TargetsLook::read(&text::parse_toml(text)?)
    }

    /// The colour for a health percentage.
    ///
    /// **PARTLY** (`0x101480504`): UNKNOWN which end the thresholds are read
    /// from; the highest one at or below `percent` wins here, as with chips.
    pub fn threshold_color(&self, percent: f32) -> Option<&str> {
        let mut best: Option<&Threshold> = None;
        for threshold in &self.thresholds {
            if percent >= threshold.at {
                match best {
                    Some(b) if b.at >= threshold.at => {}
                    _ => best = Some(threshold),
                }
            }
        }
        best.map(|t| t.color.as_str())
    }

    /// The colour a status word takes, if the file names one.
    ///
    /// **ESTABLISHED** (`0x101480f2c`).
    pub fn status_shown(&self, status: &str) -> Option<&str> {
        self.statuses.color_of(status)
    }

    /// The colour the window gives a target: its worst status, else its health.
    pub fn listed(&self, target: &Target) -> Option<&str> {
        for status in &target.statuses {
            if let Some(colour) = self.status_shown(status) {
                return Some(colour);
            }
        }
        self.threshold_color(target.health.unwrap_or(100.0))
    }

    /// The name `target` is shown under.
    pub fn word<'a>(&self, target: &'a Target, width: usize) -> &'a str {
        target.shown(self.name, width)
    }
}

/// Whether a line is a targeting order rather than a target.
///
/// **ESTABLISHED** (`0x101482bf0`): the words `target`, `next`, `previous`,
/// `clear` and `focus` in front of a name are orders.
pub fn is_order(word: &str) -> bool {
    matches!(
        word.trim(),
        "target" | "target_next" | "target_previous" | "target_clear" | "next" | "previous" | "clear"
    )
}

/// The word a target is aimed with.
///
/// **ESTABLISHED** (`0x101481cdc`): the noun, trimmed, with `#` in front of a
/// number so the game reads it as a count.
pub fn word(target: &Target) -> String {
    let noun = target.noun().trim();
    if noun.is_empty() {
        String::new()
    } else {
        format!("#{noun}")
    }
}

/// The colour name for a status word, so the story can be told what to draw.
///
/// **ESTABLISHED** (`0x101480f2c`): the colour the file names for `status`.
pub fn status_named(look: &TargetsLook, status: &str) -> Option<String> {
    look.status_shown(status).map(str::to_string)
}

/// The targets to list, in the order the window shows them.
///
/// **ESTABLISHED** (`0x1014822c8`): the fighting ones first when
/// `fighting_only` is set, and each target's colour worked out.
pub fn targets<'a>(look: &TargetsLook, all: &'a [Target]) -> Vec<(&'a Target, Option<String>)> {
    all.iter()
        .filter(|target| !look.fighting_only || !target.statuses.is_empty())
        .map(|target| (target, look.listed(target).map(str::to_string)))
        .collect()
}

/// The targets placed on the map from a target line, so a click can aim.
///
/// **PARTLY** (`0x101482c00`): the binary reads a target line into `Target`s;
/// the exact grammar is UNKNOWN, so this reads `name`, `noun` and any status
/// words the look knows, one target per line, skipping empty lines.
pub fn placed_targets(look: &TargetsLook, lines: &[String]) -> Vec<Target> {
    let mut out = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let statuses = line
            .split_whitespace()
            .filter(|word| look.status_shown(word).is_some())
            .map(str::to_string)
            .collect();
        let mut words: Vec<&str> = line.split_whitespace().collect();
        let noun = words.pop().unwrap_or("").to_string();
        out.push(Target {
            name: line.to_string(),
            noun,
            statuses,
            health: None,
        });
    }
    out
}

/// The status words a look knows, so they can be read out of a line.
pub fn read_statuses(look: &TargetsLook, line: &str) -> Vec<String> {
    line.split_whitespace()
        .filter(|word| look.status_shown(word).is_some())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_and_names_parse_exactly() {
        assert_eq!(Part::parse(" name ").unwrap(), Part::Name);
        assert_eq!(Part::parse("statuses").unwrap(), Part::Statuses);
        assert_eq!(Part::parse("health").unwrap(), Part::Health);
        assert_eq!(
            Part::parse("mane").unwrap_err(),
            "mane is not a part of the line."
        );
        assert_eq!(Part::Statuses.as_str(), "statuses");

        assert_eq!(NameShown::parse("full").unwrap(), NameShown::Full);
        assert_eq!(NameShown::parse("bare").unwrap(), NameShown::Bare);
        assert_eq!(NameShown::parse("noun").unwrap(), NameShown::Noun);
        assert_eq!(NameShown::parse("fit").unwrap(), NameShown::Fit);
        assert_eq!(
            NameShown::parse("loud").unwrap_err(),
            "loud is not a way to say a name."
        );
        assert_eq!(NameShown::Fit.as_str(), "fit");
    }

    #[test]
    fn bare_drops_the_article_and_noun_falls_back() {
        let target = Target {
            name: "a grey wolf".into(),
            noun: "wolf".into(),
            ..Target::default()
        };
        assert_eq!(target.bare(), "grey wolf");
        assert_eq!(target.noun(), "wolf");
        let bare = Target {
            name: "the town guardsman".into(),
            noun: String::new(),
            ..Target::default()
        };
        assert_eq!(bare.bare(), "town guardsman");
        assert_eq!(bare.noun(), "town guardsman");
        // fit narrows to the noun when the bare name is too long
        assert_eq!(target.shown(NameShown::Full, 4), "a grey wolf");
        assert_eq!(target.shown(NameShown::Fit, 20), "grey wolf");
        assert_eq!(target.shown(NameShown::Fit, 4), "wolf");
    }

    #[test]
    fn the_look_reads_its_table() {
        let text = "\
[targets]
name = \"bare\"
from = \"name\"
fighting_only = true
show_health = true
thresholds = [ { at = 50.0, color = \"#b03030\" }, { at = 90.0, color = \"#2e8b57\" } ]
[targets.statuses]
stunned = \"#cc8844\"
dead = \"#000000\"
";
        let look = TargetsLook::parse(text).unwrap();
        assert_eq!(look.name, NameShown::Bare);
        assert_eq!(look.from, Part::Name);
        assert!(look.fighting_only);
        assert!(look.show_health);
        assert_eq!(look.threshold_color(20.0), None);
        assert_eq!(look.threshold_color(60.0), Some("#b03030"));
        assert_eq!(look.threshold_color(95.0), Some("#2e8b57"));
        assert_eq!(look.status_shown("stunned"), Some("#cc8844"));
        assert_eq!(look.status_shown("webbed"), None);
    }

    #[test]
    fn a_status_colour_wins_over_the_health_bar() {
        let look = TargetsLook {
            statuses: StatusesWritten {
                statuses: BTreeMap::from([("stunned".to_string(), "#cc8844".to_string())]),
            },
            thresholds: vec![Threshold {
                at: 90.0,
                color: "green".into(),
            }],
            ..TargetsLook::default()
        };
        let target = Target {
            name: "a grey wolf".into(),
            noun: "wolf".into(),
            statuses: vec!["stunned".into()],
            health: Some(95.0),
        };
        assert_eq!(look.listed(&target), Some("#cc8844"));
        let plain = Target {
            statuses: Vec::new(),
            ..target
        };
        assert_eq!(look.listed(&plain), Some("green"));
        // The default look shows the whole name.
        assert_eq!(look.word(&plain, 20), "a grey wolf");
        assert_eq!(look.word(&plain, 1), "a grey wolf");
    }

    #[test]
    fn orders_and_words() {
        assert!(is_order("target"));
        assert!(is_order("next"));
        assert!(!is_order("wolf"));
        let target = Target {
            name: "a grey wolf".into(),
            noun: "wolf".into(),
            ..Target::default()
        };
        assert_eq!(word(&target), "#wolf");
        assert_eq!(word(&Target::default()), "");
    }

    #[test]
    fn targets_are_filtered_and_coloured() {
        let look = TargetsLook {
            fighting_only: true,
            ..TargetsLook::default()
        };
        let all = vec![
            Target {
                name: "a grey wolf".into(),
                noun: "wolf".into(),
                statuses: vec!["stunned".into()],
                ..Target::default()
            },
            Target {
                name: "a rock".into(),
                noun: "rock".into(),
                ..Target::default()
            },
        ];
        let listed = targets(&look, &all);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0.noun, "wolf");
        let placed = placed_targets(&look, &["a grey wolf".to_string(), String::new()]);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].noun, "wolf");
    }
}
