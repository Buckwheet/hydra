//! Theme tokens and palettes.
//!
//! Everything here was read out of the arm64 binary:
//!
//! * `Token::name`   @0x101842e08 — a 95-entry `&str` table (lengths at
//!   0x103992420, pointers at `__DATA_CONST` 0x104447a58).
//! * `Token::bare`   @0x101842dd0 — three 95-byte tables at 0x1039922ff
//!   (blue), 0x10399235e (green) and 0x1039923bd (red); the packed word is
//!   `r | g << 8 | b << 16`, i.e. an `egui::Color32` without the alpha byte.
//! * `Token::role`   @0x101842e28 — a 32-byte enum per token; see [`Role`].
//! * `Palette::bare` @0x101847978 — `Palette { names, prompt, bare }`, 304
//!   bytes, built from `Token::bare` for every token.
//! * `theme::hex`    @0x101842d74 — `format!("#{:02x}{:02x}{:02x}", r, g, b)`.
//! * `theme::parse_hex` @0x101847e40 — see [`parse_hex`].
//!
//! The `role` data feeds [`harmony`]: `Lift`/`Hue`/`Lightness`/`Under`
//! describe how to re-derive a token from a seed colour.

pub mod file;
pub mod harmony;
pub mod oklch;
pub mod shape;
pub mod typeface;
pub mod wrayth;

use std::collections::BTreeMap;
use std::sync::Arc;

/// A packed 8-bit-per-channel colour: `r | g << 8 | b << 16`.
///
/// That is exactly `egui::Color32`'s byte order with the alpha byte dropped;
/// the recovered code produces and consumes this packing everywhere (see
/// `oklch::contrast`, which reads `r` from the low byte).
pub type Rgb = u32;

/// Build an [`Rgb`] from 8-bit channels.
pub const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// The red channel.
pub const fn red(c: Rgb) -> u8 {
    (c & 0xff) as u8
}
/// The green channel.
pub const fn green(c: Rgb) -> u8 {
    ((c >> 8) & 0xff) as u8
}
/// The blue channel.
pub const fn blue(c: Rgb) -> u8 {
    ((c >> 16) & 0xff) as u8
}

/// `@0x101842d74` — render a colour as `#rrggbb`.
///
/// The binary builds the formatter arguments from three `u8`s taken from the
/// low, middle and high byte of the word, so the string is always lowercase and
/// always six digits.
pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", red(c), green(c), blue(c))
}

