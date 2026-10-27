//! The window title bar: which buttons it starts with, and what each one says.
//!
//! Recovered from `hydra_gui::play::titlebar` (unslid VAs):
//!
//! | symbol | addr | status |
//! |---|---|---|
//! | `left_at_first` | `0x1003d97c0` | **full** |
//! | `right_at_first` | `0x1003d981c` | **full** |
//! | `TitleBar as Default` | `0x1003ccb64` | **full** |
//! | `TitleBar as From<Written>` | `0x1003c6b98` | **full** |
//! | `Item::label` | `0x1003da1ac` | **full** |
//! | `Item::key` | `0x1003d9d84` | full (names) |
//! | `Item::of_key` | `0x1003da5f4` | not reconstructed |
//! | `TitleBar::set` | `0x1003da938` | not reconstructed |
//! | `TitleBar::page` | `0x1003db164` | not reconstructed |
//!
//! The original was `egui`-based (`TitleBar::set`/`page` paint through
//! `egui::Context`); the recovered layout/state lives here and painting is a
//! documented seam.

use crate::json::{self, Value};

/// One title-bar entry.
///
/// The discriminant encoding is the `String` capacity niche: `Item::label`
/// @`0x1003da1c0` does `eor x9, x8, #0x8000000000000000` and uses it as the jump
/// index when `x8 < 0`, i.e. unit variant `k` is stored as `isize::MIN + k`,
/// and the `String`-carrying variant is index 18.
///
/// Labels and names are recovered from the jump table at `0x1020740af` (19
/// entries, base `0x1003da1e8`) with the literals in `Item::label`
/// (`0x1003da1ac`) and the keys copied in `Item::key` (`0x1003d9d84`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Item {
    #[default]
    Character,
    Profession,
    Connection,
    RoomTitle,
    RoomNumber,
    Lock,
    Arrange,
    DrawersMenu,
    LayoutMenu,
    KeysMenu,
    Log,
    Menu,
    Settings,
    Zoom,
    TopDrawer,
    BottomDrawer,
    LeftDrawer,
    RightDrawer,
    /// The only variant that carries data; its label/key are the string itself.
    Text(String),
}

impl Item {
    /// The 18 unit variants in discriminant order.
    pub const EVERY: [Item; 18] = [
        Item::Character,
        Item::Profession,
        Item::Connection,
        Item::RoomTitle,
        Item::RoomNumber,
        Item::Lock,
        Item::Arrange,
        Item::DrawersMenu,
        Item::LayoutMenu,
        Item::KeysMenu,
        Item::Log,
        Item::Menu,
        Item::Settings,
        Item::Zoom,
        Item::TopDrawer,
        Item::BottomDrawer,
        Item::LeftDrawer,
        Item::RightDrawer,
    ];

    /// What the button reads.
    ///
    /// `Item::label` @`0x1003da1ac` returns `String`s built from the literals at
    /// `0x102074b65` ("Character"), `0x102074b6e` ("Profession"),
    /// `0x102074b78` ("Connection"), `0x102074b82` ("Room title"),
    /// `0x102074b8c` ("Room number"), "Lock", "Arrange",
    /// `0x102074b9e` ("Drawers menu"), "Layout menu", `0x102074bb5`
    /// ("Keys menu"), "Log", "Menu", "Settings",
    /// `0x102074bc1` ("Zoom − and +"), `0x102074bcf` ("Top drawer icon"),
    /// `0x102074bde` ("Bottom drawer icon"), `0x102073eb0`
    /// ("Left drawer icon"), `0x102074bf0` ("Right drawer icon").
    pub fn label(&self) -> &str {
        match self {
            Item::Character => "Character",
            Item::Profession => "Profession",
            Item::Connection => "Connection",
            Item::RoomTitle => "Room title",
            Item::RoomNumber => "Room number",
            Item::Lock => "Lock",
            Item::Arrange => "Arrange",
            Item::DrawersMenu => "Drawers menu",
            Item::LayoutMenu => "Layout menu",
            Item::KeysMenu => "Keys menu",
            Item::Log => "Log",
            Item::Menu => "Menu",
            Item::Settings => "Settings",
            Item::Zoom => "Zoom \u{2212} and +",
            Item::TopDrawer => "Top drawer icon",
            Item::BottomDrawer => "Bottom drawer icon",
            Item::LeftDrawer => "Left drawer icon",
            Item::RightDrawer => "Right drawer icon",
            Item::Text(text) => text,
        }
    }

