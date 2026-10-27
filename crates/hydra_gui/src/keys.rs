//! Key bindings: chords, the default movement map and the lookup.
//!
//! Recovered from `hydra_gui::keys` (unslid VAs):
//!
//! | symbol | addr | status |
//! |---|---|---|
//! | `Keys::does` | `0x1003d7be8` | partly (shape + order) |
//! | `Keys::bound` | `0x1003d81f0` | partly |
//! | `Keys::take` | `0x1003d8050` | full (retains non-key events) |
//! | `Chord::parse` | `0x1003d831c` | **full** |
//! | `Chord::written` | `0x1003d8a64` | partly |
//! | `names::known` | `0x10065b430` | **full** |
//! | `names::winit_name` | `0x10065af2c` | partly |
//! | `names::capturable` | `0x101ee2a08` | **full** |
//! | `defaults::defaults` | `0x100796548` | **full** |
//!
//! The original stored `hydra_board::binding::Macro` and
//! `Option<hydra_board::binding::Macro>` values (see the `drop_in_place`
//! symbols for `Keys::layers`'s iterator at `0x1003860e4`). `hydra_board` is
//! being reconstructed in parallel and the original relationship was circular,
//! so [`Macro`] is a local stand-in — see `NOTES.md`.

use std::collections::BTreeMap;

use crate::json::{self, Value};

/// The modifier bits.
///
/// Recovered from `Chord::parse` @`0x1003d831c` (the `orr w26, w8, w26`
/// accumulation @`0x1003d8758`) and confirmed by `vellum::Spelled::of`
/// @`0x1004cefd8`:
///
/// | name | bits |
/// |---|---|
/// | ctrl / control | `0x1` |
/// | shift | `0x2` |
/// | alt / option | `0x4` |
/// | cmd / command / win / super | `0x8` |
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mods(pub u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const CTRL: Mods = Mods(1);
    pub const SHIFT: Mods = Mods(2);
    pub const ALT: Mods = Mods(4);
    pub const CMD: Mods = Mods(8);

    /// Every modifier name `Chord::parse` accepts, with its bits. The lengths in
    /// the recovered comparisons (`cmp x20, #0x3/#0x4/#0x5/#0x6/#0x7`) fix the
    /// spellings exactly.
    pub const NAMES: [(&'static str, Mods); 9] = [
        ("ctrl", Mods::CTRL),
        ("control", Mods::CTRL),
        ("shift", Mods::SHIFT),
        ("alt", Mods::ALT),
        ("option", Mods::ALT),
        ("cmd", Mods::CMD),
        ("command", Mods::CMD),
        ("win", Mods::CMD),
        ("super", Mods::CMD),
    ];

    /// The canonical spelling of one bit, used by [`Chord::written`].
    pub fn word(self) -> Option<&'static str> {
        match self {
            Mods::CTRL => Some("ctrl"),
            Mods::SHIFT => Some("shift"),
            Mods::ALT => Some("alt"),
            Mods::CMD => Some("cmd"),
            _ => None,
        }
    }

    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 != 0
    }

    /// Look a modifier name up, case insensitively.
    pub fn of_name(name: &str) -> Option<Mods> {
        let lower = name.to_ascii_lowercase();
        Mods::NAMES
            .into_iter()
            .find(|(n, _)| *n == lower)
            .map(|(_, bits)| bits)
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, rhs: Mods) -> Mods {
        Mods(self.0 | rhs.0)
    }
}

/// A key plus its modifiers.
///
/// Layout recovered from `Keys::does` (`0x1003d7be8`): a `String` at `+0x00`
/// (its `ptr`/`len` are the `(ptr, len)` pair the lookup compares) and the
/// modifier bits one byte at `+0x18`.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chord {
    pub key: String,
    pub mods: Mods,
}

impl Chord {
    pub fn new(key: impl Into<String>, mods: Mods) -> Self {
        Chord {
            key: key.into(),
            mods,
        }
    }

