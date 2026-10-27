//! The auto-caster's settings: options, aliases, verbs, stances and the hide
//! list.
//!
//! Everything here was read out of the arm64 disassembly of
//! `hydra_cast::settings` (addresses in `NOTES.md`); the map layout is
//! established from the field offsets the accessors touch:
//!
//! ```text
//! +0x00 aliases: BTreeMap<String, u16>        name -> spell number
//! +0x18 verbs:   BTreeMap<String, Chosen>     name -> verb
//! +0x30 stances: BTreeMap<String, String>     name -> stance
//! +0x48 hide:    BTreeSet<u16>                spells hidden after
//! +0x60 options: Options { channel, conserve, safety, stance, hide }
//! +0x65 on:      bool                         "typed spells are cast"
//! ```
//!
//! The serde field order (from the `visit_str` field visitor at 0x1019a95ac and
//! the `Serialize` impl at 0x101276278) is `on`, `options`, `aliases`, `verbs`,
//! `stances`, `hide`, and `Options` is `channel`, `conserve`, `safety`,
//! `stance`, `hide`.  `Chosen` is the strings `cast`, `channel` and `evoke`
//! (the enum deserialiser at 0x1019b49a8 rejects anything else through
//! `serde::de::Error::unknown_variant`).
//!
//! Phase 1: `serde` is a real dependency again and the three types carry
//! `Serialize`/`Deserialize` with exactly those member names.  `serde_json`
//! reads and writes them through [`CasterSettings::to_json`] /
//! [`CasterSettings::from_json`], the shape the session's
//! `.settings.json` sections hold.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::game::world_model;

/// The stances the game has, in the order the binary joins them for its
/// "`{}` is not a stance: {}." message (pointer table at 0x10449c640).
pub const STANCES: [&str; 6] = [
    "offensive",
    "advance",
    "forward",
    "neutral",
    "guarded",
    "defensive",
];

/// The verbs a cast can use, in the order of the word table the binary indexes
/// with a `Chosen` (0x10449c388 pointers / 0x1039c46d0 lengths).
pub const VERBS: [&str; 3] = ["cast", "channel", "evoke"];

/// How a spell is cast.  Serialised as the strings `cast`, `channel` and
/// `evoke` (serde enum at 0x1019b49a8 / 0x101286904).
///
/// The original is a one-byte enum whose niche value `3` encodes "no verb of
/// its own", which is why `Option<Chosen>` is one byte wide there.  Here it is
/// a plain enum and the absence is an `Option`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Chosen {
    /// `cast` — the default.
    #[default]
    Cast,
    /// `channel`.
    Channel,
    /// `evoke`.
    Evoke,
}

impl Chosen {
    /// Parse a verb name, as `Chosen::named` does at 0x1019b4ce8: the argument
    /// is lower-cased first, an empty name is `None`.
    pub fn named(name: &str) -> Option<Chosen> {
        match name.to_ascii_lowercase().as_str() {
            "cast" => Some(Chosen::Cast),
            "channel" => Some(Chosen::Channel),
            "evoke" => Some(Chosen::Evoke),
            _ => None,
        }
    }

    /// The word the game wants, from the recovered word table.
    pub fn word(self) -> &'static str {
        VERBS[self.as_index() as usize]
    }

    /// The serde / word-table index.
    pub fn as_index(self) -> u8 {
        match self {
            Chosen::Cast => 0,
            Chosen::Channel => 1,
            Chosen::Evoke => 2,
        }
    }
}

/// The five `on`/`off` options (`ec set <option> on|off`).
///
/// The serde member names are the recovered ones (`channel`, `conserve`,
/// `safety`, `stance`, `hide`, from the field visitor at 0x1019a948c).  A
/// missing member keeps its default; the recovered visitor's treatment of a
/// missing member is not in the binary (INFERRED, and the safer reading for a
/// settings file that gains options over time).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// "channel what can be channeled"
    pub channel: bool,
    /// "not without mana enough, or at a target not here"
    pub conserve: bool,
    /// "no attack spell with no live target here"
    pub safety: bool,
    /// "aimed spells from offensive, back to guarded after"
    pub stance: bool,
    /// "hide after the spells on the hide list"
    pub hide: bool,
}

