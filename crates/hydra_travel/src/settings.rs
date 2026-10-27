//! The travel settings and the schema behind them.
//!
//! Every setting, its title, its description, its default and its option list
//! were read out of the merged `__cstring` run at `0x10241c575`..`0x10241d520`
//! (one string right after the other, NUL-separated) plus the message piece
//! tables at `0x103e6c117`..`0x103e6c1d0` and `0x103e63082`.
//!
//! `TravelSettings` itself is a thin wrapper over the walker's stored
//! `BTreeMap<String, String>`: `TravelSettings::for_walker` (`0x101528ef8`)
//! builds one with `shown` + `on_word`, and `set` (`0x1015293b8`) validates a
//! word against the schema and writes it back through `&mut BTreeMap::insert`.

use std::collections::BTreeMap;

use crate::json;

/// One option of a choice setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    /// The word that is stored. An empty `value` is the "blank" / auto option.
    pub value: &'static str,
    /// The label the settings page shows.
    pub label: &'static str,
}

const fn choice(value: &'static str, label: &'static str) -> Choice {
    Choice { value, label }
}

/// What a setting accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `on` / `off` / `yes` / `no` / `true` / `false`.
    Switch,
    /// A whole number in `min..=max`.
    Whole { min: u32, max: u32 },
    /// A number of seconds in `min..=max`.
    Seconds { min: u32, max: u32 },
    /// One of the listed words, or blank when `blank` is set.
    OneOf {
        options: &'static [Choice],
        blank: bool,
    },
    /// A room number.
    Room,
    /// Free text (a container, an item, a password).
    Text,
}

impl Kind {
    /// The message fragment for a rejected word, from the piece tables.
    pub fn refuse(&self, key: &str, value: &str) -> Option<String> {
        match self {
            Kind::Switch => Some(format!("{key} is on or off.")),
            Kind::Whole { min, max } => Some(format!(
                "{key} is a whole number, {} to {}.",
                min,
                max.max(min)
            )),
            Kind::Seconds { min, max } => Some(format!(
                "{key} is a number of seconds, {} to {}.",
                min,
                max.max(min)
            )),
            Kind::OneOf { options, blank } => {
                if *blank {
                    let mut words: Vec<&str> = options
                        .iter()
                        .map(|o| o.value)
                        .filter(|v| !v.is_empty())
                        .collect();
                    words.sort_unstable();
                    Some(format!("{key} is one of {}, or blank.", words.join(", ")))
                } else {
                    let mut words: Vec<&str> = options.iter().map(|o| o.value).collect();
                    words.sort_unstable();
                    Some(format!("{key} is one of {}.", words.join(", ")))
                }
            }
            Kind::Room => Some(format!("{key} is a room number, or blank.")),
            // The recovered pieces are `{} is {}` (`0x103e62c3e`), used by
            // `TravelSettings::set` as the confirmation for a plain value.
            Kind::Text => Some(format!("{key} is {value}.")),
        }
    }
}

/// One setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setting {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// The value a fresh character starts with.
    pub default: &'static str,
    pub kind: Kind,
}

/// The `ice` modes, in pool order (`0x10241c885`..`0x10241c8b4`).
pub const ICE: &[Choice] = &[
    choice("", "Auto: wait on ice when likely to slip"),
    choice("run", "Run: never wait"),
    choice("wait", "Wait: always wait"),
];

/// Which side of the Hinterwilds the walk entered from (`0x10241d18f`).
pub const HINTERWILDS: &[Choice] = &[
    choice("", ""),
    choice("Icemule Trace", "Icemule Trace"),
    choice("Elven Nations", "Elven Nations"),
];

/// Which side of the Red Forest the walk entered from (`0x10241d1a9`).
pub const RED_FOREST: &[Choice] = &[choice("", ""), choice("WL", "Wehnimer's Landing")];