/// `@0x101847e40` — parse `#rgb`, `#rrggbb` or `#rrggbbaa`.
///
/// The recovered control flow:
///
/// 1. `trim()` the input; empty or a missing leading `#` is `None`;
/// 2. every remaining byte must be an ASCII hex digit;
/// 3. the length (including the `#`) must be 4, 7 or 9 — nothing else is
///    accepted;
/// 4. for a 4-byte form the six-digit string is built by doubling each nibble
///    (`#abc` -> `aabbcc`);
/// 5. for a 9-byte form the **first six** digits are used and the trailing alpha
///    pair is ignored;
/// 6. each pair is parsed with checked arithmetic; a bad nibble gives `None`.
pub fn parse_hex(text: &str) -> Option<Rgb> {
    let text = text.trim();
    let body = text.strip_prefix('#')?;
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // The binary compares the *whole* string's length: 4 (`#rgb`), 7
    // (`#rrggbb`) or 9 (`#rrggbbaa`).
    let digits: Vec<u8> = match body.len() {
        3 => {
            // `#rgb` -> `rrggbb`, exactly what the binary's `String::from_iter`
            // over the doubled characters produces.
            let mut d = Vec::with_capacity(6);
            for c in body.bytes() {
                d.push(c);
                d.push(c);
            }
            d
        }
        // `#rrggbb` and `#rrggbbaa` (alpha dropped).
        6 | 8 => body.as_bytes()[..6].to_vec(),
        _ => return None,
    };
    let mut ch = [0u8; 3];
    for i in 0..3 {
        let hi = hex_digit(digits[2 * i])?;
        let lo = hex_digit(digits[2 * i + 1])?;
        ch[i] = (hi << 4) | lo;
    }
    Some(rgb(ch[0], ch[1], ch[2]))
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// What a token is *for*, as far as theme generation is concerned.
///
/// `@0x101842e28` returns a 32-byte value: a discriminant word (`0`, `1`, `2`,
/// `3`, `4` or `5`) followed by one or two `f64`s.  The recovered mapping is:
///
/// | disc | payload    | meaning                                            |
/// |------|------------|----------------------------------------------------|
/// | 0    | `u64, f64` | a base surface token (0..=5) and a lightness delta  |
/// | 1    | `f64`      | a hue rotation of the seed                          |
/// | 2    | `f64`      | an absolute OKLCH lightness                         |
/// | 3    | `f64`      | a lightness lift from the seed                      |
/// | 4    | —          | fixed                                               |
/// | 5    | —          | fixed (map overlays: `veil`, `grid`, `mark`)         |
///
/// The evidence for the reading: every absolute lightness is a plausible OKLCH
/// `L` (0.5, 0.55, 0.7, 0.75), every hue rotation is small (|dh| <= 0.3) and
/// every `Lift`/`Under` operand is a small brightness delta.  The binary's own
/// `harmony::hue_variants` only consults the payload of discriminant `0`, which
/// is why `Under` is the only variant that carries a base token.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    /// Take surface token `n` (`canvas`..`line_strong`) and move its lightness
    /// by this amount.
    Under(u8, f64),
    /// Rotate the seed hue by this fraction of a turn.
    Hue(f64),
    /// Use this OKLCH lightness with the seed's hue.
    Lightness(f64),
    /// Lift the seed's lightness by this amount.
    Lift(f64),
    /// Never generated: the colour is fixed.
    Fixed,
}

/// One themeable colour.
///
/// The 95 variants and their order are the binary's: the tables are indexed by
/// the variant's `u8` discriminant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Token {
    Canvas = 0,
    Surface = 1,
    Raised = 2,
    Inset = 3,
    Line = 4,
    LineStrong = 5,
    Text = 6,
    Muted = 7,
    Selection = 8,
    Tab = 9,
    TabShown = 10,
    Accent = 11,
    Link = 12,
    Hydra = 13,
    Warning = 14,
    Wrong = 15,
    RoomName = 16,
    RoomPlate = 17,
    Creature = 18,
    Player = 19,
    Object = 20,
    Container = 21,
    Shut = 22,
    Speech = 23,
    Whisper = 24,
    Thought = 25,
    RoomDesc = 26,
    Watching = 27,
    Command = 28,
    SelectedLink = 29,
    CreatureDeath = 30,
    FatalCrit = 31,
    CreaturePrep = 32,
    CreatureTrigger = 33,
    CreatureRecover = 34,
    Flare = 35,
    StatusYou = 36,
    Lookout = 37,
    Timestamp = 38,
    ChannelMerchant = 39,
    ChannelGeneral = 40,
    ChannelHelp = 41,
    ChannelRealm = 42,
    ChannelOoc = 43,
    ChannelSociety = 44,
    ChannelHouse = 45,
    Health = 46,
    Mana = 47,
    Stamina = 48,
    Spirit = 49,
    Blood = 50,
    Stance = 51,
    Encumbrance = 52,
    Mind = 53,
    Level = 54,
    Unhurt = 55,
    Wound1 = 56,
    Wound2 = 57,
    Wound3 = 58,
    Scar1 = 59,
    Scar2 = 60,
    Scar3 = 61,
    Body = 62,
    Stunned = 63,
    Bleeding = 64,
    Poisoned = 65,
    Hidden = 66,
    Silenced = 67,
    Posture = 68,
    ActiveSpells = 69,
    Buffs = 70,
    Debuffs = 71,
    Cooldowns = 72,
    Pulse = 73,
    MapBackground = 74,
    MapRoom = 75,
    MapRoomEdge = 76,
    MapLine = 77,
    MapConnector = 78,
    MapDoor = 79,
    MapYou = 80,
    MapMuted = 81,
    MapRoute = 82,
    MarkBank = 83,
    MarkFurrier = 84,
    MarkGemshop = 85,
    MarkPawnshop = 86,
    MarkGuild = 87,
    MarkLocksmith = 88,
    MarkHealer = 89,
    MarkHerbalist = 90,
    MarkAlchemist = 91,
    Veil = 92,
    Grid = 93,
    Mark = 94,
}