impl Options {
    /// The option names in the order `set_option` tests them.
    pub const NAMES: [&'static str; 5] = ["channel", "conserve", "safety", "stance", "hide"];

    /// The value of one option by name (already lower-cased).
    fn slot(&mut self, name: &str) -> Option<&mut bool> {
        match name {
            "channel" => Some(&mut self.channel),
            "conserve" => Some(&mut self.conserve),
            "safety" => Some(&mut self.safety),
            "stance" => Some(&mut self.stance),
            "hide" => Some(&mut self.hide),
            _ => None,
        }
    }

    /// The value of one option by name, read only.
    pub fn get(&self, name: &str) -> Option<bool> {
        match name.to_ascii_lowercase().as_str() {
            "channel" => Some(self.channel),
            "conserve" => Some(self.conserve),
            "safety" => Some(self.safety),
            "stance" => Some(self.stance),
            "hide" => Some(self.hide),
            _ => None,
        }
    }
}

/// The whole caster section of a character's settings.
///
/// Serialised with the recovered member names and order (`on`, `options`,
/// `aliases`, `verbs`, `stances`, `hide` — the field visitor at 0x1019a95ac).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CasterSettings {
    /// "Typed spells are cast." when set, "Typed spells are sent as typed."
    /// when clear (`switch`, 0x1019b4598).
    pub on: bool,
    /// The five options.
    pub options: Options,
    /// Alias name (lower-cased) to spell number.
    pub aliases: BTreeMap<String, u16>,
    /// Spell or alias name to verb.
    pub verbs: BTreeMap<String, Chosen>,
    /// Spell or alias name to stance.
    pub stances: BTreeMap<String, String>,
    /// Spells to hide behind after casting.
    pub hide: BTreeSet<u16>,
}

/// Parse a spell number.
///
/// Recovered from `hydra_cast::settings::spell` (0x1019b4b14): the text is
/// trimmed, parsed as `u16` (a leading `+` is allowed, `-`, empty texts and
/// overflow are not) and then looked up in the game's spell table.  Anything
/// that fails either step is "`{}` is not a spell number." with the *untrimmed*
/// argument, exactly as the binary formats it.
pub fn spell(text: &str) -> Result<u16, String> {
    let trimmed = text.trim();
    let number: u16 = match trimmed.parse() {
        Ok(number) => number,
        Err(_) => return Err(format!("`{}` is not a spell number.", text)),
    };
    // ESTABLISHED: 0x1019b4ca0 `hydra_model::spells::spell(number)` then a
    // null test.  With the real model installed the spell table answers; the
    // table is empty until `hydra_model::spells::set_spells` fills it, and an
    // empty table is not a spell either.
    let known = world_model().spell_name(number).is_some();
    if !known {
        return Err(format!("`{}` is not a spell number.", text));
    }
    Ok(number)
}

fn on_off(value: bool) -> &'static str {
    // 0x1039c49b8 "on", 0x1039c49ba "off".
    if value {
        "on"
    } else {
        "off"
    }
}

/// "none", or the values joined with ", " — the recovered `join` helper
/// (closure at 0x1019b4270 plus `join_generic_copy` with the separator at
/// 0x1039c49f6).
fn listed_or_none(values: Vec<String>) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

impl CasterSettings {
    /// `ec set <option> on|off`.
    ///
    /// Recovered from 0x1019b2690.  Both arguments are lower-cased with ASCII
    /// rules and are *not* trimmed.  The value must be `on` or `off`; the
    /// message names the lower-cased option and the state it now has.
    pub fn set_option(&mut self, option: &str, value: &str) -> Result<String, String> {
        let on = match value.to_ascii_lowercase().as_str() {
            "on" => true,
            "off" => false,
            // 0x103e275df "`{}` is not on or off." — the raw value.
            _ => return Err(format!("`{}` is not on or off.", value)),
        };
        let name = option.to_ascii_lowercase();
        match self.options.slot(&name) {
            Some(slot) => *slot = on,
            // 0x103e7b5c9 "`{}` is not an option: channel, conserve, safety, stance or hide."
            None => {
                return Err(format!(
                    "`{}` is not an option: channel, conserve, safety, stance or hide.",
                    name
                ))
            }
        }
        // 0x103e62c3e "{} is {}."
        Ok(format!("{} is {}.", name, on_off(on)))
    }

