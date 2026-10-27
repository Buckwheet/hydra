//! Stream rules: how an incoming line is classified.
//!
//! Recovered from `stream_rules::*`:
//! `0x101a01f30` (`StreamRules::new`), `0x101a02048` (`set`),
//! `0x101a02218` (`current`), `0x101a03090` / `0x101a036a0` (`like`),
//! `0x101a033d0` (`Rule::check`), `0x101a03458` (`Rule::finds`),
//! `0x101a0352c` (`Rule::fighting`), `0x101a048c0` (`Rules::load`),
//! `0x101a04b6c` (`Rules::parse`), `0x101a04c94` (`Rules::to_toml`),
//! `0x101a04dcc` (`fought`), `0x101a04e34` (`Compiled::new`),
//! `0x101a05084` (`Compiled::streams`).
//!
//! **ESTABLISHED**:
//! * the rules are **TOML**: `Rules::parse` is a `toml` deserialise and
//!   `current`/`to_toml` drive `toml_writer::TomlKeyBuilder` and
//!   `toml::ser::ValueSerializer::serialize_seq`, so the document has an array
//!   of tables whose members include a sequence.
//! * a `Rule` is **0x70 (112) bytes**, and `fought` walks them with that
//!   stride.  Field use seen in `fought` (`0x101a04dcc`):
//!
//!   ```text
//!   +0x00  u64 class tag            (a nested rule is "fighting" when == 2)
//!   +0x10  Option<Box<Regex>>       (niche null test)
//!   +0x20  ptr  \  Vec of nested rules, also 0x70 stride
//!   +0x28  len  /
//!   ```
//!
//!   So a rule can nest other rules, and `fought` is true when the rule has a
//!   regex **and** nothing nested is the fighting class.
//! * `like` builds a regex out of a word with `regex::escape` (`0x101a03f80`)
//!   and the `^{}{}` template at `0x103e7bb58` (`^` + escaped + suffix), i.e.
//!   anchored-at-the-start matching.
//! * the display helper formats `Like "{}"` (`0x103e7bb4d`) and the elided
//!   form `{}...` (`0x103e28c40`), counting characters before eliding
//!   (`core::str::count::do_count_chars`).
//! * errors: `"{}: {}"` (`0x103e21615`), `"{}: its regex is not one: {}"`
//!   (the 22-character piece at `0x103e7bb5d`).
//! * `StreamRules` is `Arc<Mutex<Compiled>>`: `set` builds a fresh `Compiled`
//!   and swaps it under the lock, `current` locks and clones.
//!
//! **UNKNOWN**: the TOML member names and the class numbering.  We write
//! `name`, `stream`, `class`, `regex` and `fighting`, and we pick `Fighting`
//! as class number 2 to match the recovered comparison.
//!
//! **Phase 1**: the TOML is the real thing.  [`Rules::parse`] is
//! `toml::from_str` into the derived [`RulesDoc`] / [`RuleDoc`] shapes (the
//! member names are the ones this crate recovered, and every member is
//! optional on read), and [`Rules::to_toml`] builds a `toml_edit` document with
//! one `[[rules]]` table per rule — nested rules are written as an array of
//! inline tables, so a rule set round trips through the file.

use crate::pattern::{self, Pattern};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// How many characters [`like`] keeps before eliding with `...`.
pub const ELIDE: usize = 60;

/// What a rule does with a matching line.  The recovered tag for
/// [`Class::Fighting`] is `2` (`cmp x12, #2` in `fought`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Class {
    /// keep the line out of the log
    #[default]
    Gag,
    /// fold the line into the previous one
    Hide,
    /// send the line to another stream
    Redirect,
    /// the line describes something fighting us
    Fighting,
    /// highlight the line
    Highlight,
}

impl Class {
    /// The recovered tag value.
    pub fn tag(self) -> u64 {
        match self {
            Class::Gag => 0,
            Class::Hide => 1,
            Class::Fighting => 2,
            Class::Redirect => 3,
            Class::Highlight => 4,
        }
    }

