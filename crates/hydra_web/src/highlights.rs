//! Highlight rules for the web page.
//!
//! Recovered from:
//!
//! * `highlights::listed` (`0x1002c9938`), `sounds` (`0x1002c99f8`), `changed`
//!   (`0x1002c9b10`), `save` (`0x1002c8350`), `merge` (`0x1002c93b8`);
//! * `Form::of` (`0x1002c6914`), `Form::worn` (`0x1002c74f8`),
//!   `MarkForm::mark` (`0x1002c9c80`);
//! * the serde glue of `Form` (`0x1002c6140`, 33 fields), `RuleOut`
//!   (`0x1002c6588`), `MarkForm` (`0x1002c66c0`) and `CategoryOut`
//!   (`0x1002c67f8`).

use std::path::{Path, PathBuf};

use crate::json::Value;

/// File name under the storage root (`0x1020725b9`, 15 bytes).
pub const FILE: &str = "highlights.toml";
/// Sub-directory holding the sound files (`0x1020725b3`, 6 bytes).
pub const SOUNDS: &str = "sounds";
/// Largest highlight document the page may merge, in bytes.
///
/// `// UNKNOWN:` the limit itself; the refusal string
/// ("The copy is too big to merge.") is recovered from `Page::highlights_merge`
/// (`0x1002b43d8`).
pub const MAX_MERGE: usize = 0x1_0000;

/// A highlight rule as the page sees it.
///
/// The original is `hydra_ui::highlight::file::Rule`; that crate is not
/// recovered, so the fields [`Form::worn`] writes are modelled here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rule {
    pub pattern: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub color: String,
    pub background: String,
    pub line: bool,
    pub squelch: bool,
    pub substitute: String,
    pub sound: String,
    pub alert: String,
    pub notify: String,
    pub push: String,
    pub send: String,
    pub when: String,
    pub rearm: String,
    pub timer: String,
    pub seconds: u32,
    pub caught: String,
    pub times: u32,
    pub down: String,
    pub list: String,
    pub flag: String,
    pub status: String,
    pub rumble: String,
    pub picture: String,
    pub cooldown: String,
    pub volume: u32,
    pub speak: String,
    pub characters: String,
    pub stream: String,
    pub off: bool,
}

/// The category list entry (`CategoryOut`): `name, on`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct CategoryOut {
    pub name: String,
    /// How many rules the category has enabled (`on`).
    pub on: u32,
}

impl CategoryOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("on", Value::int(self.on)),
        ]
    }
}

/// One rule row (`RuleOut`): `name, form, problem`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RuleOut {
    pub name: String,
    pub form: Form,
    /// Why the rule cannot be saved, if it cannot.
    pub problem: Option<String>,
}

impl RuleOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("form", crate::wire::object(self.form.fields())),
            (
                "problem",
                match &self.problem {
                    Some(p) => Value::str(p.clone()),
                    None => Value::Null,
                },
            ),
        ]
    }
}

/// The mark part of a highlight (`MarkForm`): `name, seconds, clear`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MarkForm {
    pub name: String,
    pub seconds: u32,
    pub clear: bool,
}

impl MarkForm {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("seconds", Value::int(self.seconds)),
            ("clear", Value::Bool(self.clear)),
        ]
    }

    /// The mark, or `None` for a blank name (`0x1002c9c80`).
    ///
    /// Recovered: the name is trimmed; an empty name returns `None` (the
    /// `isize::MIN + 1` niche), otherwise a mark is built from the name,
    /// `seconds` and `clear`.
    pub fn mark(&self) -> Option<Rule> {
        let name = self.name.trim();
        if name.is_empty() {
            return None;
        }
        Some(Rule {
            pattern: name.to_string(),
            seconds: self.seconds,
            off: self.clear,
            ..Rule::default()
        })
    }
}

