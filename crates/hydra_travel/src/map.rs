//! A minimal room index.
//!
//! **`std`-only / scope departure.** The original crate resolved rooms through
//! the external `cena_map` crate:
//!
//! * `cena_map::map::Map::room(&self, id) -> Option<&Room>` — called as a tail
//!   call from `walk::routines::solver_for` (`0x10156b7d8`);
//! * `cena_map::locate::title_from_subtitle` — called from
//!   `hydra_map::follow::room_of` (`0x101671368`), which is what
//!   [`crate::route::here`] is a 4-byte tail call to (`0x1015b3304`);
//! * `cena_map::exit::Cost::price` — called from `hydra_travel::route::why_shut`
//!   (`0x10166cb88`);
//! * `cena_map::cond::Cond::ask` — called from `hydra_travel::route::why_shut`
//!   and `route::not_known` (`0x10165eb24`).
//!
//! `cena_map` is not part of this workspace and `hydra_map` is still a stub, so
//! `hydra_travel` carries the little room index it actually uses here. Nothing
//! about the *behaviour* of the travel code depends on more than: a room's
//! number, its title, its area, its exits (direction word + destination room)
//! and the walking cost of an exit.

use std::collections::BTreeMap;

use crate::RoomId;

/// One exit from a room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exit {
    /// The direction word the game answers to, e.g. `"north"`, `"go gate"`.
    pub word: String,
    pub to: RoomId,
    /// `cena_map::exit::Cost::price` — the coin an exit charges, if any.
    pub price: Option<u32>,
    /// The condition the map attaches to the exit (`cena_map::cond::Cond`).
    pub when: Option<String>,
}

/// One map room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Room {
    pub id: RoomId,
    /// The game's room number when it is known (`"10862"`), else empty.
    pub number: String,
    pub title: String,
    pub area: String,
    pub exits: Vec<Exit>,
    /// `cena_map::routine::Routine` payloads attached to the room; the travel
    /// solvers are built from these (see [`crate::walk::routines`]).
    pub routines: Vec<crate::walk::routines::Routine>,
}

impl Room {
    pub fn exit(&self, word: &str) -> Option<&Exit> {
        self.exits.iter().find(|e| e.word == word)
    }
}

/// The room index.
#[derive(Clone, Debug, Default)]
pub struct Map {
    rooms: Vec<Room>,
    by_title: BTreeMap<String, RoomId>,
}

impl Map {
    pub fn new(rooms: Vec<Room>) -> Map {
        let mut by_title = BTreeMap::new();
        for room in &rooms {
            by_title
                .entry(room.title.trim().to_ascii_lowercase())
                .or_insert(room.id);
        }
        Map { rooms, by_title }
    }

    /// `cena_map::map::Map::room`.
    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.rooms.iter().find(|r| r.id == id)
    }

    pub fn rooms(&self) -> &[Room] {
        &self.rooms
    }

    /// The room whose title matches `title` (case-insensitively, trimmed).
    pub fn room_id(&self, title: &str) -> Option<RoomId> {
        self.by_title
            .get(&title.trim().to_ascii_lowercase())
            .copied()
    }

    /// Every room title, in room order.
    pub fn titles(&self) -> Vec<&str> {
        self.rooms.iter().map(|r| r.title.as_str()).collect()
    }

    /// The rooms of one area.
    pub fn area(&self, area: &str) -> Vec<&Room> {
        let area = area.trim().to_ascii_lowercase();
        self.rooms
            .iter()
            .filter(|r| r.area.trim().to_ascii_lowercase() == area)
            .collect()
    }
}

/// Which room the character is standing in.
///
/// **ESTABLISHED (shape) / INFERRED (body).** `route::here` is a 4-byte tail
/// call to `hydra_map::follow::room_of` (`0x1015b3304`), which:
///
/// 1. reads the room's map component (a component name of length 10 is looked
///    up on `Room::component`, `0x1015b3390`),
/// 2. falls back to `cena_map::locate::title_from_subtitle` on the room title
///    (`0x1015b337c`) when the component is missing,
/// 3. compares the resulting title/subtitle pair against the map and returns a
///    niche-encoded `Option<RoomId>`.
///
/// This build does steps 2-3 directly on [`Room::title`]; the component branch
/// is not recoverable without `cena_map::locate`.
pub fn room_of(map: &Map, state: &crate::GameState) -> Option<RoomId> {
    let title = state.room.title.trim();
    if title.is_empty() {
        return None;
    }
    map.room_id(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Map {
        Map::new(vec![
            Room {
                id: 284,
                number: "284".into(),
                title: "Wehnimer's Landing, Town Square".into(),
                area: "Wehnimer's Landing".into(),
                exits: vec![Exit {
                    word: "north".into(),
                    to: 285,
                    price: None,
                    when: None,
                }],
                routines: vec![],
            },
            Room {
                id: 285,
                number: "285".into(),
                title: "Wehnimer's Landing, North Ring Road".into(),
                area: "Wehnimer's Landing".into(),
                exits: vec![],
                routines: vec![],
            },
        ])
    }

    #[test]
    fn looks_rooms_up_both_ways() {
        let map = map();
        assert_eq!(map.room(285).unwrap().number, "285");
        assert_eq!(
            map.room_id("  wehnimer's landing, north ring road "),
            Some(285)
        );
        assert_eq!(map.room(999), None);
        assert_eq!(map.titles().len(), 2);
    }

    #[test]
    fn area_filter_and_exits() {
        let map = map();
        assert_eq!(map.area("wehnimer's landing").len(), 2);
        assert_eq!(map.area("Icemule Trace").len(), 0);
        let exit = map.room(284).unwrap().exit("north").unwrap();
        assert_eq!(exit.to, 285);
        assert!(map.room(284).unwrap().exit("south").is_none());
    }
}