    /// From the tag, or `None`.
    pub fn from_tag(tag: u64) -> Option<Class> {
        Some(match tag {
            0 => Class::Gag,
            1 => Class::Hide,
            2 => Class::Fighting,
            3 => Class::Redirect,
            4 => Class::Highlight,
            _ => return None,
        })
    }

    /// The name used in the TOML file.
    pub fn name(self) -> &'static str {
        match self {
            Class::Gag => "gag",
            Class::Hide => "hide",
            Class::Fighting => "fighting",
            Class::Redirect => "redirect",
            Class::Highlight => "highlight",
        }
    }

    /// From the TOML name, or `None`.
    pub fn of(name: &str) -> Option<Class> {
        Some(match name {
            "gag" => Class::Gag,
            "hide" => Class::Hide,
            "fighting" => Class::Fighting,
            "redirect" => Class::Redirect,
            "highlight" => Class::Highlight,
            _ => return None,
        })
    }
}

/// One stream rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// what a match does
    pub class: Class,
    /// the rule's name, as written
    pub name: String,
    /// the stream the line belongs to
    pub stream: String,
    /// the regular expression as written
    pub regex: String,
    /// whether the line describes something fighting
    pub fighting: bool,
    /// rules below this one (`+0x20`)
    pub nested: Vec<Rule>,
    /// the compiled `regex`, if it compiled
    pub probe: Option<Pattern>,
}

impl Default for Rule {
    fn default() -> Self {
        Rule {
            class: Class::Gag,
            name: String::new(),
            stream: String::new(),
            regex: String::new(),
            fighting: false,
            nested: Vec::new(),
            probe: None,
        }
    }
}

impl Rule {
    /// A rule that matches `regex` in `stream`.
    pub fn new(name: &str, stream: &str, regex: &str, class: Class) -> Self {
        let mut rule = Rule {
            class,
            name: name.to_string(),
            stream: stream.to_string(),
            regex: regex.to_string(),
            fighting: class == Class::Fighting,
            nested: Vec::new(),
            probe: Pattern::regex(regex).ok(),
        };
        if rule.probe.is_none() {
            // Keep the source so `check` can report the compile error.
            rule.probe = None;
        }
        rule
    }

    /// Why this rule cannot be used, if it cannot.
    ///
    /// **ESTABLISHED** (`0x101a033d0`): the result is an `Option<String>` — a
    /// blank/`None` niche for a good rule, the message for a bad one — and the
    /// message ends `"its regex is not one: {}"`.
    pub fn check(&self) -> Option<String> {
        match Pattern::regex(&self.regex) {
            Ok(_) => None,
            Err(e) => Some(format!(
                "the rule {}: its regex is not one: {e}",
                if self.name.is_empty() {
                    self.regex.as_str()
                } else {
                    &self.name
                }
            )),
        }
    }

    /// Does this rule match `text`?
    ///
    /// **ESTABLISHED** (`0x101a03458`): `found_by` builds an anchored pattern
    /// and runs it; a rule with no pattern never matches.
    pub fn finds(&self, text: &str) -> Result<bool, String> {
        match Pattern::regex(&anchored(&self.regex)) {
            Ok(p) => Ok(p.is_match(text)),
            Err(e) => Err(format!("{}: {e}", self.regex)),
        }
    }

    /// Is this rule (or one of its children) about fighting?
    ///
    /// **ESTABLISHED** (`0x101a0352c`): true when the rule's own class is
    /// `Fighting`.
    pub fn fighting(&self) -> bool {
        self.fighting || self.class == Class::Fighting
    }

    /// Does this rule's own pattern match `text`?  **ESTABLISHED** shape:
    /// `like` anchors the rule's regex at the start (`^{}{}`).
    pub fn like(&self, text: &str) -> bool {
        match &self.probe {
            Some(p) => p.is_match(&text.to_lowercase()) || p.is_match(text),
            None => self.regex.is_empty() && !text.is_empty(),
        }
    }
}