    /// Parse `ctrl+shift+a` style text.
    ///
    /// `Chord::parse` @`0x1003d831c` is fully recovered:
    ///
    /// 1. the text is split on `'+'` (`movi.2s v0, #0x2b` @`0x1003d8358`) and
    ///    collected (`from_iter` @`0x1003d8374`);
    /// 2. with a single token, [`names::known`] names the key and the modifier
    ///    bits stay zero (`w26` is set to `0` at `0x1003d8398`);
    /// 3. with several tokens, **every** token but the last is matched against
    ///    the modifier names (ASCII-lowercased in place, `orr ... #0x20`) and
    ///    the last token is named by [`names::known`];
    /// 4. failures produce the three recovered messages:
    ///    `` `{}` names no key. `` (`0x103e274dd`),
    ///    `` `{}` in `{}` is not Ctrl, Shift, Alt or Cmd. `` (`0x103e274f1`)
    ///    and `` `{}`: {} `` (`0x103e2751f`).
    pub fn parse(text: &str) -> Result<Chord, String> {
        let tokens: Vec<&str> = text.split('+').collect();
        if tokens.is_empty() {
            return Err(format!("`{text}` names no key."));
        }
        if tokens.len() == 1 {
            return match names::known(tokens[0]) {
                Some(key) => Ok(Chord::new(key, Mods::NONE)),
                None => Err(format!(
                    "`{}`: {}",
                    tokens[0],
                    "types: hold Ctrl, Alt or Cmd with it, or it could not be typed."
                )),
            };
        }
        let mut mods = Mods::NONE;
        for token in &tokens[..tokens.len() - 1] {
            match Mods::of_name(token) {
                Some(bits) => mods = mods | bits,
                None => {
                    return Err(format!(
                        "`{token}` in `{text}` is not Ctrl, Shift, Alt or Cmd."
                    ))
                }
            }
        }
        let last = tokens[tokens.len() - 1];
        match names::known(last) {
            Some(key) => Ok(Chord::new(key, mods)),
            None => Err(format!(
                "`{last}`: {}",
                "types: hold Ctrl, Alt or Cmd with it, or it could not be typed."
            )),
        }
    }

    /// Write the chord back out, modifiers first in the canonical order.
    ///
    /// `Chord::written` @`0x1003d8a64` formats `mods` then the key name; this
    /// keeps the canonical modifier words and the `'+'` separator.
    // UNKNOWN: the exact order/spelling `written` emits could not be read out of
    // the 740-byte body; `ctrl+shift+alt+cmd` order is used.
    pub fn written(&self) -> String {
        let mut out = String::new();
        for bits in [Mods::CTRL, Mods::SHIFT, Mods::ALT, Mods::CMD] {
            if self.mods.contains(bits) {
                if let Some(word) = bits.word() {
                    out.push_str(word);
                    out.push('+');
                }
            }
        }
        out.push_str(&self.key);
        out
    }

    pub fn to_json(&self) -> Value {
        json::obj([
            ("key", Value::Str(self.key.clone())),
            ("mods", Value::Num(self.mods.0 as f64)),
        ])
    }

    pub fn from_json(v: &Value) -> Option<Chord> {
        Some(Chord {
            key: v.get("key")?.as_str()?.to_string(),
            mods: Mods(v.get("mods").and_then(Value::as_f64).unwrap_or(0.0) as u8),
        })
    }
}

/// What a chord does.
///
/// Stand-in for `hydra_board::binding::Macro`: the recovered default table maps
/// a chord to a command word such as `"north"` (see [`defaults::defaults`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Macro {
    pub says: String,
}

impl Macro {
    pub fn new(says: impl Into<String>) -> Self {
        Macro { says: says.into() }
    }
}

/// How many binding layers `Keys` keeps.
///
/// `Keys::does` @`0x1003d7be8` clips the layer byte with `min(byte, 9)` and
/// indexes a 24-byte-strided array of maps (`umaddl x8, w0, w9(=0x18), x8`
/// @`0x1003d7e48`); the two bounds checks at `0x1003d8024`/`0x1003d8010` are
/// `index out of bounds: the len is 10` for `crates/hydra-gui/src/keys.rs`
/// lines 342 and 344.
pub const LAYERS: usize = 10;