    /// The stable identifier written to the layout file.
    ///
    /// `Item::key` @`0x1003d9d84` copies `character`, `left_drawer`, `top_drawer`,
    /// `connection`, `bottom_drawer`, `profession`, `right_drawer`, … (the
    /// literals at `0x102144...`).
    pub fn key(&self) -> &str {
        match self {
            Item::Character => "character",
            Item::Profession => "profession",
            Item::Connection => "connection",
            Item::RoomTitle => "room_title",
            Item::RoomNumber => "room_number",
            Item::Lock => "lock",
            Item::Arrange => "arrange",
            Item::DrawersMenu => "drawers_menu",
            Item::LayoutMenu => "layout_menu",
            Item::KeysMenu => "keys_menu",
            Item::Log => "log",
            Item::Menu => "menu",
            Item::Settings => "settings",
            Item::Zoom => "zoom",
            Item::TopDrawer => "top_drawer",
            Item::BottomDrawer => "bottom_drawer",
            Item::LeftDrawer => "left_drawer",
            Item::RightDrawer => "right_drawer",
            Item::Text(text) => text,
        }
    }

    /// The item named by `key` — the inverse of [`Item::key`].
    pub fn of_key(key: &str) -> Option<Item> {
        Item::EVERY.into_iter().find(|item| item.key() == key)
    }

    pub fn to_json(&self) -> Value {
        Value::Str(self.key().to_string())
    }

    pub fn from_json(v: &Value) -> Option<Item> {
        let key = v.as_str()?;
        // Unknown keys survive as free text, which is what `Text` is for.
        Some(Item::of_key(key).unwrap_or_else(|| Item::Text(key.to_string())))
    }
}

/// The title bar as written in the layout file.
///
/// `From<TitleBar> for Written` has the same 0x50-byte layout as
/// [`TitleBar`], with only one flag byte (`0x48`) — the `From<Written>` impl at
/// `0x1003c6b98` copies the three `Vec<Item>`s out of it and forces the second
/// flag to `true` (`mov w9, #0x1; strb w9, [sp, #0x49]` @`0x1003c6bf8`).
///
/// `left` and `right` have `#[serde(default = "left_at_first")]` /
/// `#[serde(default = "right_at_first")]` — the two functions are the only
/// callers of the `Deserialize` machinery at `0x100462f94`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub left: Vec<Item>,
    pub middle: Vec<Item>,
    pub right: Vec<Item>,
    /// The byte at `0x48`; `false` means "insert the `Menu` item if missing".
    // UNKNOWN: the flag's name; only its effect in `From<Written>` is recovered.
    pub tucked: bool,
}

impl Default for Written {
    /// `left` and `right` carry `#[serde(default = ..)]`, so a file that omits
    /// them still gets the first-paint entries.
    fn default() -> Self {
        Written {
            left: left_at_first(),
            middle: Vec::new(),
            right: right_at_first(),
            tucked: false,
        }
    }
}

impl Written {
    pub fn to_json(&self) -> Value {
        json::obj([
            (
                "left",
                Value::Arr(self.left.iter().map(Item::to_json).collect()),
            ),
            (
                "middle",
                Value::Arr(self.middle.iter().map(Item::to_json).collect()),
            ),
            (
                "right",
                Value::Arr(self.right.iter().map(Item::to_json).collect()),
            ),
            ("tucked", Value::Bool(self.tucked)),
        ])
    }

