//! Controller button names: how they rank for the legend, and how a chord like
//! `l2+south` is spelled and read.
//!
//! **ESTABLISHED**:
//!
//!  * [`rank`] is a plain match over the button names, returning `0` for `south`
//!    through `16` for `guide` and `17` for anything else (`0x101470618`).
//!  * `BindKey` is `{ modifiers: Vec<String>, key: String }`; [`BindKey::parse`]
//!    splits on `+` (`0x101470e58`) and [`BindKey::canonical`] joins with `+`
//!    (`0x101471014`, template `{}+{}` at `0x103e6a77a`).
//!  * [`BindKey::known`] answers from the key list the binary carries
//!    (`0x10207c884`): `backspace`, `ctrl+left`, `f3`, `num_1` … `num_9`,
//!    `ArrowLeft`, `PageUp`, and the rest.

/// Where a button sits in the legend: lower is earlier.
///
/// **ESTABLISHED** (`0x101470618`).
pub const RANK_SOUTH: u8 = 0;
pub const RANK_EAST: u8 = 1;
pub const RANK_NORTH: u8 = 2;
pub const RANK_WEST: u8 = 3;
pub const RANK_DPAD_UP: u8 = 4;
pub const RANK_DPAD_DOWN: u8 = 5;
pub const RANK_DPAD_LEFT: u8 = 6;
pub const RANK_DPAD_RIGHT: u8 = 7;
pub const RANK_L1: u8 = 8;
pub const RANK_R1: u8 = 9;
pub const RANK_L2: u8 = 10;
pub const RANK_R2: u8 = 11;
pub const RANK_L3: u8 = 12;
pub const RANK_R3: u8 = 13;
pub const RANK_SELECT: u8 = 14;
pub const RANK_START: u8 = 15;
pub const RANK_GUIDE: u8 = 16;
/// What a name the binary does not know ranks as.
pub const RANK_UNKNOWN: u8 = 17;

/// The legend order of a controller button.
///
/// **ESTABLISHED** (`0x101470618`): a match on the trimmed name, `17` for
/// anything else.
pub fn rank(button: &str) -> u8 {
    match button.trim() {
        "south" => RANK_SOUTH,
        "east" => RANK_EAST,
        "north" => RANK_NORTH,
        "west" => RANK_WEST,
        "dpad_up" => RANK_DPAD_UP,
        "dpad_down" => RANK_DPAD_DOWN,
        "dpad_left" => RANK_DPAD_LEFT,
        "dpad_right" => RANK_DPAD_RIGHT,
        "l1" => RANK_L1,
        "r1" => RANK_R1,
        "l2" => RANK_L2,
        "r2" => RANK_R2,
        "l3" => RANK_L3,
        "r3" => RANK_R3,
        "select" => RANK_SELECT,
        "start" => RANK_START,
        "guide" => RANK_GUIDE,
        _ => RANK_UNKNOWN,
    }
}

/// The buttons a controller bind may name, in the legend's order.
///
/// **ESTABLISHED** (`0x10207f4d2`): the comment the default `controller.toml`
/// carries.
pub const BUTTONS: [&str; 17] = [
    "south",
    "east",
    "north",
    "west",
    "dpad_up",
    "dpad_down",
    "dpad_left",
    "dpad_right",
    "l1",
    "r1",
    "l2",
    "r2",
    "l3",
    "r3",
    "select",
    "start",
    "guide",
];

/// The keys the binary knows by name.
///
/// **ESTABLISHED** (`0x10207c884`, `0x102418e84`): the literals the key matcher
/// compares against.
pub const KEYS: [&str; 76] = [
    // named by Hydra
    "backspace", "ctrl+left", "ctrl+right", "ctrl+w", "ctrl+r", "ctrl+t",
    "ctrl+alt+left", "ctrl+alt+right", "ctrl+alt+up", "ctrl+alt+down",
    "ctrl+page_up", "ctrl+page_down", "alt+page_up", "alt+page_down",
    "f3", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
    "num_1", "num_2", "num_3", "num_4", "num_5", "num_6", "num_7", "num_8",
    "num_9", "num_0", "num_decimal", "num_minus", "num_multiply", "num_divide",
    "num_add", "num_subtract", "num_times", "num_period", "num_plus",
    // the key names a keyboard layout spells out
    "Backquote", "Minus", "Equal", "BracketLeft", "BracketRight", "Backslash",
    "Semicolon", "Quote", "Comma", "Period", "Slash", "Enter", "Backspace",
    "Delete", "Insert", "Tab", "Escape", "Space", "ArrowLeft", "ArrowRight",
    "ArrowUp", "ArrowDown", "End", "Home", "PageUp", "PageDown", "CapsLock",
    "Shift", "Control", "Alt", "Super", "insert", "Num1", "Num2", "Num3",
];

/// A modifier and key chord such as `l2+south` or `ctrl+alt+down`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindKey {
    /// Everything before the last `+`, deduplicated.
    pub modifiers: Vec<String>,
    /// The key itself, always the last piece.
    pub key: String,
}