/// Which town Duskruin was entered from (`0x10241d2c0`..`0x10241d305`).
///
/// The pool stores the room number followed by the town's label; only two of
/// the pairs kept their digits (`284` -> *Wehnimer's Landing, Town Square*,
/// `10862` -> *Wehnimer's Landing, Wayside Inn*, `28908` -> *Kraken's Fall*,
/// `7` -> *Ta'Illistim*).
pub const DUSKRUIN: &[Choice] = &[
    choice("", ""),
    choice("284", "Wehnimer's Landing, Town Square"),
    choice("10862", "Wehnimer's Landing, Wayside Inn"),
    choice("River's Rest", "River's Rest"),
    choice("Kharam-Dzu", "Kharam-Dzu"),
    choice("28908", "Kraken's Fall"),
    choice("Mist Harbor", "Mist Harbor"),
    choice("7", "Ta'Illistim"),
    choice("Zul Logoth", "Zul Logoth"),
];

/// Which town Ebon Gate / Talondown / the Marks of the Beast were entered from.
pub const TOWN_ORIGIN: &[Choice] = &[
    choice("", ""),
    choice("28802", "Talondown Arena"),
    choice("Ravelin", "Ravelin"),
    choice("682", "Sylvarraend"),
    choice("the Frozen Garden", "the Frozen Garden"),
];