    pub fn from_json(v: &Value) -> Written {
        fn items(v: Option<&Value>) -> Option<Vec<Item>> {
            let arr = v?.as_arr()?;
            Some(arr.iter().filter_map(Item::from_json).collect())
        }
        Written {
            left: items(v.get("left")).unwrap_or_else(left_at_first),
            middle: items(v.get("middle")).unwrap_or_default(),
            right: items(v.get("right")).unwrap_or_else(right_at_first),
            tucked: v.get("tucked").and_then(Value::as_bool).unwrap_or(false),
        }
    }
}

/// The two flags of a title bar, at struct offsets `0x48` and `0x49`.
///
/// `Default` writes `strh w8, [x19, #0x48]` with `w8 = 0x100`, i.e. byte `0x48`
/// is `0` and byte `0x49` is `1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleBar {
    pub left: Vec<Item>,
    pub middle: Vec<Item>,
    pub right: Vec<Item>,
    pub tucked: bool,
    pub settled: bool,
}

/// The left-hand entries a title bar starts with.
///
/// `left_at_first` @`0x1003d97c0` allocates a two-entry `Vec<Item>`
/// (`cap = 2`, `len = 2`, 24 bytes per item) holding `Character`
/// (`isize::MIN + 0`, at `str x8 = -0x8000000000000000, [x0]`) followed by
/// `Connection` (`isize::MIN + 2`, at `str x8 = 0x8000000000000002, [x0, #0x18]`).
pub fn left_at_first() -> Vec<Item> {
    vec![Item::Character, Item::Connection]
}

/// The right-hand entries a title bar starts with.
///
/// `right_at_first` @`0x1003d981c` is the mirror image: a two-entry
/// `Vec<Item>` holding `Menu` (`isize::MIN + 0xb`) and `Zoom`
/// (`isize::MIN + 0xd`, `add x8, x8, #0x2`).
pub fn right_at_first() -> Vec<Item> {
    vec![Item::Menu, Item::Zoom]
}

impl Default for TitleBar {
    /// `TitleBar as Default` @`0x1003ccb64`: two entries on the left
    /// (`Character`, `Connection`), none in the middle, two on the right
    /// (`Menu`, `Zoom`), `tucked = false`, `settled = true`.
    fn default() -> Self {
        TitleBar {
            left: left_at_first(),
            middle: Vec::new(),
            right: right_at_first(),
            tucked: false,
            settled: true,
        }
    }
}