/// How many tokens the binary has.
pub const TOKEN_COUNT: usize = 95;

/// `(index, token, name, default dark-theme colour, role)`.
///
/// This is the single table behind [`Token::name`], [`Token::bare`],
/// [`Token::role`] and [`Token::ALL`]; every row is transcribed from the
/// binary's data section.
pub const TABLE: [(u8, Token, &str, Rgb, Role); TOKEN_COUNT] = [
    (0, Token::Canvas, "canvas", rgb(0x1b, 0x1b, 0x1b), Role::Lift(0.0)),
    (1, Token::Surface, "surface", rgb(0x3c, 0x3c, 0x3c), Role::Lift(0.05)),
    (2, Token::Raised, "raised", rgb(0x46, 0x46, 0x46), Role::Lift(0.09)),
    (3, Token::Inset, "inset", rgb(0x0a, 0x0a, 0x0a), Role::Lift(-0.02)),
    (4, Token::Line, "line", rgb(0x3c, 0x3c, 0x3c), Role::Lift(0.14)),
    (5, Token::LineStrong, "line_strong", rgb(0x96, 0x96, 0x96), Role::Under(0, -0.3)),
    (6, Token::Text, "text", rgb(0xb4, 0xb4, 0xb4), Role::Lightness(0.75)),
    (7, Token::Muted, "muted", rgb(0x8c, 0x8c, 0x8c), Role::Lightness(0.55)),
    (8, Token::Selection, "selection", rgb(0x00, 0x5c, 0x80), Role::Under(0, -0.2)),
    (9, Token::Tab, "tab", rgb(0x3c, 0x3c, 0x3c), Role::Lift(0.05)),
    (10, Token::TabShown, "tab_shown", rgb(0x00, 0x5c, 0x80), Role::Under(0, -0.2)),
    (11, Token::Accent, "accent", rgb(0xd7, 0xad, 0x63), Role::Under(0, 0.0)),
    (12, Token::Link, "link", rgb(0x47, 0x7a, 0xb3), Role::Under(0, -0.06)),
    (13, Token::Hydra, "hydra", rgb(0x8f, 0xc9, 0xa8), Role::Under(1, 0.06)),
    (14, Token::Warning, "warning", rgb(0xd7, 0xad, 0x63), Role::Hue(0.0)),
    (15, Token::Wrong, "wrong", rgb(0xf0, 0x96, 0x8c), Role::Hue(0.06)),
    (16, Token::RoomName, "room_name", rgb(0xd7, 0xad, 0x63), Role::Under(4, -0.04)),
    (17, Token::RoomPlate, "room_plate", rgb(0x11, 0x16, 0x1b), Role::Fixed),
    (18, Token::Creature, "creature", rgb(0xbd, 0x8c, 0xff), Role::Under(3, 0.02)),
    (19, Token::Player, "player", rgb(0x8c, 0xa8, 0xff), Role::Under(2, -0.04)),
    (20, Token::Object, "object", rgb(0xb7, 0xbd, 0xc3), Role::Under(4, 0.08)),
    (21, Token::Container, "container", rgb(0x6c, 0xa6, 0xe0), Role::Hue(0.0)),
    (22, Token::Shut, "shut", rgb(0xe0, 0x8c, 0x3c), Role::Hue(0.0)),
    (23, Token::Speech, "speech", rgb(0xf0, 0xee, 0xe8), Role::Under(1, 0.04)),
    (24, Token::Whisper, "whisper", rgb(0xa9, 0xbb, 0xf5), Role::Under(1, -0.08)),
    (25, Token::Thought, "thought", rgb(0xa9, 0xbb, 0xf5), Role::Under(2, 0.06)),
    (26, Token::RoomDesc, "room_desc", rgb(0xb4, 0xb4, 0xb4), Role::Lightness(0.7)),
    (27, Token::Watching, "watching", rgb(0xb4, 0xb4, 0xb4), Role::Under(2, -0.06)),
    (28, Token::Command, "command", rgb(0x47, 0x7a, 0xb3), Role::Under(0, -0.1)),
    (29, Token::SelectedLink, "selected_link", rgb(0x7a, 0xa8, 0xe0), Role::Under(0, 0.08)),
    (30, Token::CreatureDeath, "creature_death", rgb(0x9a, 0x86, 0xb8), Role::Hue(-0.1)),
    (31, Token::FatalCrit, "fatal_crit", rgb(0xb4, 0xb4, 0xb4), Role::Hue(0.0)),
    (32, Token::CreaturePrep, "creature_prep", rgb(0xe8, 0xa2, 0x4e), Role::Hue(0.0)),
    (33, Token::CreatureTrigger, "creature_trigger", rgb(0xf0, 0x6e, 0x50), Role::Hue(0.04)),
    (34, Token::CreatureRecover, "creature_recover", rgb(0x6c, 0xc4, 0xb4), Role::Hue(0.0)),
    (35, Token::Flare, "flare", rgb(0xf2, 0xd2, 0x5a), Role::Hue(0.0)),
    (36, Token::StatusYou, "status_you", rgb(0xe0, 0x6c, 0xd0), Role::Hue(0.0)),
    (37, Token::Lookout, "lookout", rgb(0x5c, 0xe0, 0xe8), Role::Hue(0.06)),
    (38, Token::Timestamp, "timestamp", rgb(0x8c, 0x8c, 0x8c), Role::Lightness(0.55)),
    (39, Token::ChannelMerchant, "channel_merchant", rgb(0xe0, 0xb0, 0x5c), Role::Under(1, 0.02)),
    (40, Token::ChannelGeneral, "channel_general", rgb(0x8c, 0xc8, 0xe8), Role::Under(0, 0.04)),
    (41, Token::ChannelHelp, "channel_help", rgb(0x7c, 0xd0, 0x8c), Role::Under(3, 0.04)),
    (42, Token::ChannelRealm, "channel_realm", rgb(0xc8, 0x96, 0xe8), Role::Under(4, 0.0)),
    (43, Token::ChannelOoc, "channel_ooc", rgb(0xe8, 0x96, 0x8c), Role::Under(2, 0.02)),
    (44, Token::ChannelSociety, "channel_society", rgb(0x6c, 0xc4, 0xc0), Role::Under(5, -0.02)),
    (45, Token::ChannelHouse, "channel_house", rgb(0xd8, 0xa8, 0xc8), Role::Under(1, -0.08)),
    (46, Token::Health, "health", rgb(0xcd, 0x4d, 0x4d), Role::Hue(0.0)),
    (47, Token::Mana, "mana", rgb(0x47, 0x84, 0xd9), Role::Hue(0.0)),
    (48, Token::Stamina, "stamina", rgb(0x55, 0xb8, 0x6c), Role::Hue(0.0)),
    (49, Token::Spirit, "spirit", rgb(0xcb, 0xa9, 0x42), Role::Hue(0.0)),
    (50, Token::Blood, "blood", rgb(0x8b, 0x1a, 0x1a), Role::Hue(-0.15)),
    (51, Token::Stance, "stance", rgb(0x4f, 0xa3, 0xa5), Role::Hue(0.0)),
    (52, Token::Encumbrance, "encumbrance", rgb(0xb0, 0x7a, 0x3c), Role::Hue(0.0)),
    (53, Token::Mind, "mind", rgb(0x8e, 0x6b, 0xc9), Role::Hue(0.0)),
    (54, Token::Level, "level", rgb(0xc9, 0xa2, 0x3c), Role::Hue(0.0)),
    (55, Token::Unhurt, "unhurt", rgb(0x33, 0x33, 0x33), Role::Hue(-0.3)),
    (56, Token::Wound1, "wound1", rgb(0xaa, 0x55, 0x00), Role::Hue(-0.08)),
    (57, Token::Wound2, "wound2", rgb(0xff, 0x88, 0x00), Role::Hue(0.0)),
    (58, Token::Wound3, "wound3", rgb(0xff, 0x00, 0x00), Role::Hue(0.0)),
    (59, Token::Scar1, "scar1", rgb(0x99, 0x99, 0x99), Role::Hue(0.12)),
    (60, Token::Scar2, "scar2", rgb(0x77, 0x77, 0x77), Role::Hue(0.0)),
    (61, Token::Scar3, "scar3", rgb(0x55, 0x55, 0x55), Role::Hue(-0.12)),
    (62, Token::Body, "body", rgb(0x4a, 0x4a, 0x50), Role::Lift(0.18)),
    (63, Token::Stunned, "stunned", rgb(0xd8, 0xb4, 0x3a), Role::Hue(0.0)),
    (64, Token::Bleeding, "bleeding", rgb(0xcd, 0x4d, 0x4d), Role::Hue(-0.06)),
    (65, Token::Poisoned, "poisoned", rgb(0x6d, 0xa8, 0x3c), Role::Hue(0.0)),
    (66, Token::Hidden, "hidden", rgb(0x7a, 0x86, 0xa8), Role::Hue(0.0)),
    (67, Token::Silenced, "silenced", rgb(0x8e, 0x6b, 0xc9), Role::Hue(0.0)),
    (68, Token::Posture, "posture", rgb(0x4f, 0xa3, 0xa5), Role::Hue(0.0)),
    (69, Token::ActiveSpells, "active_spells", rgb(0x47, 0x84, 0xd9), Role::Hue(0.0)),
    (70, Token::Buffs, "buffs", rgb(0x55, 0xb8, 0x6c), Role::Hue(0.04)),
    (71, Token::Debuffs, "debuffs", rgb(0xcd, 0x4d, 0x4d), Role::Hue(-0.06)),
    (72, Token::Cooldowns, "cooldowns", rgb(0x7a, 0x86, 0xa8), Role::Hue(0.0)),
    (73, Token::Pulse, "pulse", rgb(0x47, 0x84, 0xd9), Role::Hue(0.06)),
    (74, Token::MapBackground, "map_background", rgb(0x11, 0x16, 0x1b), Role::Lift(0.02)),
    (75, Token::MapRoom, "map_room", rgb(0x49, 0x7f, 0xa3), Role::Under(0, -0.1)),
    (76, Token::MapRoomEdge, "map_room_edge", rgb(0xa2, 0xc8, 0xdf), Role::Under(0, 0.15)),
    (77, Token::MapLine, "map_line", rgb(0x6e, 0x99, 0xb5), Role::Under(0, -0.1)),
    (78, Token::MapConnector, "map_connector", rgb(0x38, 0x51, 0x64), Role::Under(0, -0.25)),
    (79, Token::MapDoor, "map_door", rgb(0xff, 0xc7, 0x78), Role::Hue(0.0)),
    (80, Token::MapYou, "map_you", rgb(0xdd, 0xdc, 0xd7), Role::Lightness(0.75)),
    (81, Token::MapMuted, "map_muted", rgb(0x8e, 0x9f, 0xad), Role::Lightness(0.5)),
    (82, Token::MapRoute, "map_route", rgb(0x57, 0xf3, 0xcb), Role::Hue(0.0)),
    (83, Token::MarkBank, "mark_bank", rgb(0xe5, 0xbe, 0x52), Role::Hue(0.0)),
    (84, Token::MarkFurrier, "mark_furrier", rgb(0xcf, 0xaa, 0x80), Role::Hue(0.0)),
    (85, Token::MarkGemshop, "mark_gemshop", rgb(0x64, 0xda, 0xfa), Role::Hue(0.0)),
    (86, Token::MarkPawnshop, "mark_pawnshop", rgb(0xf3, 0xa2, 0x5c), Role::Hue(0.0)),
    (87, Token::MarkGuild, "mark_guild", rgb(0x82, 0x9f, 0xff), Role::Hue(0.0)),
    (88, Token::MarkLocksmith, "mark_locksmith", rgb(0xc4, 0xa0, 0xef), Role::Hue(0.0)),
    (89, Token::MarkHealer, "mark_healer", rgb(0xf3, 0x8d, 0x99), Role::Hue(0.0)),
    (90, Token::MarkHerbalist, "mark_herbalist", rgb(0x91, 0xd5, 0x78), Role::Hue(0.0)),
    (91, Token::MarkAlchemist, "mark_alchemist", rgb(0x60, 0xd3, 0xbd), Role::Hue(0.0)),
    (92, Token::Veil, "veil", rgb(0x00, 0x00, 0x00), Role::Fixed),
    (93, Token::Grid, "grid", rgb(0xff, 0xff, 0xff), Role::Fixed),
    (94, Token::Mark, "mark", rgb(0xff, 0xff, 0x00), Role::Fixed),
];