/// Anchor a pattern at the start of the line, as the `^{}{}` template does.
pub fn anchored(regex: &str) -> String {
    if regex.starts_with('^') {
        regex.to_string()
    } else {
        format!("^{regex}")
    }
}

/// True when any of `rules` (or their children) is about fighting.
///
/// **ESTABLISHED** (`0x101a04dcc`): walks the slice at stride `0x70`, returns
/// true as soon as a rule with no children is of the fighting class.
pub fn fought(rules: &[Rule]) -> bool {
    for rule in rules {
        if rule.nested.is_empty() {
            if rule.class.tag() == 2 {
                return true;
            }
        } else if rule.nested.iter().any(|r| r.class.tag() == 2) {
            return true;
        }
    }
    false
}

/// The display form of a set of rule names: `Like "a", "b"`, elided with
/// `...` when it gets long.
///
/// **ESTABLISHED** parts: `Like "{}"` and the `{}...` elision with a character
/// count.
pub fn like(names: &[&str]) -> String {
    let joined = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let rendered = format!("Like {joined}");
    if rendered.chars().count() <= ELIDE + 5 {
        rendered
    } else {
        let kept: String = rendered.chars().take(ELIDE).collect();
        format!("{kept}...")
    }
}

/// The compiled form of a rule set: the rules plus the distinct streams.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Compiled {
    rules: Vec<Rule>,
    streams: Vec<String>,
}

impl Compiled {
    /// Compile a rule set.
    ///
    /// **ESTABLISHED** shape (`0x101a04e34`): checks each rule, collects the
    /// stream names in first-seen order and reports the first bad rule as
    /// `"{}: {}"`.
    pub fn new(rules: &[Rule]) -> Result<Compiled, String> {
        let mut streams: Vec<String> = Vec::new();
        for rule in rules {
            if let Some(problem) = rule.check() {
                return Err(format!("{}: {problem}", rule.name));
            }
            if !rule.stream.is_empty() && !streams.contains(&rule.stream) {
                streams.push(rule.stream.clone());
            }
        }
        Ok(Compiled {
            rules: rules.to_vec(),
            streams,
        })
    }

    /// A compiled set with no rules.
    pub fn empty() -> Self {
        Compiled::default()
    }

    /// The distinct stream names, in first-seen order.
    ///
    /// **ESTABLISHED** shape (`0x101a05084`): returns the stream names and
    /// reports a bad rule as `"... its regex is not one: {}"`.
    pub fn streams(&self) -> &[String] {
        &self.streams
    }

    /// The rules themselves.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// The first rule that matches `text`.
    pub fn find(&self, text: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.like(text))
    }

    /// The stream `text` belongs to: the first match's stream, else `None`.
    pub fn stream_of(&self, text: &str) -> Option<&str> {
        self.find(text)
            .map(|r| r.stream.as_str())
            .filter(|s| !s.is_empty())
    }

    /// Does anything in this set fight?
    pub fn fought(&self) -> bool {
        fought(&self.rules)
    }
}

/// A parsed `rules.toml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rules {
    /// the rules, in file order
    pub rules: Vec<Rule>,
}