/// Every binding a character has.
///
/// Layout recovered from `Keys::does` @`0x1003d7be8`: a `BTreeMap` at `+0x00`
/// (the shared bindings, consulted last — `ldr x22, [x26]` @`0x1003d7f14`) and
/// the per-layer array at `+0x18`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    /// Bindings every layer sees.
    pub every: BTreeMap<Chord, Macro>,
    /// Bindings per layer; index 0 is the layer used when no kind is given.
    pub layers: [BTreeMap<Chord, Option<Macro>>; LAYERS],
}

impl Keys {
    /// The layer index a kind byte selects: `min(kind, 9)`.
    pub fn layer_of(kind: Option<u8>) -> usize {
        match kind {
            None => 0,
            Some(k) => (k as usize).min(LAYERS - 1),
        }
    }

    /// What `chord` does, if anything.
    ///
    /// `Keys::does` @`0x1003d7be8` looks the chord up in the map selected by the
    /// kind byte first (`0x1003d7e40`..`0x1003d7ed8`) and falls back to the
    /// shared map (`0x1003d7f14`..`0x1003d7fd4`), returning `Option<&Macro>`
    /// (`csel x21, xzr, x9, eq` @`0x1003d7dfc`).
    pub fn does(&self, kind: Option<u8>, chord: &Chord) -> Option<&Macro> {
        let layer = &self.layers[Keys::layer_of(kind)];
        match layer.get(chord) {
            Some(mac) => mac.as_ref(),
            None => self.every.get(chord),
        }
    }

    /// Insert/replace a binding on one layer.
    pub fn bind(&mut self, kind: Option<u8>, chord: Chord, action: Option<Macro>) {
        self.layers[Keys::layer_of(kind)].insert(chord, action);
    }

    /// The default movement map, as bindings every layer sees.
    pub fn with_defaults() -> Keys {
        let mut keys = Keys::default();
        for (chord, says) in defaults::defaults() {
            keys.every.insert(chord, Macro::new(says));
        }
        keys
    }

    /// Drop every `Event::Text`-style event from a queue, keeping the rest.
    ///
    /// `Keys::take` @`0x1003d8050` retains the events whose discriminant is not
    /// `0xc` (`cmp w8, #0xc` @`0x1003d80d4`, 72-byte `egui::Event`s). Modelled on
    /// a plain event list here.
    pub fn take<T>(&self, events: &mut Vec<KeyEvent>) -> Vec<T>
    where
        T: From<KeyEvent>,
    {
        let mut taken = Vec::new();
        events.retain(|event| {
            if event.is_text() {
                taken.push(T::from(event.clone()));
                false
            } else {
                true
            }
        });
        taken
    }
}

/// A key event, as much of `egui::Event` as the recovered code touches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyEvent {
    /// `egui::Event::Text` — discriminant `0xc` in `Keys::take`.
    Text(String),
    /// Anything else the original passed through untouched.
    Other,
}

impl KeyEvent {
    pub fn is_text(&self) -> bool {
        matches!(self, KeyEvent::Text(_))
    }
}

impl From<KeyEvent> for String {
    fn from(event: KeyEvent) -> String {
        match event {
            KeyEvent::Text(text) => text,
            KeyEvent::Other => String::new(),
        }
    }
}

/// Key names.
///
/// The command word for a chord. `defaults` @`0x100796548` copies a static table
/// of 11 32-byte `(&str, &str)` pairs (`memcpy` of `0x160` bytes from
/// `0x10439e418`, `mov w21, #0xb`) and inserts each into a `BTreeMap` keyed by
/// the parsed [`Chord`].
pub mod defaults {
    use super::{Chord, Mods};
    use std::collections::BTreeMap;

    /// `(chord, command)` — the recovered table, verbatim.
    pub const TABLE: [(&str, &str); 11] = [
        ("Numpad8", "north"),
        ("Numpad2", "south"),
        ("Numpad6", "east"),
        ("Numpad4", "west"),
        ("Numpad9", "northeast"),
        ("Numpad7", "northwest"),
        ("Numpad3", "southeast"),
        ("Numpad1", "southwest"),
        ("Numpad5", "out"),
        ("Numpad0", "down"),
        ("NumpadDecimal", "up"),
    ];