impl Token {
    /// Every token, in discriminant order.
    pub const ALL: [Token; TOKEN_COUNT] = {
        let mut out = [Token::Canvas; TOKEN_COUNT];
        let mut i = 0;
        while i < TOKEN_COUNT {
            out[i] = TABLE[i].1;
            i += 1;
        }
        out
    };

    /// The token for a discriminant byte, if it is in range.
    pub fn from_u8(v: u8) -> Option<Token> {
        if (v as usize) < TOKEN_COUNT {
            Some(TABLE[v as usize].1)
        } else {
            None
        }
    }

    /// The discriminant, exactly what the binary's tables are indexed by.
    pub fn index(self) -> u8 {
        self as u8
    }

    /// `@0x101842e08` — the long name, e.g. `"channel_merchant"`.
    pub fn name(self) -> &'static str {
        TABLE[self as usize].2
    }

    /// The inverse of [`Token::name`].
    pub fn named(name: &str) -> Option<Token> {
        TABLE.iter().find(|r| r.2 == name).map(|r| r.1)
    }

    /// `@0x101842dd0` — the built-in dark-theme colour for this token.
    pub fn bare(self) -> Rgb {
        TABLE[self as usize].3
    }

    /// `@0x101842e28` — how the harmony generator should derive this token.
    pub fn role(self) -> Role {
        TABLE[self as usize].4
    }
}

