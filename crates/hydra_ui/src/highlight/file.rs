//! The highlight rule file (`highlights.toml`).
//!
//! Recovered field names for a rule (serde pool at 0x103991abf / 0x103993400):
//!
//! ```text
//! regex case_sensitive whole_word color squelch substitute status sound
//! volume alert notify speak rumble picture timer seconds caught times rearm
//! character stream origin off window spans
//! ```
//!
//! `HighlightFile` has `categories`, `highlight` and `widgets`;
//! `struct variant Mark::Full` and the `SayMark`/`Mark` untagged enums are the
//! `"a pattern: not Vellum's highlights"` style errors.  `highlight::file::general`
//! @0x10184ad3c is 6.1 KB: it returns the built-in rule set.

use std::collections::BTreeMap;

use crate::layer::Over;
use crate::theme::{Rgb, Token};
use crate::toml::{self, Table, Value};

/// The categories the editor shows, in the binary's order.
pub const CATEGORIES: [&str; 8] = [
    "creature",
    "speech",
    "room",
    "combat",
    "channel",
    "status",
    "map",
    "general",
];

/// The category name a rule belongs to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Mark {
    /// One of [`CATEGORIES`].
    Named(String),
    /// A rule with no category.
    #[default]
    None,
}

impl Mark {
    /// `@0x10184ad1c` — the category name.
    pub fn name(&self) -> &str {
        match self {
            Mark::Named(name) => name,
            Mark::None => "",
        }
    }

    pub fn named(name: &str) -> Mark {
        if name.is_empty() {
            Mark::None
        } else {
            Mark::Named(name.to_string())
        }
    }
}

/// One highlight rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rule {
    /// The pattern text (see [`super::matcher::Pattern`] for the subset).
    pub pattern: String,
    /// Match case exactly?
    pub case_sensitive: bool,
    /// Require word boundaries?
    pub whole_word: bool,
    /// The colour to paint the match with, if the rule overrides the token.
    pub color: Option<Rgb>,
    /// Drop the line entirely.
    pub squelch: bool,
    /// Replace the matched text.
    pub substitute: Option<String>,
    /// Set a status flag while the match lasts.
    pub status: Option<String>,
    /// The sound file to play.
    pub sound: Option<String>,
    /// The sound's volume, 0..=100.
    pub volume: Option<u32>,
    /// Show a desktop alert.
    pub alert: bool,
    /// Send a notification.
    pub notify: bool,
    /// Speak the line.
    pub speak: bool,
    /// Rumble.
    pub rumble: bool,
    /// Show a picture.
    pub picture: Option<String>,
    /// Start a timer.
    pub timer: Option<String>,
    /// The timer's length.
    pub seconds: Option<u32>,
    /// How many times the rule has fired.
    pub caught: u32,
    /// How many times it may fire.
    pub times: Option<u32>,
    /// Re-arm after this many seconds.
    pub rearm: Option<u32>,
    /// Only for this character.
    pub character: Option<String>,
    /// Only on this stream.
    pub stream: Option<String>,
    /// Where the line goes when it matches.
    pub origin: Option<String>,
    /// Turn the rule off.
    pub off: bool,
    /// Only inside this window.
    pub window: Option<String>,
    /// The category.
    pub category: Mark,
}

/// `@0x10184ad3c` — the built-in rules.
///
/// `// UNKNOWN:` the 6.1 KB body is a table of `(pattern, category, token)`
/// triples; only the structure and the categories are reproduced here, plus the
/// handful of patterns whose literals are visible in the pool.
pub fn general() -> Vec<Rule> {
    let table: [(&str, &str); 5] = [
        ("You also see", "room"),
        ("Obvious exits:", "room"),
        ("You are stunned", "status"),
        ("Death cry:", "combat"),
        ("You hear the voice of", "speech"),
    ];
    table
        .iter()
        .map(|(pattern, category)| Rule {
            pattern: pattern.to_string(),
            category: Mark::named(category),
            ..Rule::default()
        })
        .collect()
}