    /// The default bindings, keyed by parsed chord.
    ///
    /// All eleven are unmodified numpad keys (`Mods::NONE`); their names come
    /// straight from the table, so [`super::names::known`] resolves them without
    /// needing the alias table.
    pub fn defaults() -> BTreeMap<Chord, String> {
        TABLE
            .into_iter()
            .map(|(chord, says)| (Chord::new(chord, Mods::NONE), says.to_string()))
            .collect()
    }
}

/// Key name resolution.
pub mod names {
    use super::Chord;
    use std::collections::HashSet;

    /// `(name, alias)` pairs — the 16-entry static table copied by
    /// `names::known` @`0x10065b458` (`0x200` bytes at `0x104394298`, 32-byte
    /// stride). The alias is the GemStone/Wrayth spelling, the name is the
    /// canonical one that is returned.
    pub const ALIASES: [(&str, &str); 16] = [
        ("Numpad0", "num_0"),
        ("Numpad1", "num_1"),
        ("Numpad2", "num_2"),
        ("Numpad3", "num_3"),
        ("Numpad4", "num_4"),
        ("Numpad5", "num_5"),
        ("Numpad6", "num_6"),
        ("Numpad7", "num_7"),
        ("Numpad8", "num_8"),
        ("Numpad9", "num_9"),
        ("NumpadAdd", "num_plus"),
        ("NumpadSubtract", "num_minus"),
        ("NumpadMultiply", "num_multiply"),
        ("NumpadDivide", "num_divide"),
        ("NumpadEnter", "num_enter"),
        ("NumpadDecimal", "num_decimal"),
    ];