/// **ESTABLISHED** — every key, title and description below is a literal from
/// the merged run at `0x10241c575`.. (`hydra_travel::settings`).
pub const SETTINGS: &[Setting] = &[
    Setting {
        key: "use_urchins",
        title: "Use the urchin guides",
        description:
            "Let a route use the urchin guides, while the game says they are paid for.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "use_portmasters",
        title: "Use the portmasters",
        description: "Let a route sail with the portmasters.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "use_own_ship",
        title: "Use your own ship",
        description: "Let a walk with no way by land sail your own ship there. A dock handler's fee is paid when the ship is not out.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "call_wind",
        title: "Call wind into the sail",
        description: "Sailing your own ship, cast Breeze (612) into the sail now and then, when this character knows it.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "use_seeking",
        title: "Use Voln's symbol of seeking",
        description: "Let a route use the Order of Voln's symbol of seeking.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "ice",
        title: "Ice",
        description: "How icy paths are crossed: run, wait for footing, or wait only when likely to slip (heavily loaded, or untrained in survival without Haste).",
        default: "",
        kind: Kind::OneOf {
            options: ICE,
            blank: true,
        },
    },
    Setting {
        key: "vaalor_shortcut",
        title: "Vaalor shortcut",
        description: "Let a route take the rocky trail into Ta'Vaalor.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "car_to_sos",
        title: "Caravan to Sanctum of Scales",
        description: "Let a route ride the caravan to the Sanctum of Scales.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "car_from_sos",
        title: "Caravan from Sanctum of Scales",
        description: "Let a route ride the caravan back from the Sanctum of Scales.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "use_day_pass",
        title: "Use day passes",
        description: "Travel between towns on a Chronomage day pass from the day pass sack.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "day_pass_sack",
        title: "Day pass sack",
        description: "The container day passes are kept in.",
        default: "",
        kind: Kind::Text,
    },
    Setting {
        key: "buy_day_pass",
        title: "Buy day passes",
        description: "yes to buy a pass when none held serves; or the routes to buy for, such as imt,wl.",
        default: "off",
        kind: Kind::Text,
    },
    Setting {
        key: "fwi_trinket",
        title: "Mist Harbor trinket",
        description: "The trinket that takes this character to Mist Harbor, by its name.",
        default: "",
        kind: Kind::Text,
    },
    Setting {
        key: "rogue_password",
        title: "Rogue guild password",
        description: "The rogue guild's password, for its doors.",
        default: "",
        kind: Kind::Text,
    },
    Setting {
        key: "key",
        title: "House key",
        description: "The key a private property's door takes, by its name.",
        default: "",
        kind: Kind::Text,
    },
    Setting {
        key: "keysack",
        title: "House key sack",
        description: "The container that key is kept in.",
        default: "",
        kind: Kind::Text,
    },
    Setting {
        key: "allow_vornavis",
        title: "Allow Vornavis",
        description: "Let a route pass through Vornavis.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "hate_familiar",
        title: "Send the familiar home",
        description: "After the familiar has opened the Vaalorn ring doors, send it home to stay.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "mularos_lover",
        title: "Mularos' Lover",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "peregrine",
        title: "Peregrine",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "journeys_end",
        title: "Journey's End",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "safe_harbor",
        title: "Safe Harbor",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "spindrift",
        title: "Spindrift",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "sunset_cabin",
        title: "Sunset Cabin",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "black_swan",
        title: "Black Swan",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "cuddfan_hollow",
        title: "Cuddfan Hollow",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "oleander_house",
        title: "Oleander House",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "shivergale",
        title: "Shivergale",
        description: "A private property the map can route through: set it only if this character may go in.",
        default: "",
        kind: Kind::Room,
    },
    Setting {
        key: "get_silvers",
        title: "Fetch silver from the bank",
        description: "Short of what a route charges, walk to a bank first. Off, go anyway.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "get_return_trip_silvers",
        title: "Fetch silver for the way back",
        description: "Count the return trip in the silver fetched.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "use_gigas_hwtravel",
        title: "Use gigas fragments to the Hinterwilds",
        description: "Cross to and from the Hinterwilds by gigas fragments.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        // The pool stores the title only ("Gigas fragments kept back"); the key
        // is the snake_case of the description's subject.
        key: "gigas_min_number",
        title: "Gigas fragments kept back",
        description: "Take the fragment crossing only with at least this many.",
        default: "4",
        kind: Kind::Whole { min: 0, max: 999 },
    },
    Setting {
        key: "typeahead",
        title: "How many moves the game holds beyond the one it is doing: plain moves go 1 + this at a time. 0 for one move at a time.",
        description: "How many moves the game holds beyond the one it is doing: plain moves go 1 + this at a time. 0 for one move at a time.",
        default: "2",
        kind: Kind::Whole { min: 0, max: 9 },
    },
    Setting {
        key: "delay",
        title: "Pause in each room",
        description: "Seconds to wait in every room walked into.",
        default: "0",
        kind: Kind::Seconds { min: 0, max: 60 },
    },
    Setting {
        key: "stop_for_dead",
        title: "Stop beside the dead",
        description: "Halt the walk beside a dead character, rather than walking on.",
        default: "off",
        kind: Kind::Switch,
    },
    Setting {
        key: "hinterwilds_location",
        title: "Hinterwilds side",
        description: "Which side the Hinterwilds was entered from: the caravan home goes back there.",
        default: "",
        kind: Kind::OneOf {
            options: HINTERWILDS,
            blank: true,
        },
    },
    Setting {
        key: "redforest_location",
        title: "Red Forest side",
        description: "Which side the Red Forest was entered from, by the symbol of seeking.",
        default: "",
        kind: Kind::OneOf {
            options: RED_FOREST,
            blank: true,
        },
    },
    Setting {
        key: "duskruin_origin",
        title: "Duskruin entered from",
        description: "The town Duskruin was entered from: the way back lands there.",
        default: "",
        kind: Kind::OneOf {
            options: DUSKRUIN,
            blank: true,
        },
    },
    Setting {
        key: "ebon_gate_origin",
        title: "Ebon Gate entered from",
        description: "The town Ebon Gate was entered from: the way back lands there.",
        default: "",
        kind: Kind::OneOf {
            options: TOWN_ORIGIN,
            blank: true,
        },
    },
    Setting {
        key: "talondown_origin",
        title: "Talondown entered from",
        description: "The town Talondown Arena was entered from: the way back lands there.",
        default: "",
        kind: Kind::OneOf {
            options: TOWN_ORIGIN,
            blank: true,
        },
    },
    Setting {
        key: "marksofthebeast_origin",
        title: "Marks of the Beast entered from",
        description: "Where the Marks of the Beast quest was entered from.",
        default: "",
        kind: Kind::OneOf {
            options: TOWN_ORIGIN,
            blank: true,
        },
    },
    Setting {
        key: "fwi_return_room",
        title: "Mist Harbor return room",
        description: "The room the trinket was turned in, by its number: the trinket goes back there.",
        default: "",
        kind: Kind::Room,
    },
];

/// The setting registered under `name`, if any (`hydra_travel::settings::key`,
/// `0x10152a3c0` — a table walk comparing the key).
pub fn key(name: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|setting| setting.key == name)
}