/// The widget rule: a highlight attached to a UI element.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widget {
    pub name: String,
    pub rule: Rule,
}

/// A whole `highlights.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HighlightFile {
    /// The rules, by category.
    pub categories: BTreeMap<String, Vec<Rule>>,
    /// The rules themselves, by name.
    pub highlight: BTreeMap<String, Rule>,
    /// Rules attached to widgets.
    pub widgets: Vec<Widget>,
    /// Redirect targets, kept aside so [`HighlightFile::take_redirects`] can
    /// hand them to the story window once.
    pub redirects: BTreeMap<String, String>,
}

impl HighlightFile {
    pub fn new() -> HighlightFile {
        HighlightFile::default()
    }

    /// `@0x1018494d0` — parse a `highlights.toml` body.
    pub fn parse(src: &str) -> Result<HighlightFile, String> {
        let doc = toml::parse(src).map_err(|e| e.to_string())?;
        Ok(HighlightFile::from_doc(&doc))
    }

    fn from_doc(doc: &Table) -> HighlightFile {
        let mut file = HighlightFile::default();
        for (name, v) in doc {
            match name.as_str() {
                "highlight" => {
                    if let Some(t) = v.as_table() {
                        for (rule_name, rv) in t {
                            if let Some(rule) = rule_from(rv) {
                                file.highlight.insert(rule_name.clone(), rule);
                            }
                        }
                    }
                }
                "categories" => {
                    if let Some(t) = v.as_table() {
                        for (cat, list) in t {
                            let rules: Vec<Rule> = list
                                .as_array()
                                .map(|a| a.iter().filter_map(rule_from).collect())
                                .unwrap_or_default();
                            file.categories.insert(cat.clone(), rules);
                        }
                    }
                }
                "widgets" => {
                    if let Some(t) = v.as_table() {
                        for (w, rv) in t {
                            if let Some(rule) = rule_from(rv) {
                                file.widgets.push(Widget {
                                    name: w.clone(),
                                    rule,
                                });
                            }
                        }
                    }
                }
                "redirect" => {
                    if let Some(t) = v.as_table() {
                        for (k, v) in t {
                            if let Some(s) = v.as_str() {
                                file.redirects.insert(k.clone(), s.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        file
    }

    /// `@0x1018490b0` — read `highlights.toml` from a folder.
    pub fn load(dir: &std::path::Path) -> Result<HighlightFile, String> {
        let path = dir.join("highlights.toml");
        match std::fs::read_to_string(&path) {
            Ok(text) => HighlightFile::parse(&text),
            // No file is not an error: the caller starts from `general()`.
            Err(_) => Ok(HighlightFile::from_rules(general())),
        }
    }

    /// A file holding one category's worth of rules.
    pub fn from_rules(rules: Vec<Rule>) -> HighlightFile {
        let mut file = HighlightFile::default();
        for rule in rules {
            let name = rule.pattern.clone();
            let cat = rule.category.name().to_string();
            file.highlight.insert(name, rule.clone());
            file.categories.entry(cat).or_default().push(rule);
        }
        file
    }

    /// `@0x10184930c` — write `highlights.toml`.
    pub fn save(&self, dir: &std::path::Path) -> Result<(), String> {
        let path = dir.join("highlights.toml");
        std::fs::write(&path, self.to_toml()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// `@0x1018495fc` — render the file.
    pub fn to_toml(&self) -> String {
        let mut doc = Table::new();
        let mut highlight = Table::new();
        for (name, rule) in &self.highlight {
            highlight.insert(name.clone(), rule_to(rule));
        }
        doc.insert("highlight".into(), Value::Table(highlight));
        let mut categories = Table::new();
        for (cat, rules) in &self.categories {
            categories.insert(
                cat.clone(),
                Value::Array(rules.iter().map(rule_to).collect()),
            );
        }
        doc.insert("categories".into(), Value::Table(categories));
        toml::to_string(&doc)
    }

    /// `@0x101848ed4` — is a category switched on?
    ///
    /// A category is on when it has any rule that is not `off`.
    pub fn category_on(&self, category: &str) -> bool {
        self.categories
            .get(category)
            .map(|rules| rules.iter().any(|r| !r.off))
            .unwrap_or(false)
    }

    /// `@0x101848fb8` — take the pending redirects.
    pub fn take_redirects(&mut self) -> BTreeMap<String, String> {
        std::mem::take(&mut self.redirects)
    }

    /// `@0x10184967c` — every rule, by name, with redirects resolved.
    ///
    /// The recovered body is 5.8 KB: it walks the categories, follows a rule's
    /// `redirect_to` into another rule and keeps the resulting colour.
    pub fn rules(&self) -> Vec<&Rule> {
        self.highlight.values().collect()
    }
}

fn rule_from(v: &Value) -> Option<Rule> {
    if let Some(s) = v.as_str() {
        return Some(Rule {
            pattern: s.to_string(),
            ..Rule::default()
        });
    }
    let t = v.as_table()?;
    let s = |k: &str| t.get(k).and_then(|v| v.as_str()).map(String::from);
    let b = |k: &str| t.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let n = |k: &str| t.get(k).and_then(|v| v.as_i64()).map(|v| v as u32);
    Some(Rule {
        pattern: s("regex").or_else(|| s("pattern")).unwrap_or_default(),
        case_sensitive: b("case_sensitive"),
        whole_word: b("whole_word"),
        color: s("color").and_then(|c| crate::theme::parse_hex(&c)),
        squelch: b("squelch"),
        substitute: s("substitute"),
        status: s("status"),
        sound: s("sound"),
        volume: n("volume"),
        alert: b("alert"),
        notify: b("notify"),
        speak: b("speak"),
        rumble: b("rumble"),
        picture: s("picture"),
        timer: s("timer"),
        seconds: n("seconds"),
        caught: n("caught").unwrap_or(0),
        times: n("times"),
        rearm: n("rearm"),
        character: s("character"),
        stream: s("stream"),
        origin: s("origin"),
        off: b("off"),
        window: s("window"),
        category: s("category").map(|c| Mark::named(&c)).unwrap_or(Mark::None),
    })
}

fn rule_to(rule: &Rule) -> Value {
    let mut t = Table::new();
    t.insert("regex".into(), Value::Str(rule.pattern.clone()));
    if rule.case_sensitive {
        t.insert("case_sensitive".into(), Value::Bool(true));
    }
    if rule.whole_word {
        t.insert("whole_word".into(), Value::Bool(true));
    }
    if let Some(c) = rule.color {
        t.insert("color".into(), Value::Str(crate::theme::hex(c)));
    }
    if rule.squelch {
        t.insert("squelch".into(), Value::Bool(true));
    }
    if let Some(s) = &rule.substitute {
        t.insert("substitute".into(), Value::Str(s.clone()));
    }
    if let Some(s) = &rule.sound {
        t.insert("sound".into(), Value::Str(s.clone()));
    }
    if let Some(n) = rule.volume {
        t.insert("volume".into(), Value::Int(n as i64));
    }
    if let Some(s) = &rule.stream {
        t.insert("stream".into(), Value::Str(s.clone()));
    }
    if rule.off {
        t.insert("off".into(), Value::Bool(true));
    }
    if let Mark::Named(cat) = &rule.category {
        t.insert("category".into(), Value::Str(cat.clone()));
    }
    Value::Table(t)
}

impl Over for Rule {
    fn over(&self, under: &Self) -> Self {
        let mut out = self.clone();
        if out.pattern.is_empty() {
            out.pattern = under.pattern.clone();
        }
        out.color = self.color.or(under.color);
        out.sound = self.sound.clone().or_else(|| under.sound.clone());
        out.substitute = self.substitute.clone().or_else(|| under.substitute.clone());
        out.stream = self.stream.clone().or_else(|| under.stream.clone());
        out
    }
    fn differing(&self, base: &Self) -> Self {
        let mut out = self.clone();
        if out == *base {
            return Rule::default();
        }
        if out.color == base.color {
            out.color = None;
        }
        if out.sound == base.sound {
            out.sound = None;
        }
        out
    }
    fn is_empty(&self) -> bool {
        *self == Rule::default()
    }
    fn from_value(v: &Value) -> Option<Self> {
        rule_from(v)
    }
    fn to_value(&self) -> Value {
        rule_to(self)
    }
}

/// The token a rule paints with when it does not set a colour.
pub fn token_of(rule: &Rule) -> Token {
    match rule.category.name() {
        "creature" => Token::Creature,
        "speech" => Token::Speech,
        "room" => Token::RoomName,
        "combat" => Token::CreatureDeath,
        "channel" => Token::ChannelGeneral,
        "status" => Token::StatusYou,
        "map" => Token::MapRoom,
        _ => Token::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_are_the_recovered_eight() {
        assert_eq!(CATEGORIES.len(), 8);
        assert_eq!(CATEGORIES[0], "creature");
        assert_eq!(CATEGORIES[7], "general");
        assert_eq!(Mark::named("room").name(), "room");
        assert_eq!(Mark::None.name(), "");
    }

    #[test]
    fn general_rules_have_categories() {
        let rules = general();
        assert_eq!(rules.len(), 5);
        assert_eq!(rules[0].pattern, "You also see");
        assert_eq!(rules[0].category.name(), "room");
        assert!(rule_round_trips(&rules[3]));
    }

    fn rule_round_trips(rule: &Rule) -> bool {
        rule_from(&rule_to(rule)).map(|r| r.pattern == rule.pattern).unwrap_or(false)
    }

    #[test]
    fn parse_a_file() {
        let src = r##"
[highlight]
burn = { regex = "You are burning", color = "#ff8800", category = "status" }
quiet = { regex = "nothing happens", off = true }

[redirect]
thoughts = "thoughts"
"##;
        let mut f = HighlightFile::parse(src).unwrap();
        assert_eq!(f.highlight.len(), 2);
        assert_eq!(f.highlight["burn"].color, Some(crate::theme::rgb(0xff, 0x88, 0x00)));
        assert!(f.highlight["quiet"].off);
        let r = f.take_redirects();
        assert_eq!(r.get("thoughts").map(|s| s.as_str()), Some("thoughts"));
        assert!(f.take_redirects().is_empty());
    }

    #[test]
    fn category_on_needs_a_live_rule() {
        let mut f = HighlightFile::default();
        f.categories.insert(
            "room".into(),
            vec![Rule {
                pattern: "x".into(),
                ..Rule::default()
            }],
        );
        f.categories.insert(
            "combat".into(),
            vec![Rule {
                pattern: "y".into(),
                off: true,
                ..Rule::default()
            }],
        );
        assert!(f.category_on("room"));
        assert!(!f.category_on("combat"));
        assert!(!f.category_on("nope"));
    }

    #[test]
    fn from_rules_groups_by_category() {
        let f = HighlightFile::from_rules(general());
        assert_eq!(f.highlight.len(), 5);
        assert!(f.categories.get("room").unwrap().len() >= 2);
        assert!(f.category_on("room"));
    }

    #[test]
    fn to_toml_round_trips() {
        let f = HighlightFile::from_rules(general());
        let back = HighlightFile::parse(&f.to_toml()).unwrap();
        assert_eq!(back.highlight.len(), f.highlight.len());
        assert!(back.highlight.contains_key("You also see"));
    }

    #[test]
    fn rule_tokens() {
        let mut r = Rule::default();
        r.category = Mark::named("creature");
        assert_eq!(token_of(&r), Token::Creature);
        r.category = Mark::named("speech");
        assert_eq!(token_of(&r), Token::Speech);
        r.category = Mark::None;
        assert_eq!(token_of(&r), Token::Text);
    }
}