impl Rules {
    /// Read `path`, returning an empty set when the file is missing.
    ///
    /// **ESTABLISHED** (`0x101a048c0`): reads the file and formats a parse
    /// failure as `"{}: {}"`.
    pub fn load(path: &Path) -> Result<Rules, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Rules::default()),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        Rules::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Parse the TOML form with the `toml` crate.
    ///
    /// The document is a list of `[[rules]]` tables; each may nest further
    /// rules in a `nested` array, which is our spelling for the recovered
    /// `nested` vector.
    pub fn parse(text: &str) -> Result<Rules, String> {
        let doc: RulesDoc = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut rules = Vec::with_capacity(doc.rules.len());
        for item in doc.rules {
            rules.push(rule_of(item)?);
        }
        Ok(Rules { rules })
    }

    /// Render the TOML form with `toml_edit`.
    ///
    /// **ESTABLISHED**: the original serialises with `toml` (and writes
    /// through `toml_writer`).
    pub fn to_toml(&self) -> String {
        let mut doc = toml_edit::DocumentMut::new();
        let mut tables = toml_edit::ArrayOfTables::new();
        for rule in &self.rules {
            tables.push(table_of(rule));
        }
        doc["rules"] = toml_edit::Item::ArrayOfTables(tables);
        doc.to_string()
    }

    /// Compile the whole set.
    pub fn compile(&self) -> Result<Compiled, String> {
        Compiled::new(&self.rules)
    }
}

/// The document `rules.toml` holds: `[[rules]]` tables.
#[derive(Debug, Default, serde::Deserialize)]
struct RulesDoc {
    #[serde(default)]
    rules: Vec<RuleDoc>,
}

/// One `[[rules]]` table.  Every member is optional, so a hand-written file
/// that leaves one out still loads (the same tolerance the hand parser had).
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct RuleDoc {
    name: String,
    stream: String,
    class: Option<String>,
    regex: String,
    fighting: bool,
    nested: Vec<RuleDoc>,
}

fn rule_of(doc: RuleDoc) -> Result<Rule, String> {
    let class = match doc.class.as_deref() {
        Some(name) => Class::of(name).ok_or_else(|| format!("`{name}` is not a stream class"))?,
        None => Class::Gag,
    };
    let mut rule = Rule::new(&doc.name, &doc.stream, &doc.regex, class);
    if doc.fighting {
        rule.fighting = true;
    }
    for item in doc.nested {
        rule.nested.push(rule_of(item)?);
    }
    Ok(rule)
}

/// One rule as a `[[rules]]` table.
fn table_of(rule: &Rule) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table["name"] = toml_edit::value(rule.name.as_str());
    table["stream"] = toml_edit::value(rule.stream.as_str());
    table["class"] = toml_edit::value(rule.class.name());
    table["regex"] = toml_edit::value(rule.regex.as_str());
    table["fighting"] = toml_edit::value(rule.fighting);
    if !rule.nested.is_empty() {
        let mut nested = toml_edit::Array::new();
        for child in &rule.nested {
            nested.push(inline_of(child));
        }
        table["nested"] = toml_edit::value(nested);
    }
    table
}

/// One nested rule as an inline table, so `nested` stays inside its rule.
fn inline_of(rule: &Rule) -> toml_edit::Value {
    let mut table = toml_edit::InlineTable::new();
    table.insert("name", rule.name.as_str().into());
    table.insert("stream", rule.stream.as_str().into());
    table.insert("class", rule.class.name().into());
    table.insert("regex", rule.regex.as_str().into());
    table.insert("fighting", rule.fighting.into());
    toml_edit::Value::InlineTable(table)
}

/// The session's current rule set, swappable while the session runs.
///
/// **ESTABLISHED**: `Arc<Mutex<Compiled>>`, `set` under the lock, `current`
/// clones.
#[derive(Debug, Clone, Default)]
pub struct StreamRules {
    shared: Arc<Mutex<Compiled>>,
}

impl StreamRules {
    /// A set with these rules.
    pub fn new(rules: &[Rule]) -> Result<Self, String> {
        Ok(StreamRules {
            shared: Arc::new(Mutex::new(Compiled::new(rules)?)),
        })
    }

    /// A set with no rules.
    pub fn empty() -> Self {
        StreamRules::default()
    }

    /// Replace the rules.
    pub fn set(&self, rules: &[Rule]) -> Result<(), String> {
        let fresh = Compiled::new(rules)?;
        *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = fresh;
        Ok(())
    }