/// The prompt-accent table in the shape the binary stores it: key -> value.
pub type PromptMap = BTreeMap<u32, u8>;

/// A resolved set of colours.
///
/// Recovered layout (`Palette::bare` @0x101847978 copies exactly 304 bytes):
/// `names` at `+0x00`, `prompt` at `+0x08`, the 95 token colours at `+0x10`
/// (three bytes each: `r`, `g`, `b`).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// Colour overrides by token name, from a theme file's `[names]` table.
    pub names: Arc<BTreeMap<String, Rgb>>,
    /// The prompt accents.  `// UNKNOWN:` the binary stores `u32 -> u8` here and
    /// `Palette::prompt` reads them back with an eight-byte load, so the value
    /// width is ambiguous; `u8` is what `Palette::bare` writes.
    pub prompt: Arc<PromptMap>,
    /// The resolved colour for every token.
    pub bare: [Rgb; TOKEN_COUNT],
}

/// The four prompt accents `Palette::bare` installs.
///
/// `@0x101847ae4`..`@0x101847b1c` writes the pairs `(82, 15)`, `(33, 15)`,
/// `(83, 14)` and `(72, 12)` — i.e. `map_route`, `creature_trigger`,
/// `mark_bank` and `cooldowns`.
pub const PROMPT_DEFAULTS: [(u32, u8); 4] = [(82, 15), (33, 15), (83, 14), (72, 12)];