    /// The search order for winit key codes — the 117-byte permutation copied at
    /// `0x1021403e1` by `names::known` @`0x10065b5b4`.
    pub const SEARCH_ORDER: [u8; 117] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x08, 0x12, 0x13, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x14, 0x15, 0x16, 0x17, 0x18, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c,
        0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b,
        0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a,
        0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
        0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
        0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74,
    ];

    /// `egui::Key`'s variant names, in discriminant order.
    ///
    /// These are the literals of `<egui::data::key::Key as core::fmt::Debug>::
    /// fmt` @`0x10064d2bc`, which indexes the pointer table at `0x1043947b0`
    /// with the `Key` discriminant and the length table at `0x102140490`.
    pub const KEY_NAMES: [&'static str; 138] = [
        "ArrowDown", "ArrowLeft", "ArrowRight", "ArrowUp", "Escape", "Tab", "Backspace", "Enter",
        "Space", "Insert", "Delete", "Home", "End", "PageUp", "PageDown", "Copy",
        "Cut", "Paste", "Colon", "Comma", "Backslash", "Slash", "Pipe", "Questionmark",
        "Exclamationmark", "OpenBracket", "CloseBracket", "OpenCurlyBracket", "CloseCurlyBracket", "Backtick", "Minus", "Period",
        "Plus", "Equals", "Semicolon", "Quote", "Num0", "Num1", "Num2", "Num3",
        "Num4", "Num5", "Num6", "Num7", "Num8", "Num9", "A", "B",
        "C", "D", "E", "F", "G", "H", "I", "J",
        "K", "L", "M", "N", "O", "P", "Q", "R",
        "S", "T", "U", "V", "W", "X", "Y", "Z",
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8",
        "F9", "F10", "F11", "F12", "F13", "F14", "F15", "F16",
        "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
        "F25", "F26", "F27", "F28", "F29", "F30", "F31", "F32",
        "F33", "F34", "F35", "BrowserBack", "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight",
        "AltLeft", "AltRight", "SuperLeft", "SuperRight", "IntlBackslash", "LaunchApp1", "LaunchApp2", "LaunchMail",
        "MediaPlayPause", "MediaSelect", "MediaStop", "MediaTrackNext", "MediaTrackPrevious", "Power", "Sleep", "AudioVolumeDown",
        "AudioVolumeMute", "AudioVolumeUp", "WakeUp", "Meta", "Hyper", "Turbo", "Abort", "Resume",
        "Suspend", "Again",
    ];

    /// The name a chord uses for a key.
    ///
    /// `names::winit_name` @`0x10065af2c` takes an **`egui::Key` discriminant**
    /// (the `Debug` impl above is the formatting argument at `0x10065af6c`), and:
    ///
    /// * `0x6c ..= 0x73` (`ShiftLeft` … `SuperRight`) return `None`: the message
    ///   recovered next to that branch says these keys are "bound to nothing"
    ///   (`0x103e2b85a`);
    /// * `0x0f ..= 0x12` (`Copy`, `Cut`, `Paste`, `Colon`), `0x16 ..= 0x18`
    ///   (`Pipe`, `Questionmark`, `Exclamationmark`), `0x1b ..= 0x1c`
    ///   (`OpenCurlyBracket`, `CloseCurlyBracket`) and `0x20` (`Plus`) return
    ///   `None` (`b.lo 0x10065b0d0` chains from `0x10065b058`);
    /// * `0x19` → `"BracketLeft"`, `0x1a` → `"BracketRight"`,
    ///   `0x1d` → `"Backquote"`, `0x21` → `"Equal"` — the four names where
    ///   winit's spelling differs from egui's (`0x10065b104`, `0x10065b098`,
    ///   `0x10065b114`, `0x10065b0c0`);
    /// * a one-character name (the letters `A`..`Z`) becomes `"Key" + name`
    ///   (`format!("Key{}")`, the literal at `0x103e290ec`, @`0x10065af9c`);
    /// * `Num0`..`Num9` become `Digit0`..`Digit9`
    ///   (`format!("Digit{}", &text[3..])`, literal at `0x103e290e4`,
    ///   @`0x10065b000`);
    /// * everything else is the `egui::Key` name unchanged (`0x10065b15c`).
    pub fn winit_name(code: u8) -> Option<String> {
        if (0x6c..=0x73).contains(&code) {
            return None;
        }
        let name = *KEY_NAMES.get(code as usize)?;
        if name.chars().count() == 1 {
            return Some(format!("Key{name}"));
        }
        if (0x24..=0x2d).contains(&code) && name.starts_with("Num") {
            return Some(format!("Digit{}", &name[3..]));
        }
        match code {
            0x0f..=0x12 | 0x16..=0x18 | 0x1b..=0x1c | 0x20 => return None,
            0x19 => return Some("BracketLeft".to_string()),
            0x1a => return Some("BracketRight".to_string()),
            0x1d => return Some("Backquote".to_string()),
            0x21 => return Some("Equal".to_string()),
            _ => {}
        }
        Some(name.to_string())
    }

    /// The canonical name for `name`, matched case insensitively.
    ///
    /// `names::known` @`0x10065b430`: first the 16-entry alias table
    /// ([`ALIASES`], comparing both the name and the alias, returning a copy of
    /// the *name*), then [`winit_name`] over [`SEARCH_ORDER`].
    pub fn known(name: &str) -> Option<String> {
        for (canonical, alias) in ALIASES {
            if canonical.eq_ignore_ascii_case(name) || alias.eq_ignore_ascii_case(name) {
                return Some(canonical.to_string());
            }
        }
        for code in SEARCH_ORDER {
            if let Some(found) = winit_name(code) {
                if found.eq_ignore_ascii_case(name) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// The key codes that may be captured.
    ///
    /// `names::capturable` @`0x101ee2a08` builds a `HashSet<winit::keyboard::
    /// KeyCode>` and inserts five values: `0x77`, `0x76`, `0x75`, `0x35` and
    /// `0x36` (`mov w1, #0x77/#0x76/#0x75/#0x35/#0x36` @`0x10065a74`..).
    pub const CAPTURABLE: [u8; 5] = [0x77, 0x76, 0x75, 0x35, 0x36];

    pub fn capturable() -> HashSet<u8> {
        CAPTURABLE.into_iter().collect()
    }

    /// A chord from a key code, or `None` when the code has no name.
    pub fn chord_of(code: u8, mods: super::Mods) -> Option<Chord> {
        winit_name(code).map(|key| Chord::new(key, mods))
    }
}

/// The key file that a character's bindings are read from.
///
/// `keys::character_path` @`0x1003d79b8` and `Keybinds::in_file`
/// @`0x1003d9408` build the path; the recovered shape is
/// `<data dir>/<character>.keys`. Only the naming rule is reproduced here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keybinds {
    pub rows: BTreeMap<Chord, Macro>,
}

impl Keybinds {
    /// `keys::character_path` @`0x1003d79b8`: join the data dir with the
    /// character name and the `keys` file name.
    ///
    /// UNKNOWN: the exact suffix constant (`.keys` / `keys.json`) is not in the
    /// recovered string pool next to this function; `.keys` is used.
    pub fn character_path(data_dir: &std::path::Path, character: &str) -> std::path::PathBuf {
        data_dir.join(format!("{character}.keys"))
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Read bindings from the written JSON form: `{"<chord>": "<command>"}`.
    ///
    /// `Keybinds::read` @`0x1003d8e1c` reads the file (`KeyFile::read`
    /// @`0x1004cb9f8`) and merges [`defaults::defaults`] underneath
    /// (`bl defaults` @`0x1003d8eac`).
    pub fn read(text: &str) -> Result<Keybinds, String> {
        let value = json::parse(text)?;
        let Value::Obj(fields) = value else {
            return Err("expected an object of chords".into());
        };
        let mut rows: BTreeMap<Chord, Macro> = defaults::defaults()
            .into_iter()
            .map(|(chord, says)| (chord, Macro::new(says)))
            .collect();
        for (chord, action) in fields {
            let chord = Chord::parse(&chord)?;
            let says = action.as_str().ok_or("expected a command string")?;
            rows.insert(chord, Macro::new(says));
        }
        Ok(Keybinds { rows })
    }

    /// Write the bindings back out in chord order.
    pub fn write(&self) -> String {
        Value::Obj(
            self.rows
                .iter()
                .map(|(chord, mac)| (chord.written(), Value::Str(mac.says.clone())))
                .collect(),
        )
        .write()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_bits_match_the_recovered_immediates() {
        assert_eq!(Mods::CTRL.0, 1);
        assert_eq!(Mods::SHIFT.0, 2);
        assert_eq!(Mods::ALT.0, 4);
        assert_eq!(Mods::CMD.0, 8);
        for (name, bits) in Mods::NAMES {
            assert_eq!(Mods::of_name(name), Some(bits), "{name}");
            assert_eq!(Mods::of_name(&name.to_uppercase()), Some(bits), "{name}");
        }
    }

    #[test]
    fn parse_single_key_has_no_modifiers() {
        let chord = Chord::parse("Numpad8").unwrap();
        assert_eq!(chord, Chord::new("Numpad8", Mods::NONE));
    }

    #[test]
    fn parse_accepts_every_modifier_spelling() {
        // `Numpad8` is one of the names the recovered tables can resolve without
        // the winit code → name step, which is still UNKNOWN (see NOTES).
        assert_eq!(
            Chord::parse("ctrl+shift+alt+cmd+num_8").unwrap().mods,
            Mods::CTRL | Mods::SHIFT | Mods::ALT | Mods::CMD
        );
        assert_eq!(Chord::parse("control+numpad8").unwrap().mods, Mods::CTRL);
        assert_eq!(Chord::parse("option+numpad8").unwrap().mods, Mods::ALT);
        assert_eq!(Chord::parse("win+numpad8").unwrap().mods, Mods::CMD);
        assert_eq!(Chord::parse("super+numpad8").unwrap().mods, Mods::CMD);
        assert_eq!(Chord::parse("command+numpad8").unwrap().mods, Mods::CMD);
        // Case insensitive (`orr ... #0x20` in the recovered comparison).
        assert_eq!(Chord::parse("CTRL+NUM_8").unwrap().mods, Mods::CTRL);
        assert_eq!(Chord::parse("CTRL+NUM_8").unwrap().key, "Numpad8");
        assert_eq!(Chord::parse("ctrl+keya").unwrap().key, "KeyA");
        assert_eq!(Chord::parse("alt+digit0").unwrap().key, "Digit0");
    }

    #[test]
    fn parse_resolves_aliases_and_reports_errors() {
        assert_eq!(Chord::parse("num_8").unwrap().key, "Numpad8");
        assert_eq!(Chord::parse("numpaddecimal").unwrap().key, "NumpadDecimal");
        assert_eq!(
            Chord::parse("hyper+a").unwrap_err(),
            "`hyper` in `hyper+a` is not Ctrl, Shift, Alt or Cmd."
        );
        assert_eq!(
            Chord::parse("ctrl+Nosuchkey").unwrap_err(),
            "`Nosuchkey`: types: hold Ctrl, Alt or Cmd with it, or it could not be typed."
        );
    }

    #[test]
    fn written_round_trips_through_parse() {
        let chord = Chord::new("Numpad4", Mods::CTRL | Mods::ALT);
        assert_eq!(chord.written(), "ctrl+alt+Numpad4");
        assert_eq!(Chord::parse(&chord.written()).unwrap(), chord);
    }

    #[test]
    fn defaults_are_the_eleven_numpad_moves() {
        let map = defaults::defaults();
        assert_eq!(map.len(), 11);
        assert_eq!(map[&Chord::new("Numpad8", Mods::NONE)], "north");
        assert_eq!(map[&Chord::new("Numpad5", Mods::NONE)], "out");
        assert_eq!(map[&Chord::new("NumpadDecimal", Mods::NONE)], "up");
        assert_eq!(defaults::TABLE.len(), 11);
    }

    #[test]
    fn known_resolves_aliases_case_insensitively() {
        assert_eq!(names::known("num_0").as_deref(), Some("Numpad0"));
        assert_eq!(names::known("NUM_9").as_deref(), Some("Numpad9"));
        assert_eq!(names::known("numpadadd").as_deref(), Some("NumpadAdd"));
        assert_eq!(names::known("bracketleft").as_deref(), Some("BracketLeft"));
        assert_eq!(names::known("backquote").as_deref(), Some("Backquote"));
        assert_eq!(names::known("keya").as_deref(), Some("KeyA"));
        assert_eq!(names::known("digit0").as_deref(), Some("Digit0"));
        assert_eq!(names::known("Enter").as_deref(), Some("Enter"));
        // `LaunchApp1` (0x75) is outside the 117-entry search order.
        assert_eq!(names::known("launchapp1"), None);
        assert_eq!(names::known("nothinghere"), None);
    }

    #[test]
    fn key_names_are_the_egui_debug_table() {
        assert_eq!(names::KEY_NAMES.len(), 138);
        assert_eq!(names::KEY_NAMES[0], "ArrowDown");
        assert_eq!(names::KEY_NAMES[8], "Space");
        assert_eq!(names::KEY_NAMES[0x19], "OpenBracket");
        assert_eq!(names::KEY_NAMES[0x24], "Num0");
        assert_eq!(names::KEY_NAMES[0x2e], "A");
        assert_eq!(names::KEY_NAMES[0x47], "Z");
        assert_eq!(names::KEY_NAMES[0x6b], "BrowserBack");
        assert_eq!(names::KEY_NAMES[0x6c], "ShiftLeft");
        assert_eq!(names::KEY_NAMES[0x74], "IntlBackslash");
        assert_eq!(names::KEY_NAMES[0x89], "Again");
    }

    #[test]
    fn winit_name_translates_egui_names_to_winit_ones() {
        // Letters and digits are rebuilt (`format!("Key{}")` / `"Digit{}"`).
        assert_eq!(names::winit_name(0x2e).as_deref(), Some("KeyA"));
        assert_eq!(names::winit_name(0x47).as_deref(), Some("KeyZ"));
        assert_eq!(names::winit_name(0x24).as_deref(), Some("Digit0"));
        assert_eq!(names::winit_name(0x2d).as_deref(), Some("Digit9"));
        // The four spellings winit writes differently.
        assert_eq!(names::winit_name(0x19).as_deref(), Some("BracketLeft"));
        assert_eq!(names::winit_name(0x1a).as_deref(), Some("BracketRight"));
        assert_eq!(names::winit_name(0x1d).as_deref(), Some("Backquote"));
        assert_eq!(names::winit_name(0x21).as_deref(), Some("Equal"));
        // Everything else keeps its name.
        assert_eq!(names::winit_name(0x00).as_deref(), Some("ArrowDown"));
        assert_eq!(names::winit_name(0x07).as_deref(), Some("Enter"));
        assert_eq!(names::winit_name(0x48).as_deref(), Some("F1"));
        assert_eq!(names::winit_name(0x6a).as_deref(), Some("F35"));
    }

    #[test]
    fn winit_name_keeps_the_recovered_gaps() {
        // The eight modifier keys are "bound to nothing".
        for code in 0x6c..=0x73u8 {
            assert_eq!(names::winit_name(code), None, "{code:#x}");
        }
        // Copy, Cut, Paste, Colon | Pipe, Questionmark, Exclamationmark |
        // OpenCurlyBracket, CloseCurlyBracket | Plus.
        for code in [0x0fu8, 0x10, 0x11, 0x12, 0x16, 0x17, 0x18, 0x1b, 0x1c, 0x20] {
            assert_eq!(names::winit_name(code), None, "{code:#x}");
        }
        assert_eq!(names::winit_name(0xff), None);
    }

    #[test]
    fn search_order_is_the_recovered_permutation() {
        assert_eq!(names::SEARCH_ORDER.len(), 117);
        assert_eq!(&names::SEARCH_ORDER[..19], &[0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16, 17, 8, 18]);
        assert_eq!(&names::SEARCH_ORDER[19..31], &[0x13, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x14]);
        assert_eq!(names::SEARCH_ORDER[116], 0x74);
    }

    #[test]
    fn capturable_is_the_recovered_five_codes() {
        let set = names::capturable();
        assert_eq!(set.len(), 5);
        for code in [0x77u8, 0x76, 0x75, 0x35, 0x36] {
            assert!(set.contains(&code), "{code:#x}");
        }
    }

    #[test]
    fn layer_clipping_matches_min_kind_nine() {
        assert_eq!(Keys::layer_of(None), 0);
        assert_eq!(Keys::layer_of(Some(0)), 0);
        assert_eq!(Keys::layer_of(Some(9)), 9);
        assert_eq!(Keys::layer_of(Some(200)), 9);
        assert_eq!(LAYERS, 10);
    }

    #[test]
    fn does_prefers_the_layer_then_the_shared_map() {
        let mut keys = Keys::with_defaults();
        let chord = Chord::new("Numpad8", Mods::NONE);
        assert_eq!(keys.does(Some(3), &chord).map(|m| m.says.as_str()), Some("north"));

        keys.bind(Some(3), chord.clone(), Some(Macro::new("n")));
        assert_eq!(keys.does(Some(3), &chord).map(|m| m.says.as_str()), Some("n"));
        // Other layers still see the shared binding.
        assert_eq!(keys.does(Some(4), &chord).map(|m| m.says.as_str()), Some("north"));

        // An explicit `None` action on a layer shadows the shared binding.
        keys.bind(Some(4), chord.clone(), None);
        assert_eq!(keys.does(Some(4), &chord), None);
        assert!(keys.does(None, &chord).is_some());
    }

    #[test]
    fn take_removes_only_text_events() {
        let keys = Keys::default();
        let mut events = vec![
            KeyEvent::Other,
            KeyEvent::Text("a".into()),
            KeyEvent::Other,
        ];
        let taken: Vec<String> = keys.take(&mut events);
        assert_eq!(taken, vec!["a".to_string()]);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn keybinds_round_trip_json_under_the_defaults() {
        let bindings = Keybinds::read(r#"{"ctrl+num_8":"look"}"#).unwrap();
        assert_eq!(bindings.len(), 12);
        assert_eq!(
            bindings
                .rows
                .get(&Chord::new("Numpad8", Mods::CTRL))
                .map(|m| m.says.as_str()),
            Some("look")
        );
        assert_eq!(
            bindings.rows[&Chord::new("Numpad8", Mods::NONE)],
            Macro::new("north")
        );
        let text = bindings.write();
        assert!(text.starts_with(
            r#"{"Numpad0":"down","Numpad1":"southwest","Numpad2":"south","Numpad3":"southeast""#
        ));
        assert!(text.contains(r#""ctrl+Numpad8":"look""#));
        let again = Keybinds::read(&text).unwrap();
        assert_eq!(again.rows.len(), bindings.rows.len());
    }

    #[test]
    fn character_path_appends_the_keys_file() {
        let path = Keybinds::character_path(std::path::Path::new("/tmp/data"), "Aelish");
        assert_eq!(path, std::path::PathBuf::from("/tmp/data/Aelish.keys"));
    }
}