/// The value of one setting as a boolean.
///
/// `TravelSettings::on` (`0x1015292cc`) goes through `on_word`; a missing
/// setting yields its default, and a default that is not a switch yields
/// `false`.
pub fn on(name: &str) -> bool {
    key(name).map(|s| s.default == "on" || s.default == "yes").unwrap_or(false)
}

/// Parse an on/off word (`0x10152b780`, `settings::on_word`).
///
/// **ESTABLISHED** (needles recovered from the body): `on`/`yes`/`true` are
/// true and `off`/`no`/`false` are false; anything else is `None`. The rejection
/// message a caller reports is `"{key} is on or off."`
/// (pieces at `0x103e63082`).
pub fn on_word(word: &str) -> Option<bool> {
    match word.trim().to_ascii_lowercase().as_str() {
        "on" | "yes" | "true" => Some(true),
        "off" | "no" | "false" => Some(false),
        _ => None,
    }
}

/// Validate one word against a setting's [`Kind`].
///
/// **ESTABLISHED** message fragments (all from `__cstring`): `{key} is on or
/// off.`, `{key} is a whole number, {a} to {b}.`, `{key} is a number of
/// seconds, {a} to {b}.`, `{key} is one of {list}.`, `{key} is one of {list},
/// or blank.`, `{key} is a room number…`. `Travel has no setting {key}.` covers
/// an unknown key (pieces at `0x103e6c18a`).
pub fn check(name: &str, word: &str) -> Result<String, String> {
    let Some(setting) = key(name) else {
        return Err(format!("Travel has no setting {name}."));
    };
    let word = word.trim();
    match setting.kind {
        Kind::Switch => match on_word(word) {
            Some(true) => Ok("on".to_string()),
            Some(false) => Ok("off".to_string()),
            None => Err(setting.kind.refuse(name, word).unwrap()),
        },
        Kind::Whole { min, max } => match word.parse::<u32>() {
            Ok(n) if n >= min && n <= max => Ok(n.to_string()),
            _ => Err(setting.kind.refuse(name, word).unwrap()),
        },
        Kind::Seconds { min, max } => match word.parse::<u32>() {
            Ok(n) if n >= min && n <= max => Ok(n.to_string()),
            _ => Err(setting.kind.refuse(name, word).unwrap()),
        },
        Kind::OneOf { options, blank } => {
            if word.is_empty() && blank {
                return Ok(String::new());
            }
            options
                .iter()
                .find(|o| !o.value.is_empty() && o.value.eq_ignore_ascii_case(word))
                .map(|o| o.value.to_string())
                .ok_or_else(|| setting.kind.refuse(name, word).unwrap())
        }
        Kind::Room => {
            if word.is_empty() {
                return Ok(String::new());
            }
            match word.parse::<u32>() {
                Ok(n) => Ok(n.to_string()),
                Err(_) => Err(setting.kind.refuse(name, word).unwrap()),
            }
        }
        Kind::Text => Ok(word.to_string()),
    }
}

/// One character's travel settings: the words stored for each key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TravelSettings {
    values: BTreeMap<String, String>,
}

/// One row of the settings page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub value: String,
}

impl TravelSettings {
    pub fn new() -> TravelSettings {
        TravelSettings::default()
    }

    /// Build from a `Spot`'s `memories` map.
    ///
    /// `TravelSettings::for_walker` (`0x101528ef8`) calls `shown` and
    /// `on_word`, walks the schema and clones the stored words into the
    /// settings map (`String::clone` + `HashMap::insert` at `0x101529154`).
    pub fn from_memories(memories: &BTreeMap<String, json::Value>) -> TravelSettings {
        let mut values = BTreeMap::new();
        for setting in SETTINGS {
            if let Some(value) = memories.get(setting.key) {
                let word = match value {
                    json::Value::String(s) => s.clone(),
                    json::Value::Bool(b) => (if *b { "on" } else { "off" }).to_string(),
                    json::Value::Number(n) => n.to_string(),
                    _ => continue,
                };
                values.insert(setting.key.to_string(), word);
            }
        }
        TravelSettings { values }
    }