impl Default for Palette {
    fn default() -> Self {
        Palette::bare()
    }
}

impl Palette {
    /// `@0x101847978` — the built-in palette: no name overrides, the four
    /// default prompt accents and `Token::bare` for every slot.
    pub fn bare() -> Palette {
        let mut bare = [0u32; TOKEN_COUNT];
        for (i, row) in TABLE.iter().enumerate() {
            bare[i] = row.3;
        }
        let prompt: PromptMap = PROMPT_DEFAULTS.iter().copied().collect();
        Palette {
            names: Arc::new(BTreeMap::new()),
            prompt: Arc::new(prompt),
            bare,
        }
    }

    /// `@0x1018478b8` — the colour of a token.
    pub fn get(&self, token: Token) -> Rgb {
        self.bare[token as usize]
    }

    /// `@0x101847bc0` — a named override, as `(r, g, b)`, if the theme set one.
    pub fn named(&self, name: &str) -> Option<(u8, u8, u8)> {
        self.names
            .get(name)
            .map(|c| (red(*c), green(*c), blue(*c)))
    }

    /// `@0x101847d2c` — the `name -> colour` map a theme starts from.
    ///
    /// The binary folds the token table into a `BTreeMap<String, Rgb>`; the
    /// recovered result is exactly `Token::name -> Token::bare`.
    pub fn by_name(&self) -> BTreeMap<String, Rgb> {
        TABLE.iter().map(|r| (r.2.to_string(), r.3)).collect()
    }