    /// `ec stance <spell|alias> <stance|clear>`.
    ///
    /// Recovered from 0x1019b2c40.  `clear` (5 characters, ASCII
    /// case-insensitive) drops the entry; otherwise the stance must be one of
    /// [`STANCES`].
    pub fn set_stance(&mut self, spell_text: &str, stance: &str) -> Result<String, String> {
        let key = self.which(spell_text)?;
        let name = stance.to_ascii_lowercase();
        if name == "clear" {
            return match self.stances.remove(&key) {
                // 0x103e7b60c "{} had no stance of its own."
                None => Ok(format!("{} had no stance of its own.", key)),
                // 0x103e7b629 "{} has no stance of its own."
                Some(_) => Ok(format!("{} has no stance of its own.", key)),
            };
        }
        if !STANCES.contains(&name.as_str()) {
            // 0x103e7b646 "`{}` is not a stance: {}." with the joined list.
            return Err(format!(
                "`{}` is not a stance: {}.",
                name,
                STANCES.join(", ")
            ));
        }
        match self.stances.insert(key.clone(), name.clone()) {
            // 0x103e7b661 "{} was cast from {}; it is now cast from {}."
            Some(old) if old != name => Ok(format!(
                "{} was cast from {}; it is now cast from {}.",
                key, old, name
            )),
            // 0x103e7b68e "{} is cast from {}."
            _ => Ok(format!("{} is cast from {}.", key, name)),
        }
    }

    /// `ec verb <spell|alias> <cast|channel|evoke|clear>`.
    ///
    /// Recovered from 0x1019b4640.
    pub fn set_verb(&mut self, spell_text: &str, verb: &str) -> Result<String, String> {
        let key = self.which(spell_text)?;
        let name = verb.to_ascii_lowercase();
        if name == "clear" {
            return match self.verbs.remove(&key) {
                // 0x103e7b7cb "{} had no verb of its own."
                None => Ok(format!("{} had no verb of its own.", key)),
                // 0x103e7b7e6 "{} is cast as it would be."
                Some(_) => Ok(format!("{} is cast as it would be.", key)),
            };
        }
        let chosen = match Chosen::named(&name) {
            Some(chosen) => chosen,
            // 0x103e7b838 "`{}` is not a verb: cast, channel or evoke."
            None => return Err(format!("`{}` is not a verb: cast, channel or evoke.", verb)),
        };
        match self.verbs.insert(key.clone(), chosen) {
            // 0x103e7b801 "{} was cast with {}; it is now {}."
            Some(old) if old != chosen => Ok(format!(
                "{} was cast with {}; it is now {}.",
                key,
                old.word(),
                chosen.word()
            )),
            // 0x103e7b824 "{} is cast with {}."
            _ => Ok(format!("{} is cast with {}.", key, chosen.word())),
        }
    }

    /// `ec alias <spell> <name>`.
    ///
    /// Recovered from 0x1019b3720: the name is trimmed and lower-cased, must be
    /// one word of letters, digits or `_`, and must not be all digits.
    pub fn alias(&mut self, spell_text: &str, name: &str) -> Result<String, String> {
        let number = spell(spell_text)?;
        let name = name.trim().to_lowercase();
        // 0x1039c4a21 (43 bytes) — the fixed message.
        let one_word =
            !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
        if !one_word {
            return Err("An alias is one word of letters and digits.".to_string());
        }
        // 0x1039c4a4c (48 bytes) — all ASCII digits is a spell number.
        if name.chars().all(|c| c.is_ascii_digit()) {
            return Err("An alias is not a number: that is a spell's own.".to_string());
        }
        match self.aliases.insert(name.clone(), number) {
            // 0x103e7b708 "\nThe alias {} was spell {}; it is now {}."
            Some(old) if old != number => Ok(format!(
                "\nThe alias {} was spell {}; it is now {}.",
                name, old, number
            )),
            // 0x103e7b732 "\nThe alias {} casts spell {}."
            _ => Ok(format!("\nThe alias {} casts spell {}.", name, number)),
        }
    }

    /// `ec alias clear <name>`.
    ///
    /// Recovered from 0x1019b3308.  A missing alias is an error.
    pub fn clear_alias(&mut self, name: &str) -> Result<String, String> {
        let key = name.trim().to_lowercase();
        match self.aliases.remove(&key) {
            // 0x103e7b6b9 "The alias {} is taken away."
            Some(_) => Ok(format!("The alias {} is taken away.", key)),
            // 0x103e7b6a2 "There is no alias {}."
            None => Err(format!("There is no alias {}.", key)),
        }
    }

    /// `ec hide <spell>` — the same command takes the spell off again.
    ///
    /// Recovered from 0x1019b33fc.  Adding a spell while the `hide` option is
    /// off appends the recovered hint " The hide option is off: .ec set hide
    /// on." so the user knows nothing will be hidden.
    pub fn toggle_hide(&mut self, spell_text: &str) -> Result<String, String> {
        let number = spell(spell_text)?;
        if self.hide.remove(&number) {
            // 0x103e7b6d6 "{} is off the hide list."
            return Ok(format!("{} is off the hide list.", number));
        }
        self.hide.insert(number);
        let tail = if self.options.hide {
            ""
        } else {
            // 0x1039c49f8 (41 bytes) — the leading space is in the literal.
            " The hide option is off: .ec set hide on."
        };
        // 0x103e7b6ef "{} is on the hide list.{}"
        Ok(format!("{} is on the hide list.{}", number, tail))
    }