/// The whole highlight editor form (`Form`).
///
/// The 33 field names **and** the serialization order are recovered from
/// `Serialize for highlights::Form` (`0x1002c6140`).  The fields are declared in
/// that serialization order — `serde` writes struct fields in declaration order
/// and [`Form::fields`] lists the same order — so the wire document is the
/// recovered one.  The original's *memory* layout was different; the offsets in
/// the comments are that layout.
///
/// `// UNKNOWN:` the field types of the recovered 16-byte slots (`rearm`,
/// `cooldown`).  They are modelled as `String`; everything at a 24-byte offset is
/// a `String`, the tail flags are `bool`s and `seconds`/`times`/`volume` are
/// numbers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
// The page sends the form's fields next to the `"t"` tag (`In::HighlightSave`
// flattens it), so a missing field must default rather than fail the parse.
#[serde(default)]
pub struct Form {
    pub category: String,     // +0x20
    pub find: String,         // +0x38
    pub pattern: bool,        // +0x282
    pub case_sensitive: bool, // +0x283
    pub whole_word: bool,     // +0x284
    pub color: String,        // +0x50
    pub background: String,   // +0x68
    pub line: bool,           // +0x285
    pub squelch: bool,        // +0x286
    pub substitute: String,   // +0x80
    pub sound: String,        // +0x98
    pub alert: String,        // +0x188
    pub notify: String,       // +0x1a0
    pub push: String,         // +0x1b8
    pub send: String,         // +0xb0
    pub when: String,         // +0x238
    pub rearm: String,        // +0x00
    pub timer: String,        // +0xc8
    pub seconds: u32,         // +0x270
    pub caught: String,       // +0xe0
    pub times: u32,           // +0x278
    pub down: String,         // +0xf8
    pub list: String,         // +0x110
    pub flag: String,         // +0x1d0
    pub status: String,       // +0x1f8
    pub rumble: String,       // +0x128
    pub picture: String,      // +0x140
    pub cooldown: String,     // +0x10
    pub volume: u16,          // +0x280
    pub speak: String,        // +0x220
    pub characters: String,   // +0x158
    pub stream: String,       // +0x170
    pub off: bool,            // +0x287
}

impl Form {
    /// The JSON fields in the recovered serialization order.
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("category", Value::str(self.category.clone())),
            ("find", Value::str(self.find.clone())),
            ("pattern", Value::Bool(self.pattern)),
            ("case_sensitive", Value::Bool(self.case_sensitive)),
            ("whole_word", Value::Bool(self.whole_word)),
            ("color", Value::str(self.color.clone())),
            ("background", Value::str(self.background.clone())),
            ("line", Value::Bool(self.line)),
            ("squelch", Value::Bool(self.squelch)),
            ("substitute", Value::str(self.substitute.clone())),
            ("sound", Value::str(self.sound.clone())),
            ("alert", Value::str(self.alert.clone())),
            ("notify", Value::str(self.notify.clone())),
            ("push", Value::str(self.push.clone())),
            ("send", Value::str(self.send.clone())),
            ("when", Value::str(self.when.clone())),
            ("rearm", Value::str(self.rearm.clone())),
            ("timer", Value::str(self.timer.clone())),
            ("seconds", Value::int(self.seconds)),
            ("caught", Value::str(self.caught.clone())),
            ("times", Value::int(self.times)),
            ("down", Value::str(self.down.clone())),
            ("list", Value::str(self.list.clone())),
            ("flag", Value::str(self.flag.clone())),
            ("status", Value::str(self.status.clone())),
            ("rumble", Value::str(self.rumble.clone())),
            ("picture", Value::str(self.picture.clone())),
            ("cooldown", Value::str(self.cooldown.clone())),
            ("volume", Value::int(self.volume)),
            ("speak", Value::str(self.speak.clone())),
            ("characters", Value::str(self.characters.clone())),
            ("stream", Value::str(self.stream.clone())),
            ("off", Value::Bool(self.off)),
        ]
    }

    /// Read a `Form` out of an `In::HighlightSave` message.
    ///
    /// The page splices the form's fields next to the `"t"` tag, so the message
    /// *is* a form document apart from that tag (which the derived
    /// `Deserialize` ignores).  Missing fields default (`#[serde(default)]`).
    pub fn from_value(v: &Value) -> Result<Form, String> {
        serde_json::from_str(&v.to_json()).map_err(|e| e.to_string())
    }

    /// Build the form for a stored rule (`0x1002c6914`).
    ///
    /// Recovered: every rule field is cloned into the form;
    /// `hydra_ui::condition::Condition::clone` and `join(", ")` are used for the
    /// conditions column, which the web form keeps as `when`.
    pub fn of(rule: &Rule, category: &str, conditions: &[String]) -> Form {
        Form {
            category: category.to_string(),
            find: rule.pattern.clone(),
            pattern: true,
            case_sensitive: rule.case_sensitive,
            whole_word: rule.whole_word,
            color: rule.color.clone(),
            background: rule.background.clone(),
            line: rule.line,
            squelch: rule.squelch,
            substitute: rule.substitute.clone(),
            sound: rule.sound.clone(),
            alert: rule.alert.clone(),
            notify: rule.notify.clone(),
            push: rule.push.clone(),
            send: rule.send.clone(),
            when: if conditions.is_empty() {
                rule.when.clone()
            } else {
                conditions.join(", ")
            },
            rearm: rule.rearm.clone(),
            timer: rule.timer.clone(),
            seconds: rule.seconds,
            caught: rule.caught.clone(),
            times: rule.times,
            down: rule.down.clone(),
            list: rule.list.clone(),
            flag: rule.flag.clone(),
            status: rule.status.clone(),
            rumble: rule.rumble.clone(),
            picture: rule.picture.clone(),
            cooldown: rule.cooldown.clone(),
            volume: rule.volume as u16,
            speak: rule.speak.clone(),
            characters: rule.characters.clone(),
            stream: rule.stream.clone(),
            off: rule.off,
        }
    }

    /// Write the form back into a rule (`0x1002c74f8`).
    ///
    /// Recovered: the pattern is `find.trim()`-ed and cloned field by field;
    /// `MarkForm::mark` is used for the mark part and `Vec::from_iter` for the
    /// character list.
    pub fn worn(&self) -> Rule {
        Rule {
            pattern: self.find.trim().to_string(),
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            color: self.color.clone(),
            background: self.background.clone(),
            line: self.line,
            squelch: self.squelch,
            substitute: self.substitute.clone(),
            sound: self.sound.clone(),
            alert: self.alert.clone(),
            notify: self.notify.clone(),
            push: self.push.clone(),
            send: self.send.clone(),
            when: self.when.clone(),
            rearm: self.rearm.clone(),
            timer: self.timer.clone(),
            seconds: self.seconds,
            caught: self.caught.clone(),
            times: self.times,
            down: self.down.clone(),
            list: self.list.clone(),
            flag: self.flag.clone(),
            status: self.status.clone(),
            rumble: self.rumble.clone(),
            picture: self.picture.clone(),
            cooldown: self.cooldown.clone(),
            volume: u32::from(self.volume),
            speak: self.speak.clone(),
            characters: self.characters.clone(),
            stream: self.stream.clone(),
            off: self.off,
        }
    }

    /// Is this form a usable rule?
    pub fn problem(&self) -> Option<String> {
        if self.find.trim().is_empty() {
            Some("the rule needs something to find".to_string())
        } else {
            None
        }
    }
}