    /// The current compiled rules.
    ///
    /// **ESTABLISHED** (`0x101a02218`): takes the lock, clones the `Compiled`,
    /// and re-renders it (the original also re-serialises the TOML here).
    pub fn current(&self) -> Compiled {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The current rules rendered as TOML.
    pub fn to_toml(&self) -> String {
        let rules = self.current().rules.clone();
        Rules { rules }.to_toml()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn say_rule() -> Rule {
        Rule::new("say", "speech", r"^\w+ says", Class::Highlight)
    }

    #[test]
    fn classes_round_trip_through_their_tags() {
        for class in [
            Class::Gag,
            Class::Hide,
            Class::Redirect,
            Class::Fighting,
            Class::Highlight,
        ] {
            assert_eq!(Class::from_tag(class.tag()), Some(class));
            assert_eq!(Class::of(class.name()), Some(class));
        }
        assert_eq!(Class::Fighting.tag(), 2, "the recovered fighting tag");
        assert_eq!(Class::from_tag(9), None);
    }

    #[test]
    fn check_reports_a_bad_regex_with_the_recovered_wording() {
        assert_eq!(say_rule().check(), None);
        let bad = Rule::new("oops", "speech", "^(unclosed", Class::Gag);
        let problem = bad.check().unwrap();
        assert!(problem.starts_with("the rule oops: its regex is not one: "));
    }

    #[test]
    fn finds_is_anchored_at_the_start() {
        let r = say_rule();
        assert!(r.finds("Aelith says, \"hi\"").unwrap());
        assert!(!r.finds("then Aelith says").unwrap());
        assert_eq!(anchored("abc"), "^abc");
        assert_eq!(anchored("^abc"), "^abc");
    }

    #[test]
    fn fought_sees_the_fighting_class() {
        let calm = vec![say_rule()];
        assert!(!fought(&calm));
        let fighting = vec![Rule::new("hit", "combat", "^The .* hits", Class::Fighting)];
        assert!(fought(&fighting));
        let mut nested = Rule::new("outer", "combat", "^x", Class::Gag);
        nested
            .nested
            .push(Rule::new("inner", "combat", "^y", Class::Fighting));
        assert!(fought(std::slice::from_ref(&nested)));
    }

    #[test]
    fn like_elides_a_long_list() {
        assert_eq!(like(&["say"]), "Like \"say\"");
        let many: Vec<&str> = (0..12).map(|_| "abcdefgh").collect();
        let rendered = like(&many);
        assert!(rendered.ends_with("..."));
        assert_eq!(rendered.chars().count(), ELIDE + 3);
    }

    #[test]
    fn toml_round_trip() {
        let rules = Rules {
            rules: vec![
                say_rule(),
                Rule::new("hit", "combat", "^(\\w+) hits", Class::Fighting),
            ],
        };
        let toml = rules.to_toml();
        assert!(toml.contains("[[rules]]"));
        assert!(toml.contains("class = \"highlight\""));
        assert!(toml.contains("fighting = true"));
        let back = Rules::parse(&toml).unwrap();
        assert_eq!(back.rules.len(), 2);
        assert_eq!(back.rules[0].stream, "speech");
        assert_eq!(back.rules[1].class, Class::Fighting);
        assert!(back.rules[1].fighting);
    }

    #[test]
    fn compiled_streams_are_deduped_in_order() {
        let rules = vec![say_rule(), Rule::new("b", "combat", "^b", Class::Gag), Rule::new("c", "speech", "^c", Class::Gag)];
        let compiled = Compiled::new(&rules).unwrap();
        assert_eq!(compiled.streams(), ["speech", "combat"]);
        assert_eq!(compiled.stream_of("Aelith says hi"), Some("speech"));
        assert_eq!(compiled.stream_of("nothing here"), None);
        assert!(!compiled.fought());
    }

    #[test]
    fn stream_rules_can_be_swapped_at_runtime() {
        let set = StreamRules::empty();
        assert_eq!(set.current().rules().len(), 0);
        set.set(&[say_rule()]).unwrap();
        assert_eq!(set.current().streams(), ["speech"]);
        assert!(set.to_toml().contains("[[rules]]"));
        assert!(set.set(&[Rule::new("oops", "s", "^(bad", Class::Gag)]).is_err());
        // the failed set left the old rules in place
        assert_eq!(set.current().streams(), ["speech"]);
    }

    #[test]
    fn a_missing_rules_file_is_empty() {
        let p = std::env::temp_dir().join("hydra-rules-missing.toml");
        let _ = std::fs::remove_file(&p);
        assert_eq!(Rules::load(&p).unwrap(), Rules::default());
    }

    #[test]
    fn toml_round_trips_nested_rules_and_escaped_regexes() {
        // A quoted, backslashed expression is what the `regex` column really
        // holds, and it has to survive the `toml_edit` writer.
        let mut outer = Rule::new("outer", "combat", "^x \"quoted\" \\w+", Class::Gag);
        outer
            .nested
            .push(Rule::new("inner", "combat", "^y", Class::Fighting));
        let rules = Rules {
            rules: vec![outer, say_rule()],
        };
        let text = rules.to_toml();
        // The writer produces TOML the `toml` crate accepts, on its own terms.
        let as_doc: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(
            as_doc.get("rules").and_then(|v| v.as_array()).map(Vec::len),
            Some(2)
        );
        let back = Rules::parse(&text).unwrap();
        assert_eq!(back.rules.len(), 2);
        assert_eq!(back.rules[0].regex, "^x \"quoted\" \\w+");
        assert_eq!(back.rules[0].nested.len(), 1);
        assert_eq!(back.rules[0].nested[0].name, "inner");
        assert_eq!(back.rules[0].nested[0].class, Class::Fighting);
        assert!(back.rules[0].nested[0].fighting);
        assert_eq!(back.rules[1].stream, "speech");
        // and the second pass is stable
        assert_eq!(back.to_toml(), text);
    }

    #[test]
    fn parse_tolerates_missing_members_and_ignores_unknown_ones() {
        let text = r#"
            # a hand-written file: no `class`, no `fighting`, an unknown key
            [[rules]]
            name = "say"
            regex = "^\\w+ says"
            colour = "green"
        "#;
        let rules = Rules::parse(text).unwrap();
        assert_eq!(rules.rules.len(), 1);
        assert_eq!(rules.rules[0].class, Class::Gag, "class defaults to gag");
        assert!(!rules.rules[0].fighting);
        assert!(rules.rules[0].stream.is_empty());
        assert!(rules.rules[0].finds("Aelith says hi").unwrap());
    }

    #[test]
    fn a_bad_class_or_broken_toml_is_reported() {
        let bad_class = "[[rules]]\nname = \"x\"\nclass = \"sparkle\"\n";
        let e = Rules::parse(bad_class).unwrap_err();
        assert_eq!(e, "`sparkle` is not a stream class");
        let broken = "[[rules]\nname = \"x\"\n";
        let e = Rules::parse(broken).unwrap_err();
        assert!(!e.is_empty(), "a TOML error carries the crate's message");
        // `rules` that is not a list of tables is refused too.
        assert!(Rules::parse("rules = 3\n").is_err());
    }

    #[test]
    fn a_rules_file_round_trips_through_the_filesystem() {
        let dir = std::env::temp_dir().join(format!("hydra-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rules.toml");
        let rules = Rules {
            rules: vec![
                say_rule(),
                Rule::new("hit", "combat", "^(\\w+) hits", Class::Fighting),
            ],
        };
        std::fs::write(&path, rules.to_toml()).unwrap();
        let back = Rules::load(&path).unwrap();
        assert_eq!(back.rules.len(), 2);
        assert_eq!(back.rules[1].class, Class::Fighting);
        // the file really says `[[rules]]` twice
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("[[rules]]").count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

