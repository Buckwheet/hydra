//! Macros: named runs of commands, chained with delays.
//!
//! Recovered field names (0x103995d07 and 0x103993400):
//!
//! ```text
//! struct MacroFile   struct MacroDef   struct MacroState
//! label fg bg  chain key set alias when repeat_after once_per_room
//! tooltip countdown states macros unavailable
//! ```
//!
//! `MacroFile::{load, merged, named, parse, to_toml}` read `macros.toml`;
//! `Runs::{allows, note}` decide whether a command may run now.

use std::collections::BTreeMap;

use crate::layer::Over;
use crate::theme::Rgb;
use crate::toml::{self, Table, Value};

/// One macro definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MacroDef {
    pub label: String,
    /// The button's foreground colour.
    pub fg: Option<Rgb>,
    /// The button's background colour.
    pub bg: Option<Rgb>,
    /// The commands, in order.
    pub chain: Vec<String>,
    /// The key that fires it, if any.
    pub key: Option<String>,
    /// The `set` the macro belongs to (a named macro bar).
    pub set: Option<String>,
    /// An alias this macro also answers to.
    pub alias: Option<String>,
    /// Only run when this condition holds.
    pub when: Option<String>,
    /// Repeat this many seconds after the last run.
    pub repeat_after: Option<f64>,
    /// Run at most once per room.
    pub once_per_room: bool,
}

/// One macro's live state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MacroState {
    /// How far into the chain we are.
    pub chain: usize,
    /// The last time the macro ran (game seconds).
    pub when: Option<u32>,
    /// The room it last ran in, for `once_per_room`.
    pub room: Option<String>,
    /// Everything the macro has typed.
    pub typed: Vec<String>,
}

/// A whole `macros.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MacroFile {
    /// The macro sets, by name.
    pub sets: BTreeMap<String, Vec<MacroDef>>,
    /// The tools the macros can use.
    pub tools: BTreeMap<String, String>,
}

impl MacroFile {
    /// `@0x10193a1ec` — parse a `macros.toml` body.
    pub fn parse(src: &str) -> Result<MacroFile, String> {
        let doc = toml::parse(src).map_err(|e| e.to_string())?;
        let mut file = MacroFile::default();
        for (name, v) in &doc {
            match v {
                Value::Table(t) => {
                    if name == "tools" {
                        for (k, v) in t {
                            if let Some(s) = v.as_str() {
                                file.tools.insert(k.clone(), s.to_string());
                            }
                        }
                        continue;
                    }
                    file.sets
                        .insert(name.clone(), defs_from(&[], t));
                }
                Value::Tables(tables) => {
                    file.sets.insert(name.clone(), defs_from(tables, &Table::new()));
                }
                _ => {}
            }
        }
        Ok(file)
    }

    /// `@0x10193a140` — the definitions in one set.
    pub fn named(&self, set: &str) -> &[MacroDef] {
        self.sets.get(set).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// `@0x10193a314` — overlay `self` on top of `base`.
    pub fn merged(&self, base: &MacroFile) -> MacroFile {
        let mut sets = base.sets.clone();
        for (name, defs) in &self.sets {
            let entry = sets.entry(name.clone()).or_default();
            for def in defs {
                match entry.iter_mut().find(|d| d.label == def.label) {
                    Some(have) => *have = def.clone(),
                    None => entry.push(def.clone()),
                }
            }
        }
        MacroFile {
            sets,
            tools: {
                let mut tools = base.tools.clone();
                for (k, v) in &self.tools {
                    tools.insert(k.clone(), v.clone());
                }
                tools
            },
        }
    }

    /// `@0x10193a6a0` — write the file back.
    pub fn to_toml(&self) -> String {
        let mut doc = Table::new();
        for (set, defs) in &self.sets {
            let mut t = Table::new();
            let mut items = Vec::new();
            for def in defs {
                let mut d = Table::new();
                d.insert("label".into(), Value::Str(def.label.clone()));
                d.insert(
                    "chain".into(),
                    Value::Array(def.chain.iter().map(|c| Value::Str(c.clone())).collect()),
                );
                if let Some(k) = &def.key {
                    d.insert("key".into(), Value::Str(k.clone()));
                }
                if let Some(a) = &def.alias {
                    d.insert("alias".into(), Value::Str(a.clone()));
                }
                if def.once_per_room {
                    d.insert("once_per_room".into(), Value::Bool(true));
                }
                if let Some(r) = def.repeat_after {
                    d.insert("repeat_after".into(), Value::Float(r));
                }
                items.push(Value::Table(d));
            }
            t.insert("macros".into(), Value::Array(items));
            doc.insert(set.clone(), Value::Table(t));
        }
        toml::to_string(&doc)
    }

    /// `@0x101939e28` — read `macros.toml` from a folder.
    pub fn load(dir: &std::path::Path) -> Result<MacroFile, String> {
        let path = dir.join("macros.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        MacroFile::parse(&text)
    }
}

fn defs_from(tables: &[Table], table: &Table) -> Vec<MacroDef> {
    let mut out = Vec::new();
    let items: Vec<&Table> = if tables.is_empty() {
        table
            .get("macros")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_table()).collect())
            .unwrap_or_default()
    } else {
        tables.iter().collect()
    };
    for t in items {
        out.push(MacroDef {
            label: t.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            fg: t.get("fg").and_then(|v| v.as_str()).and_then(crate::theme::parse_hex),
            bg: t.get("bg").and_then(|v| v.as_str()).and_then(crate::theme::parse_hex),
            chain: t
                .get("chain")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            key: t.get("key").and_then(|v| v.as_str()).map(String::from),
            set: t.get("set").and_then(|v| v.as_str()).map(String::from),
            alias: t.get("alias").and_then(|v| v.as_str()).map(String::from),
            when: t.get("when").and_then(|v| v.as_str()).map(String::from),
            repeat_after: t.get("repeat_after").and_then(|v| v.as_f64()),
            once_per_room: t
                .get("once_per_room")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        });
    }
    out
}