/// A highlight file: categories, each with its rules.
///
/// Stand-in for `hydra_ui::highlight::file::HighlightFile` (unrecovered).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HighlightFile {
    pub categories: std::collections::BTreeMap<String, Vec<Rule>>,
}

impl HighlightFile {
    /// `HighlightFile::category_names`.
    pub fn category_names(&self) -> Vec<String> {
        self.categories.keys().cloned().collect()
    }

    /// `HighlightFile::get`.
    pub fn get(&self, category: &str, name: &str) -> Option<&Rule> {
        self.categories
            .get(category)?
            .iter()
            .find(|r| r.pattern == name)
    }

    /// `HighlightFile::named` + `put`.
    pub fn put(&mut self, category: &str, rule: Rule) {
        let rules = self.categories.entry(category.to_string()).or_default();
        match rules.iter().position(|r| r.pattern == rule.pattern) {
            Some(at) => rules[at] = rule,
            None => rules.push(rule),
        }
    }

    /// The number of enabled rules in a category.
    pub fn on(&self, category: &str) -> u32 {
        self.categories
            .get(category)
            .map(|rules| rules.iter().filter(|r| !r.off).count() as u32)
            .unwrap_or(0)
    }
}

impl HighlightFile {
    /// Remove a rule by pattern (`HighlightFile::remove` / `In::HighlightRemove`).
    pub fn remove(&mut self, name: &str) -> bool {
        for rules in self.categories.values_mut() {
            if let Some(at) = rules.iter().position(|r| r.pattern == name) {
                rules.remove(at);
                return true;
            }
        }
        false
    }

    /// Switch a rule on or off (`HighlightFile::switch` / `In::HighlightSwitch`).
    ///
    /// `on` is the page's word: `false` sets the rule's `off` flag.
    pub fn switch(&mut self, name: &str, on: bool) -> bool {
        for rules in self.categories.values_mut() {
            if let Some(rule) = rules.iter_mut().find(|r| r.pattern == name) {
                rule.off = !on;
                return true;
            }
        }
        false
    }
}