    /// `@0x101847d84` — install the `name -> colour` overrides.
    pub fn set_names(&mut self, names: BTreeMap<String, Rgb>) {
        self.names = Arc::new(names);
    }

    /// `@0x101847cb4` — the prompt accent registered for `at`, defaulting to
    /// `2` when the map has no entry (that is the binary's `mov w0, #2`).
    pub fn prompt(&self, at: u32) -> u8 {
        self.prompt.get(&at).copied().unwrap_or(2)
    }

    /// `@0x1018477fc` — install the prompt accents.
    pub fn set_prompt(&mut self, prompt: PromptMap) {
        self.prompt = Arc::new(prompt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_table_is_consistent() {
        assert_eq!(TABLE.len(), TOKEN_COUNT);
        for (i, row) in TABLE.iter().enumerate() {
            assert_eq!(row.0 as usize, i, "row {i} has the wrong discriminant");
            assert_eq!(row.1.index() as usize, i);
            assert_eq!(row.1.name(), row.2);
            assert_eq!(row.1.bare(), row.3);
        }
        assert_eq!(Token::ALL.len(), TOKEN_COUNT);
        assert_eq!(Token::ALL[0], Token::Canvas);
        assert_eq!(Token::ALL[94], Token::Mark);
    }

    #[test]
    fn naming_round_trips() {
        assert_eq!(Token::from_u8(0), Some(Token::Canvas));
        assert_eq!(Token::from_u8(94), Some(Token::Mark));
        assert_eq!(Token::from_u8(95), None);
        assert_eq!(Token::named("channel_merchant"), Some(Token::ChannelMerchant));
        assert_eq!(Token::named("nope"), None);
        let mut names: Vec<&str> = TABLE.iter().map(|r| r.2).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "token names must be unique");
    }

    #[test]
    fn recovered_token_colours() {
        // The dark theme's surface stack.
        assert_eq!(hex(Token::Canvas.bare()), "#1b1b1b");
        assert_eq!(hex(Token::Surface.bare()), "#3c3c3c");
        assert_eq!(hex(Token::Raised.bare()), "#464646");
        assert_eq!(hex(Token::Inset.bare()), "#0a0a0a");
        assert_eq!(hex(Token::Text.bare()), "#b4b4b4");
        assert_eq!(hex(Token::Muted.bare()), "#8c8c8c");
        // Vitals keep their conventional colours.
        assert_eq!(hex(Token::Health.bare()), "#cd4d4d");
        assert_eq!(hex(Token::Mana.bare()), "#4784d9");
        assert_eq!(hex(Token::Stamina.bare()), "#55b86c");
        assert_eq!(hex(Token::Spirit.bare()), "#cba942");
        // Wounds go orange -> red; scars step down in grey.
        assert_eq!(hex(Token::Wound1.bare()), "#aa5500");
        assert_eq!(hex(Token::Wound2.bare()), "#ff8800");
        assert_eq!(hex(Token::Wound3.bare()), "#ff0000");
        assert_eq!(hex(Token::Scar1.bare()), "#999999");
        assert_eq!(hex(Token::Scar2.bare()), "#777777");
        assert_eq!(hex(Token::Scar3.bare()), "#555555");
        // The map overlays.
        assert_eq!(hex(Token::Veil.bare()), "#000000");
        assert_eq!(hex(Token::Grid.bare()), "#ffffff");
        assert_eq!(hex(Token::Mark.bare()), "#ffff00");
        assert_eq!(hex(Token::Blood.bare()), "#8b1a1a");
    }

    #[test]
    fn recovered_roles() {
        assert_eq!(Token::Surface.role(), Role::Lift(0.05));
        assert_eq!(Token::Raised.role(), Role::Lift(0.09));
        assert_eq!(Token::Text.role(), Role::Lightness(0.75));
        assert_eq!(Token::Timestamp.role(), Role::Lightness(0.55));
        assert_eq!(Token::LineStrong.role(), Role::Under(0, -0.3));
        assert_eq!(Token::Hydra.role(), Role::Under(1, 0.06));
        assert_eq!(Token::ChannelSociety.role(), Role::Under(5, -0.02));
        assert_eq!(Token::Wrong.role(), Role::Hue(0.06));
        assert_eq!(Token::Blood.role(), Role::Hue(-0.15));
        assert_eq!(Token::Veil.role(), Role::Fixed);
        assert_eq!(Token::Grid.role(), Role::Fixed);
        for row in TABLE {
            if let Role::Under(base, _) = row.4 {
                assert!(base <= 5, "{} names surface {base}", row.2);
            }
        }
    }

    #[test]
    fn hex_packs_low_byte_red() {
        assert_eq!(hex(rgb(0x63, 0xad, 0xd7)), "#63add7");
        assert_eq!(rgb(0x63, 0xad, 0xd7), 0x63 | 0xad << 8 | 0xd7 << 16);
        assert_eq!(red(rgb(1, 2, 3)), 1);
        assert_eq!(green(rgb(1, 2, 3)), 2);
        assert_eq!(blue(rgb(1, 2, 3)), 3);
    }

    #[test]
    fn parse_hex_forms() {
        assert_eq!(parse_hex("#63add7"), Some(rgb(0x63, 0xad, 0xd7)));
        assert_eq!(parse_hex("  #63ADd7 "), Some(rgb(0x63, 0xad, 0xd7)));
        assert_eq!(parse_hex("#abc"), Some(rgb(0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_hex("#63add780"), Some(rgb(0x63, 0xad, 0xd7)));
        assert_eq!(parse_hex("63add7"), None);
        assert_eq!(parse_hex("#"), None);
        assert_eq!(parse_hex("#63add"), None);
        assert_eq!(parse_hex("#63addg"), None);
        assert_eq!(parse_hex("#63add7a"), None);
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    fn palette_defaults() {
        let p = Palette::bare();
        assert!(p.names.is_empty());
        assert_eq!(p.get(Token::Canvas), Token::Canvas.bare());
        assert_eq!(p.get(Token::Mark), 0x00ffff);
        assert_eq!(p.bare.len(), TOKEN_COUNT);
        assert_eq!(p.prompt(82), 15);
        assert_eq!(p.prompt(33), 15);
        assert_eq!(p.prompt(83), 14);
        assert_eq!(p.prompt(72), 12);
        assert_eq!(p.prompt(0), 2);
        assert_eq!(p.prompt(94), 2);
    }

    #[test]
    fn palette_overrides() {
        let mut p = Palette::bare();
        p.set_names(p.by_name());
        assert_eq!(p.by_name().len(), TOKEN_COUNT);
        assert_eq!(p.named("canvas"), Some((0x1b, 0x1b, 0x1b)));
        assert_eq!(p.named("not_a_token"), None);

        let mut names = BTreeMap::new();
        names.insert("text".to_string(), rgb(1, 2, 3));
        p.set_names(names);
        assert_eq!(p.named("text"), Some((1, 2, 3)));
        assert_eq!(p.get(Token::Text), Token::Text.bare());

        let mut prompts = PromptMap::new();
        prompts.insert(0u32, 9u8);
        p.set_prompt(prompts);
        assert_eq!(p.prompt(0), 9);
        assert_eq!(p.prompt(82), 2);
    }
}