impl BindKey {
    /// A bare key with no modifiers.
    pub fn new(key: &str) -> BindKey {
        BindKey {
            modifiers: Vec::new(),
            key: key.trim().to_string(),
        }
    }

    /// Read a chord.
    ///
    /// **ESTABLISHED** (`0x101470e58`): the text is split on `+`, adjacent
    /// duplicates are dropped, and the last piece is the key.  An empty text
    /// gives `None`.
    pub fn parse(text: &str) -> Option<BindKey> {
        let mut pieces: Vec<String> = text
            .split('+')
            .map(|piece| piece.trim().to_string())
            .filter(|piece| !piece.is_empty())
            .collect();
        let key = pieces.pop()?;
        let mut modifiers: Vec<String> = Vec::new();
        for piece in pieces {
            if modifiers.last() != Some(&piece) {
                modifiers.push(piece);
            }
        }
        Some(BindKey { modifiers, key })
    }

    /// The chord as one string.
    ///
    /// **ESTABLISHED** (`0x101471014`): the modifiers joined with `+` and the
    /// key appended, or just the key when there are none.
    pub fn canonical(&self) -> String {
        if self.modifiers.is_empty() {
            self.key.clone()
        } else {
            format!("{}+{}", self.modifiers.join("+"), self.key)
        }
    }

    /// Is the key one the binary has a name for?
    ///
    /// **PARTLY** (`0x101470bb4`): the binary compares against its own list; the
    /// list here is the same one, but whether it also accepted single characters
    /// and `f1`–`f24` is UNKNOWN, so those are accepted too.
    pub fn known(&self) -> bool {
        let key = self.key.as_str();
        if KEYS.contains(&key) || key.len() == 1 {
            return true;
        }
        // `f1` to `f24`, and any `num_` name.
        if let Some(rest) = key.strip_prefix('f') {
            if let Ok(n) = rest.parse::<u8>() {
                return (1..=24).contains(&n);
            }
        }
        key.starts_with("num_")
    }

    /// The same chord with the modifiers sorted, so two spellings of one chord
    /// compare equal.
    pub fn sorted(&self) -> BindKey {
        let mut modifiers = self.modifiers.clone();
        modifiers.sort();
        BindKey {
            modifiers,
            key: self.key.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_run_from_south_to_guide() {
        assert_eq!(rank("south"), 0);
        assert_eq!(rank("east"), 1);
        assert_eq!(rank("north"), 2);
        assert_eq!(rank("west"), 3);
        assert_eq!(rank("dpad_up"), 4);
        assert_eq!(rank("dpad_down"), 5);
        assert_eq!(rank("dpad_left"), 6);
        assert_eq!(rank("dpad_right"), 7);
        assert_eq!(rank("l1"), 8);
        assert_eq!(rank("r1"), 9);
        assert_eq!(rank("l2"), 10);
        assert_eq!(rank("r2"), 11);
        assert_eq!(rank("l3"), 12);
        assert_eq!(rank("r3"), 13);
        assert_eq!(rank("select"), 14);
        assert_eq!(rank("start"), 15);
        assert_eq!(rank("guide"), 16);
        assert_eq!(rank("trackpad"), RANK_UNKNOWN);
        // every button in the list has its own rank
        let mut seen: Vec<u8> = BUTTONS.iter().map(|b| rank(b)).collect();
        seen.sort();
        assert_eq!(seen, (0..17).collect::<Vec<u8>>());
    }

    #[test]
    fn a_chord_splits_on_plus_and_keeps_the_last_piece_as_the_key() {
        let chord = BindKey::parse("l2 + south").unwrap();
        assert_eq!(chord.modifiers, vec!["l2"]);
        assert_eq!(chord.key, "south");
        assert_eq!(chord.canonical(), "l2+south");

        let plain = BindKey::parse("south").unwrap();
        assert!(plain.modifiers.is_empty());
        assert_eq!(plain.canonical(), "south");

        // adjacent duplicates are dropped
        let doubled = BindKey::parse("ctrl+ctrl+left").unwrap();
        assert_eq!(doubled.modifiers, vec!["ctrl"]);
        assert_eq!(doubled.canonical(), "ctrl+left");

        assert!(BindKey::parse("").is_none());
        assert!(BindKey::parse("  ").is_none());
    }

    #[test]
    fn known_keys() {
        assert!(BindKey::new("backspace").known());
        assert!(BindKey::new("num_5").known());
        assert!(BindKey::new("ArrowLeft").known());
        assert!(BindKey::new("f7").known());
        assert!(BindKey::new("q").known());
        assert!(!BindKey::new("fnord").known());
        assert!(!BindKey::new("f99").known());
    }

    #[test]
    fn sorting_modifiers_makes_two_spellings_equal() {
        let one = BindKey::parse("ctrl+alt+down").unwrap();
        let two = BindKey::parse("alt+ctrl+down").unwrap();
        assert_ne!(one, two);
        assert_eq!(one.sorted(), two.sorted());
        assert_eq!(one.sorted().canonical(), "alt+ctrl+down");
    }
}