/// The path of the highlight document.
pub fn file(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// The category/sound lists the editor needs (`0x1002c9938`).
///
/// Recovered: `HighlightFile::category_names` feeds a `Vec<CategoryOut>`
/// (`from_iter`) and the rules feed a `Vec<RuleOut>`.
pub fn listed(file: &HighlightFile) -> (Vec<CategoryOut>, Vec<RuleOut>) {
    let categories = file
        .category_names()
        .into_iter()
        .map(|name| {
            let on = file.on(&name);
            CategoryOut { name, on }
        })
        .collect();
    let mut rules = Vec::new();
    for (category, list) in &file.categories {
        for rule in list {
            rules.push(RuleOut {
                name: rule.pattern.clone(),
                form: Form::of(rule, category, &[]),
                problem: None,
            });
        }
    }
    (categories, rules)
}

/// The sound file names under `root/sounds`, sorted (`0x1002c99f8`).
///
/// Recovered: `root.join("sounds")`, `read_dir`, collect the entry names into a
/// `Vec<String>` and sort (`insertion_sort_shift_left` / `driftsort_main`).  A
/// missing directory yields an empty list (the error path returns `Vec::new()`).
///
/// `// UNKNOWN:` the original collected *file names*; whether it filtered by
/// extension is not visible, so everything is listed.
pub fn sounds(root: &Path) -> Vec<String> {
    let dir = root.join(SOUNDS);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    out.sort();
    out
}

/// When the highlight document last changed (`0x1002c9b10`).
///
/// Recovered: `root.join("highlights.toml")`, `metadata`, `modified` and
/// `duration_since(UNIX_EPOCH)`; any failure yields `None`.  Only the whole
/// seconds are returned (the nanoseconds word is not read).
pub fn changed(root: &Path) -> Option<u64> {
    let meta = std::fs::metadata(root.join(FILE)).ok()?;
    let when = meta.modified().ok()?;
    let since = when.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(since.as_secs())
}

/// Apply a batch of edited forms (`0x1002c8350`).
///
/// Recovered: each form is turned into a rule with `Form::worn`, then
/// `HighlightFile::put` stores it under its category (`named` + `put`).  The
/// original returned the `format!`-ed text of the resulting file.
pub fn save(file: &mut HighlightFile, forms: &[Form]) -> String {
    for form in forms {
        let rule = form.worn();
        let category = if form.category.is_empty() {
            "Uncategorised".to_string()
        } else {
            form.category.clone()
        };
        file.put(&category, rule);
    }
    render(file)
}

/// Merge a pasted highlight document into `file` (`0x1002c93b8`).
///
/// Recovered: the pasted text is parsed with `HighlightFile::parse`, then every
/// parsed rule is inserted with `BTreeMap::insert` / `named` + `put`.  The size
/// check is what produces "The copy is too big to merge."
/// (`Page::highlights_merge`).
pub fn merge(file: &mut HighlightFile, text: &str) -> Result<String, String> {
    if text.len() > MAX_MERGE {
        return Err("The copy is too big to merge.".to_string());
    }
    let incoming = parse(text)?;
    for (category, rules) in incoming.categories {
        for rule in rules {
            file.put(&category, rule);
        }
    }
    Ok(render(file))
}

/// `HighlightFile::parse` — the recovered document format.
///
/// `// UNKNOWN:` the on-disk syntax.  The binary's parser is not readable in
/// isolation, so the safest round-trippable form is used: one
/// `category|pattern|color|background|squelch` line per rule, documented here and
/// in `NOTES.md`.
pub fn parse(text: &str) -> Result<HighlightFile, String> {
    let mut file = HighlightFile::default();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() < 2 {
            return Err(format!("line {}: expected `category|pattern`", n + 1));
        }
        file.put(
            fields[0],
            Rule {
                pattern: fields[1].to_string(),
                color: fields.get(2).copied().unwrap_or("").to_string(),
                background: fields.get(3).copied().unwrap_or("").to_string(),
                squelch: fields.get(4).map(|v| *v == "yes").unwrap_or(false),
                ..Rule::default()
            },
        );
    }
    Ok(file)
}