    /// The words, for writing back into `memories`.
    ///
    /// **Phase 1 reconnection:** the values are `serde_json::Value`s now, so the
    /// string arm is `Value::from(String)`.
    pub fn memories(&self) -> BTreeMap<String, json::Value> {
        self.values
            .iter()
            .map(|(k, v)| (k.clone(), json::Value::from(v.clone())))
            .collect()
    }

    /// Build the settings for a walker from its stored words.
    pub fn for_walker(stored: &BTreeMap<String, String>) -> TravelSettings {
        let mut values = BTreeMap::new();
        for setting in SETTINGS {
            let value = stored
                .get(setting.key)
                .cloned()
                .unwrap_or_else(|| setting.default.to_string());
            values.insert(setting.key.to_string(), value);
        }
        TravelSettings { values }
    }

    /// The stored word for `name`, or its default.
    pub fn get(&self, name: &str) -> Option<String> {
        if let Some(value) = self.values.get(name) {
            return Some(value.clone());
        }
        key(name).map(|s| s.default.to_string())
    }

    /// Is the switch `name` on? (`0x1015292cc`)
    pub fn on(&self, name: &str) -> bool {
        matches!(
            self.get(name).as_deref().map(on_word),
            Some(Some(true))
        )
    }

    /// A number setting, or its default.
    pub fn number(&self, name: &str) -> u32 {
        self.get(name)
            .and_then(|word| word.parse::<u32>().ok())
            .unwrap_or(0)
    }

    /// Set one setting (`0x1015293b8`).
    ///
    /// The body looks the key up in the schema, validates the word, removes the
    /// empty value (`BTreeMap::remove` at `0x101529594`) or inserts the new one
    /// (`insert` at `0x101529b30`), and answers through [`TravelSettings::said`].
    pub fn set(&mut self, name: &str, word: &str) -> Result<String, String> {
        let value = check(name, word)?;
        if value.is_empty() {
            self.values.remove(name);
        } else {
            self.values.insert(name.to_string(), value.clone());
        }
        Ok(self.said(name))
    }

    /// The confirmation a change prints.
    ///
    /// **ESTABLISHED** message shapes: `{key} is back to {default}.`,
    /// `{key} is off.`, and `Not saved: {e}` / `Nothing was changed: {e}` for
    /// the failure paths (pieces at `0x103e6c117`, `0x103e6c129`, `0x103e62c4d`,
    /// `0x103e62c56`).
    pub fn said(&self, name: &str) -> String {
        let value = self.get(name).unwrap_or_default();
        match on_word(&value) {
            Some(true) => format!("{name} is on."),
            Some(false) => format!("{name} is off."),
            None if value.is_empty() => {
                let default = key(name).map(|s| s.default).unwrap_or("");
                format!("{name} is back to {default}.")
            }
            None => format!("{name} is {value}."),
        }
    }

    /// Every setting with its current value (`0x10152a0e8`).
    pub fn shown(&self) -> Vec<Shown> {
        SETTINGS
            .iter()
            .map(|setting| Shown {
                key: setting.key,
                title: setting.title,
                description: setting.description,
                value: self
                    .values
                    .get(setting.key)
                    .cloned()
                    .unwrap_or_else(|| setting.default.to_string()),
            })
            .collect()
    }
}

/// The string a memory is stored under.
///
/// `settings::memory` (`0x10152aaf8`) reads the stored memories and formats the
/// message that names them; `Memory::called` (`0x10152aa18`) returns the name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub name: String,
    pub value: String,
}

impl Memory {
    /// Accept a stored word, remembering it (`0x10152a4b0`).
    ///
    /// The body trims the word, looks the memory's key up in the schema and
    /// reports `"No memory {key}."` on a miss (pieces at `0x103e63126`); a hit
    /// keeps the word.
    pub fn accept(&mut self, word: &str) -> Result<(), String> {
        let word = word.trim();
        if self.name.is_empty() {
            return Err("No memory .".to_string());
        }
        if key(&self.name).is_none() {
            return Err(format!("No memory {}.", self.name));
        }
        self.value = word.to_string();
        Ok(())
    }

    /// The memory's name.
    pub fn called(&self) -> &str {
        &self.name
    }
}