impl From<Written> for TitleBar {
    /// `From<Written> for TitleBar` @`0x1003c6b98`:
    ///
    /// * copy the three lists across and set the second flag to `true`;
    /// * if none of the three lists already contains [`Item::Menu`]
    ///   (`cmp x11, x21(=0x800000000000000b)` over all three, @`0x1003c6c18`)
    ///   and `tucked` is clear, insert [`Item::Menu`] into the right-hand list at
    ///   the first position whose item is not `Settings`
    ///   (`add x13, x21, #0x1` — `isize::MIN + 0xc` — @`0x1003c6c8c`).
    fn from(written: Written) -> TitleBar {
        let mut bar = TitleBar {
            left: written.left,
            middle: written.middle,
            right: written.right,
            tucked: written.tucked,
            settled: true,
        };
        let has_menu = bar
            .left
            .iter()
            .chain(bar.middle.iter())
            .chain(bar.right.iter())
            .any(|item| *item == Item::Menu);
        if !has_menu && !bar.tucked {
            // `cmp x12, x13(=Settings); b.eq` @0x1003c6c90 stops at the first
            // `Settings`, which is where the `Menu` goes.
            let at = bar
                .right
                .iter()
                .position(|item| *item == Item::Settings)
                .unwrap_or(bar.right.len());
            bar.right.insert(at, Item::Menu);
        }
        bar
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_paint_zones_are_the_recovered_ones() {
        assert_eq!(left_at_first(), vec![Item::Character, Item::Connection]);
        assert_eq!(right_at_first(), vec![Item::Menu, Item::Zoom]);
    }

    #[test]
    fn default_matches_the_recovered_allocations() {
        let bar = TitleBar::default();
        assert_eq!(bar.left.len(), 2);
        assert_eq!(bar.middle.len(), 0);
        assert_eq!(bar.right.len(), 2);
        assert!(!bar.tucked);
        assert!(bar.settled);
        assert_eq!(bar, TitleBar::from(Written::default()));
    }

    #[test]
    fn labels_and_keys_are_the_recovered_literals() {
        assert_eq!(Item::Character.label(), "Character");
        assert_eq!(Item::RoomNumber.label(), "Room number");
        assert_eq!(Item::Zoom.label(), "Zoom \u{2212} and +");
        assert_eq!(Item::TopDrawer.label(), "Top drawer icon");
        assert_eq!(Item::BottomDrawer.label(), "Bottom drawer icon");
        assert_eq!(Item::LeftDrawer.key(), "left_drawer");
        assert_eq!(Item::BottomDrawer.key(), "bottom_drawer");
        assert_eq!(Item::RightDrawer.key(), "right_drawer");
        assert_eq!(Item::TopDrawer.key(), "top_drawer");
        assert_eq!(Item::Character.key(), "character");
        assert_eq!(Item::Connection.key(), "connection");
        assert_eq!(Item::Profession.key(), "profession");
        assert_eq!(Item::Text("anything".into()).label(), "anything");
    }

    #[test]
    fn keys_round_trip_through_of_key() {
        for item in Item::EVERY {
            assert_eq!(Item::of_key(item.key()), Some(item.clone()));
        }
        assert_eq!(Item::of_key("nope"), None);
    }

    #[test]
    fn from_written_inserts_menu_before_the_first_settings() {
        let written = Written {
            left: vec![Item::Character],
            middle: Vec::new(),
            right: vec![Item::Settings, Item::Zoom],
            tucked: false,
        };
        let bar = TitleBar::from(written);
        assert_eq!(
            bar.right,
            vec![Item::Menu, Item::Settings, Item::Zoom],
            "Menu goes before the first Settings entry"
        );
        assert!(bar.settled);
    }

    #[test]
    fn from_written_keeps_an_existing_menu() {
        let written = Written {
            left: vec![Item::Menu],
            middle: Vec::new(),
            right: vec![Item::Settings],
            tucked: false,
        };
        let bar = TitleBar::from(written);
        assert_eq!(bar.right, vec![Item::Settings]);
    }

    #[test]
    fn from_written_respects_tucked() {
        let written = Written {
            left: vec![Item::Character],
            middle: Vec::new(),
            right: vec![Item::Settings],
            tucked: true,
        };
        let bar = TitleBar::from(written);
        assert_eq!(bar.right, vec![Item::Settings]);
    }

    #[test]
    fn from_written_appends_menu_when_everything_is_settings() {
        let written = Written {
            left: Vec::new(),
            middle: Vec::new(),
            right: vec![Item::Settings],
            tucked: false,
        };
        assert_eq!(
            TitleBar::from(written).right,
            vec![Item::Menu, Item::Settings]
        );
    }

    #[test]
    fn written_json_round_trips_and_defaults_the_lists() {
        let written = Written::from_json(
            &json::parse(r#"{"middle":["zoom"],"tucked":true}"#).unwrap(),
        );
        assert_eq!(written.left, left_at_first());
        assert_eq!(written.right, right_at_first());
        assert_eq!(written.middle, vec![Item::Zoom]);
        assert!(written.tucked);

        let text = written.to_json().write();
        assert_eq!(
            text,
            r#"{"left":["character","connection"],"middle":["zoom"],"right":["menu","zoom"],"tucked":true}"#
        );
        assert_eq!(Written::from_json(&json::parse(&text).unwrap()), written);
    }

    #[test]
    fn unknown_keys_survive_as_text() {
        let written = Written::from_json(&json::parse(r#"{"left":["custom"]}"#).unwrap());
        assert_eq!(written.left, vec![Item::Text("custom".into())]);
        assert_eq!(written.to_json().write(), r#"{"left":["custom"],"middle":[],"right":["menu","zoom"],"tucked":false}"#);
    }
}