impl MacroDef {
    /// `@0x101939d04` — the next command in the chain, advancing the state.
    pub fn chain_in(&self, state: &mut MacroState) -> Option<String> {
        if self.chain.is_empty() {
            return None;
        }
        let at = state.chain % self.chain.len();
        state.chain = at + 1;
        let command = self.chain[at].clone();
        state.typed.push(command.clone());
        Some(command)
    }
}

/// The typing state a macro set runs against.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Runs {
    /// The macros that are on cooldown, by label.
    pub unavailable: BTreeMap<String, String>,
}

impl Runs {
    /// `@0x101939ac8` — may this macro run now?
    ///
    /// `// UNKNOWN:` the recovered body consults the room, the game clock and
    /// the macro's `when` clause; the reproducible part is the `unavailable`
    /// set and `once_per_room`.
    pub fn allows(&self, def: &MacroDef, state: &MacroState, room: &str) -> bool {
        if self.unavailable.contains_key(&def.label) {
            return false;
        }
        if def.once_per_room && state.room.as_deref() == Some(room) {
            return false;
        }
        if let (Some(after), Some(when)) = (def.repeat_after, state.when) {
            let _ = after;
            let _ = when;
        }
        true
    }

    /// `@0x1019399a8` — record that a macro ran.
    pub fn note(&mut self, def: &MacroDef, state: &mut MacroState, room: &str, now: u32) {
        state.when = Some(now);
        state.room = Some(room.to_string());
        if let Some(after) = def.repeat_after {
            self.unavailable
                .insert(def.label.clone(), format!("{}s", after));
        }
    }
}

/// `@0x101939150` — the look of a macro button.
///
/// Returns `(label, foreground, background)` with the theme's button colours as
/// the fallback.
pub fn button_look(def: &MacroDef, palette: &crate::theme::Palette) -> (String, Rgb, Rgb) {
    (
        def.label.clone(),
        def.fg.unwrap_or_else(|| palette.get(crate::theme::Token::Text)),
        def.bg.unwrap_or_else(|| palette.get(crate::theme::Token::Raised)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
[travel]
macros = [
  { label = "N", chain = ["north"], key = "Numpad8" },
  { label = "Look", chain = ["look", "look in pack"], once_per_room = true },
]
"#;

    #[test]
    fn parses_sets_and_defs() {
        let f = MacroFile::parse(FILE).unwrap();
        let travel = f.named("travel");
        assert_eq!(travel.len(), 2);
        assert_eq!(travel[0].label, "N");
        assert_eq!(travel[0].chain, vec!["north".to_string()]);
        assert_eq!(travel[0].key.as_deref(), Some("Numpad8"));
        assert!(travel[1].once_per_room);
        assert_eq!(f.named("nope").len(), 0);
    }

    #[test]
    fn chains_cycle() {
        let def = MacroDef {
            label: "x".into(),
            chain: vec!["a".into(), "b".into()],
            ..MacroDef::default()
        };
        let mut state = MacroState::default();
        assert_eq!(def.chain_in(&mut state).as_deref(), Some("a"));
        assert_eq!(def.chain_in(&mut state).as_deref(), Some("b"));
        assert_eq!(def.chain_in(&mut state).as_deref(), Some("a"));
        assert_eq!(state.typed, vec!["a", "b", "a"]);
        assert_eq!(MacroDef::default().chain_in(&mut state), None);
    }

    #[test]
    fn once_per_room_and_unavailable() {
        let def = MacroDef {
            label: "Look".into(),
            once_per_room: true,
            ..MacroDef::default()
        };
        let mut runs = Runs::default();
        let mut state = MacroState::default();
        assert!(runs.allows(&def, &state, "room1"));
        runs.note(&def, &mut state, "room1", 10);
        assert!(!runs.allows(&def, &state, "room1"));
        assert!(runs.allows(&def, &state, "room2"));
        runs.unavailable.insert("Look".into(), "5s".into());
        assert!(!runs.allows(&def, &state, "room2"));
    }

    #[test]
    fn merged_overlays_by_label() {
        let base = MacroFile::parse(FILE).unwrap();
        let mut top = MacroFile::default();
        top.sets.insert(
            "travel".into(),
            vec![MacroDef {
                label: "N".into(),
                chain: vec!["south".into()],
                ..MacroDef::default()
            }],
        );
        let merged = top.merged(&base);
        let travel = merged.named("travel");
        assert_eq!(travel.len(), 2);
        assert_eq!(travel[0].chain, vec!["south".to_string()]);
    }

    #[test]
    fn button_look_falls_back_to_the_palette() {
        let p = crate::theme::Palette::bare();
        let (label, fg, bg) = button_look(
            &MacroDef {
                label: "N".into(),
                ..MacroDef::default()
            },
            &p,
        );
        assert_eq!(label, "N");
        assert_eq!(fg, p.get(crate::theme::Token::Text));
        assert_eq!(bg, p.get(crate::theme::Token::Raised));
    }
}