/// Render a highlight file back to text (the inverse of [`parse`]).
pub fn render(file: &HighlightFile) -> String {
    let mut out = String::new();
    for (category, rules) in &file.categories {
        for rule in rules {
            out.push_str(&format!(
                "{category}|{}|{}|{}|{}\n",
                rule.pattern,
                rule.color,
                rule.background,
                if rule.squelch { "yes" } else { "no" },
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra-web-hl-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn form_has_the_thirty_three_recovered_fields() {
        let f = Form::default();
        let fields = f.fields();
        assert_eq!(fields.len(), 33);
        let names: Vec<&str> = fields.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            &names[..10],
            &[
                "category",
                "find",
                "pattern",
                "case_sensitive",
                "whole_word",
                "color",
                "background",
                "line",
                "squelch",
                "substitute"
            ]
        );
        assert_eq!(*names.last().unwrap(), "off");
        let json = crate::wire::object(f.fields()).to_json();
        assert!(json.starts_with(r#"{"category":"","find":"","pattern":false"#));
    }

    #[test]
    fn mark_form_shape_and_mark() {
        let m = MarkForm {
            name: "  watch  ".into(),
            seconds: 30,
            clear: false,
        };
        assert_eq!(
            crate::wire::object(m.fields()).to_json(),
            r#"{"name":"  watch  ","seconds":30,"clear":false}"#
        );
        let rule = m.mark().unwrap();
        assert_eq!(rule.pattern, "watch");
        assert_eq!(rule.seconds, 30);
        assert!(MarkForm::default().mark().is_none());
    }

    #[test]
    fn form_of_and_worn_round_trip() {
        let rule = Rule {
            pattern: " a kobold".into(),
            color: "#ff0000".into(),
            squelch: true,
            seconds: 12,
            ..Rule::default()
        };
        let form = Form::of(&rule, "Combat", &["at night".to_string(), "in town".to_string()]);
        assert_eq!(form.category, "Combat");
        assert!(form.pattern);
        assert_eq!(form.when, "at night, in town");
        assert_eq!(form.seconds, 12);
        let back = form.worn();
        assert_eq!(back.pattern, "a kobold"); // trimmed
        assert_eq!(back.color, "#ff0000");
        assert!(back.squelch);
        assert_eq!(back.when, "at night, in town");
    }

    #[test]
    fn listed_reports_categories_with_counts() {
        let mut file = HighlightFile::default();
        file.put(
            "Combat",
            Rule {
                pattern: "kobold".into(),
                ..Rule::default()
            },
        );
        file.put(
            "Combat",
            Rule {
                pattern: "orc".into(),
                off: true,
                ..Rule::default()
            },
        );
        let (cats, rules) = listed(&file);
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "Combat");
        assert_eq!(cats[0].on, 1);
        assert_eq!(
            crate::wire::object(cats[0].fields()).to_json(),
            r#"{"name":"Combat","on":1}"#
        );
        assert_eq!(rules.len(), 2);
        assert_eq!(
            crate::wire::object(rules[0].fields()).to_json().split(',').next().unwrap(),
            r#"{"name":"kobold""#
        );
    }

    #[test]
    fn merge_refuses_a_too_big_copy_and_merges_otherwise() {
        let mut file = HighlightFile::default();
        let err = merge(&mut file, &"x".repeat(MAX_MERGE + 1)).unwrap_err();
        assert_eq!(err, "The copy is too big to merge.");

        let text = parse("Combat|kobold|#ff0000||yes\nTrade|silver\n").unwrap();
        assert_eq!(text.category_names(), vec!["Combat", "Trade"]);
        let out = merge(&mut file, "Combat|kobold|#ff0000||yes\n").unwrap();
        assert_eq!(out, "Combat|kobold|#ff0000||yes\n");
        assert_eq!(file.get("Combat", "kobold").unwrap().color, "#ff0000");
        assert!(file.get("Combat", "kobold").unwrap().squelch);
    }

    #[test]
    fn parse_reports_bad_lines() {
        assert!(parse("no-separator").is_err());
        assert_eq!(parse("# comment\n\n").unwrap().categories.len(), 0);
    }

    #[test]
    fn save_puts_forms_and_renders() {
        let mut file = HighlightFile::default();
        let form = Form {
            category: "Combat".into(),
            find: "kobold".into(),
            color: "#00ff00".into(),
            squelch: true,
            ..Form::default()
        };
        let text = save(&mut file, std::slice::from_ref(&form));
        assert_eq!(text, "Combat|kobold|#00ff00||yes\n");
        // Saving the same pattern twice replaces, it does not duplicate.
        save(&mut file, &[form]);
        assert_eq!(file.categories["Combat"].len(), 1);
    }

    #[test]
    fn sounds_are_sorted_and_changed_tracks_the_file() {
        let dir = scratch("sounds");
        assert!(sounds(&dir).is_empty());
        std::fs::create_dir_all(dir.join(SOUNDS)).unwrap();
        for name in ["b.ogg", "a.ogg"] {
            std::fs::write(dir.join(SOUNDS).join(name), b"x").unwrap();
        }
        assert_eq!(sounds(&dir), vec!["a.ogg", "b.ogg"]);
        assert_eq!(changed(&dir), None);
        std::fs::write(dir.join(FILE), "Combat|kobold\n").unwrap();
        assert!(changed(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