/// Read the stored memories into settings (`0x10152aaf8`).
pub fn memory(settings: &TravelSettings) -> Vec<Memory> {
    settings
        .values
        .iter()
        .map(|(name, value)| Memory {
            name: name.clone(),
            value: value.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_keeps_the_recovered_literals() {
        assert_eq!(key("use_urchins").unwrap().title, "Use the urchin guides");
        assert_eq!(
            key("use_urchins").unwrap().description,
            "Let a route use the urchin guides, while the game says they are paid for."
        );
        assert_eq!(key("typeahead").unwrap().default, "2");
        assert_eq!(key("gigas_min_number").unwrap().default, "4");
        assert_eq!(key("delay").unwrap().default, "0");
        assert_eq!(key("nope"), None);
        // The switch set includes every recovered `*_*` toggle name.
        for name in [
            "use_urchins",
            "use_portmasters",
            "use_own_ship",
            "call_wind",
            "use_seeking",
            "vaalor_shortcut",
            "car_to_sos",
            "car_from_sos",
            "use_day_pass",
            "allow_vornavis",
            "hate_familiar",
            "get_silvers",
            "get_return_trip_silvers",
            "use_gigas_hwtravel",
            "stop_for_dead",
        ] {
            assert_eq!(key(name).unwrap().kind, Kind::Switch, "{name}");
        }
    }

    #[test]
    fn ice_choices_are_the_recovered_three() {
        let ice = key("ice").unwrap();
        match ice.kind {
            Kind::OneOf { options, blank } => {
                assert!(blank);
                assert_eq!(options.len(), 3);
                assert_eq!(options[1].value, "run");
                assert_eq!(options[1].label, "Run: never wait");
                assert_eq!(options[2].value, "wait");
                assert_eq!(options[2].label, "Wait: always wait");
                assert_eq!(options[0].label, "Auto: wait on ice when likely to slip");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn set_validates_and_answers() {
        let mut settings = TravelSettings::new();
        assert_eq!(settings.set("use_urchins", "yes").unwrap(), "use_urchins is on.");
        assert!(settings.on("use_urchins"));
        assert_eq!(settings.set("use_urchins", "off").unwrap(), "use_urchins is off.");
        assert!(!settings.on("use_urchins"));
        assert_eq!(
            settings.set("use_urchins", "maybe").unwrap_err(),
            "use_urchins is on or off."
        );
        assert_eq!(
            settings.set("nope", "on").unwrap_err(),
            "Travel has no setting nope."
        );
        assert_eq!(settings.set("delay", "5").unwrap(), "delay is 5.");
        assert_eq!(
            settings.set("delay", "900").unwrap_err(),
            "delay is a number of seconds, 0 to 60."
        );
        assert_eq!(
            settings.set("typeahead", "x").unwrap_err(),
            "typeahead is a whole number, 0 to 9."
        );
        assert_eq!(
            settings.set("ice", "sprint").unwrap_err(),
            "ice is one of run, wait, or blank."
        );
        // `{} is a room number, or blank.` (0x103e6c1b1..0x103e6c1d4).
        assert_eq!(
            settings.set("mularos_lover", "at home").unwrap_err(),
            "mularos_lover is a room number, or blank."
        );
    }

    #[test]
    fn missing_values_fall_back_to_the_default() {
        let settings = TravelSettings::new();
        assert_eq!(settings.number("typeahead"), 2);
        assert_eq!(settings.number("gigas_min_number"), 4);
        assert!(!settings.on("use_own_ship"));
        assert_eq!(settings.shown().len(), SETTINGS.len());
    }

    #[test]
    fn on_word_accepts_the_recovered_words() {
        assert_eq!(on_word(" ON "), Some(true));
        assert_eq!(on_word("Yes"), Some(true));
        assert_eq!(on_word("false"), Some(false));
        // The recovered arms are only the words; `0`/`1` are not accepted
        // (`on_word`, 0x10152b780..0x10152b9e0: no single-character arm).
        assert_eq!(on_word("0"), None);
        assert_eq!(on_word("1"), None);
        assert_eq!(on_word("maybe"), None);
    }
}