    /// `ec on|off` — whether typed spell numbers are cast or just sent.
    ///
    /// Recovered from 0x1019b4598: the flag is stored and then named.
    pub fn switch(&mut self, on: bool) -> String {
        self.on = on;
        if on {
            // 0x1039c4aa4 (22 bytes)
            "Typed spells are cast.".to_string()
        } else {
            // 0x1039c4a80 (31 bytes)
            "Typed spells are sent as typed.".to_string()
        }
    }

    /// Look a spell's verb up: by name first, then by the spell number written
    /// out in decimal.
    ///
    /// Recovered from 0x1019b350c.  `None` means "no verb of its own" (the
    /// niche value 3 in the binary's one-byte enum).
    pub fn verb(&self, number: u16, name: Option<&str>) -> Option<Chosen> {
        if let Some(name) = name {
            if let Some(chosen) = self.verbs.get(name) {
                return Some(*chosen);
            }
        }
        self.verbs.get(&number.to_string()).copied()
    }

    /// Look a spell's stance up, the same way (0x1019b4384).
    pub fn stance(&self, number: u16, name: Option<&str>) -> Option<&str> {
        if let Some(name) = name {
            if let Some(stance) = self.stances.get(name) {
                return Some(stance.as_str());
            }
        }
        self.stances.get(&number.to_string()).map(|s| s.as_str())
    }

    /// The six lines `ec status` prints.
    ///
    /// Recovered from 0x1019b3d14: a `Vec<String>` of
    /// `Typed spells cast: …`, `Options: …`, `Aliases: …`, `Verbs: …`,
    /// `Stances: …` and `Hide after: …`; every list is `", "`-joined, or
    /// `none` when empty.
    pub fn listed(&self) -> Vec<String> {
        let options = &self.options;
        vec![
            // 0x103e7b750
            format!("Typed spells cast: {}", on_off(self.on)),
            // 0x103e7b766
            format!(
                "Options: channel {}, conserve {}, safety {}, stance {}, hide {}",
                on_off(options.channel),
                on_off(options.conserve),
                on_off(options.safety),
                on_off(options.stance),
                on_off(options.hide)
            ),
            // 0x103e6164b, entries "{} {}"
            format!(
                "Aliases: {}",
                listed_or_none(
                    self.aliases
                        .iter()
                        .map(|(name, number)| format!("{} {}", name, number))
                        .collect()
                )
            ),
            // 0x103e7b7a6
            format!(
                "Verbs: {}",
                listed_or_none(
                    self.verbs
                        .iter()
                        .map(|(name, chosen)| format!("{} {}", name, chosen.word()))
                        .collect()
                )
            ),
            // 0x103e7b7b0
            format!(
                "Stances: {}",
                listed_or_none(
                    self.stances
                        .iter()
                        .map(|(name, stance)| format!("{} {}", name, stance))
                        .collect()
                )
            ),
            // 0x103e7b7bc — the hide list is numbers on their own.
            format!(
                "Hide after: {}",
                listed_or_none(self.hide.iter().map(|n| n.to_string()).collect())
            ),
        ]
    }

    /// Resolve a `<spell|alias>` argument to the key the maps use.
    ///
    /// Recovered from `CasterSettings::which` (0x1019b3b24): trim, lower-case,
    /// and if the result is an alias return it as it is; otherwise parse it as
    /// a spell number and return that number written out.
    fn which(&self, spell_text: &str) -> Result<String, String> {
        let key = spell_text.trim().to_lowercase();
        if self.aliases.contains_key(&key) {
            return Ok(key);
        }
        Ok(spell(&key)?.to_string())
    }

    /// The settings as JSON, in the recovered member shape.
    ///
    /// Not a recovered function (the binary's writer is in the session's
    /// settings file, not here): this is the `serde_json` side of the recorded
    /// `Serialize`/`Deserialize` impls, so the shape in §2.2 of `NOTES.md` has
    /// a reader and a writer again.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Read settings back out of JSON.  Absent members keep their defaults.
    pub fn from_json(text: &str) -> Result<CasterSettings, serde_json::Error> {
        serde_json::from_str(text)
    }
}
