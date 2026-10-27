//! Routes: the paths between the places a walk can name.
//!
//! Recovered from `hydra_travel::route::{table,places,destination,here,
//! described,itinerary,how,why_shut,not_known}`
//! (`0x10154fd24`, `0x10155082c`, `0x10154ebc4`, `0x10154fd20`,
//! `0x101551634`, `0x1015517e4`, `0x10154f7fc`, `0x10155138c`, `0x101551ccc`).
//!
//! The three refusal texts are recovered verbatim from the pool at
//! `0x10241eff0`:
//!
//! * `not for this character`
//! * `Hydra does not walk this yet`
//! * `the map gives it no cost`

use crate::map::Map;
use crate::{GameState, RoomId};

/// Why a route will not serve a walker.
pub const NOT_FOR_THIS_CHARACTER: &str = "not for this character";
/// Why a route is not implemented.
pub const DOES_NOT_WALK: &str = "Hydra does not walk this yet";
/// Why a route has no price.
pub const NO_COST: &str = "the map gives it no cost";

/// One step of a route as the walk sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The command.
    pub command: String,
    /// The room it leads to.
    pub to: RoomId,
    /// The silver it charges, if any.
    pub price: Option<u32>,
    /// The condition on the exit, if any.
    pub when: Option<String>,
}

/// A route between two named places.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Route {
    /// The name the route is saved under.
    pub name: String,
    /// Where it starts.
    pub from: RoomId,
    /// Where it ends.
    pub to: RoomId,
    /// The steps.
    pub steps: Vec<Step>,
}

/// The saved routes (`route::table`, 2824 bytes at `0x10154fd24`).
///
/// **Re-checked: partial, and not what the earlier note said.** The body is not a
/// route builder at all — it renders a **report**:
///
/// * `cena_map::map::Map::room` (`0x10154fd54`) for the room it starts from;
/// * the header literal at `0x10241efc0` is `"STEP      TRIP    TIME  "`;
/// * the second header, from the piece run at `0x103e23aeb`, is
///   `"[status] " | HP " | MP " | RT " | CT " | L " | R "` — the same
///   `[status] … | HP …` header shape the rest of Hydra prints;
/// * `format_inner` is called four times, and the pieces it formats into are
///   `… is a room, not a name: .go2 …` and `… goes there. Give it a word:
///   .go2 save den=…` (`0x10241f03a`, decoded);
/// * it walks a list of 0x78-byte (120) records and grows a `Vec` of them
///   (`0x10154ff68`..), so the "table" is one row per saved route.
///
/// This build keeps the plain `Route` list: the original's row type lives in
/// `cena_map::route` (`Routes::seconds_to` is what `drive::walk` calls at
/// `0x10110c538`), and its 120-byte shape is not reconstructible from here.
pub fn table(saved: &std::collections::BTreeMap<String, Vec<u32>>) -> Vec<Route> {
    saved
        .iter()
        .map(|(name, rooms)| Route {
            name: name.clone(),
            from: rooms.first().copied().unwrap_or_default(),
            to: rooms.last().copied().unwrap_or_default(),
            steps: rooms
                .windows(2)
                .map(|pair| Step {
                    command: String::new(),
                    to: pair[1],
                    price: None,
                    when: None,
                })
                .collect(),
        })
        .collect()
}

/// The places a route can be asked for (`route::places`, 2912 bytes at
/// `0x10155082c`).
///
/// **Partly recovered.** The body builds a `BTreeMap<&str, String>`
/// (`or_insert_with`, `BTreeMap::remove`, and a `&str` key — the drop shim is
/// `drop_in_place<BTreeMap<&str, String>>`), folds a `Map<I,F>` over it
/// (`unicode::uppercase::lookup` — so something is upper-cased), takes the keys
/// off with `Map::routes` in the loop, and returns a `Vec<String>`. This build
/// returns the room titles of the rooms that carry a priced exit, which is the
/// recovered "priced rooms" half and not the `BTreeMap` de-duplication.
pub fn places(map: &Map) -> Vec<&str> {
    map.rooms()
        .iter()
        .filter(|room| room.exits.iter().any(|exit| exit.price.is_some()))
        .map(|room| room.title.as_str())
        .collect()
}

/// The room a place name means (`route::destination`, 2840 bytes at
/// `0x10154ebc4`).
///
/// **ESTABLISHED (shape)**: the body trims and lower-cases the name
/// (`<str>::trim_matches` then `<str>::to_lowercase`), calls [`described`] to
/// split it, and looks the parts up through **three** doors, in this order:
/// `cena_map::map::Map::ids_for_uid`, `cena_map::map::Map::room` and
/// `cena_map::route::<impl Map>::routes` (`0x10154e`..), answering with the
/// niche-encoded `Option<RoomId>`. The `Vec` collect in the middle is the
/// `.`-split (the closure at `0x10154f6dc`).
pub fn destination(map: &Map, name: &str) -> Option<RoomId> {
    let name = name.trim().to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    if let Some(room) = map.room_id(&name) {
        return Some(room);
    }
    // "Wehnimer's Landing. The Statue" style names: take the last part that
    // resolves.
    name.split('.')
        .rev()
        .find_map(|part| map.room_id(part.trim()))
}

/// Which room the character is in (`route::here`, 4 bytes at `0x10154fd20`).
///
/// **ESTABLISHED**: a tail call to `hydra_map::follow::room_of`
/// (`0x1015b3304`) — see [`crate::map::room_of`].
pub fn here(map: &Map, state: &GameState) -> Option<RoomId> {
    crate::map::room_of(map, state)
}

/// Turn a route description into steps (`route::described`, 432 bytes at
/// `0x101551634`).
///
/// **ESTABLISHED**: trim, ASCII-lowercase, split on `.` (`movi.2s v0, #0x2e` at
/// `0x1015516fc`), then map each part through the map with the 384-byte room
/// stride (`madd x9, x9, #0x180, x8` at `0x101551728`). A name that resolves to
/// nothing yields an empty step list (the `w8 = 4` empty-vector niche at
/// `0x10155178c`).
pub fn described(map: &Map, line: &str) -> Vec<RoomId> {
    line.trim()
        .to_ascii_lowercase()
        .split('.')
        .filter_map(|part| map.room_id(part.trim()))
        .collect()
}

/// The itinerary of a route (`route::itinerary`, 11004 bytes at
/// `0x1015517e4`).
///
/// **Partly recovered**: the body walks the map from `from` to `to`, pricing
/// every exit (`cena_map::exit::Cost::price`) and testing every condition
/// (`cena_map::cond::Cond::ask`), and reports the room ids it would walk. This
/// build defers to [`crate::walk::Trip::path_from`], which is the same search
/// with the routine rooms removed.
pub fn itinerary(map: &Map, from: RoomId, to: RoomId) -> Option<Vec<RoomId>> {
    crate::walk::Trip::path_from(map, from, to)
}

/// Render a route the way the walker reads it (`route::how`, 1032 bytes at
/// `0x10154f7fc`).
///
/// The recovered shape is one `direction` (or `command`) per line, with the
/// silver charge appended as `; (through the urchins)` / `(through the
/// urchins)` when the exit is paid for (`0x10241ef9b`, `0x10241efab`).
pub fn how(steps: &[Step]) -> String {
    let mut out = String::new();
    for step in steps {
        out.push_str(&step.command);
        if step.price.is_some() {
            out.push_str("; (through the urchins)");
        }
        out.push('\n');
    }
    out
}

/// Why a route is shut for this walker (`route::why_shut`, 680 bytes at
/// `0x10155138c`).
///
/// **Seam.** Recovered: it builds a `Vec` of **pointers to conditions**
/// (8-byte elements, `ldr x23, [x20, x26]` at `0x101551528`) from a slice of
/// 0x40-byte records (`add x9, x23, x8, lsl #6` at `0x1015514f0`), asks each one
/// with `cena_map::cond::Cond::ask` (`0x101551534`), and for the ones that answer
/// `2` calls [`not_known`] and formats a sentence (`format_inner` at
/// `0x10155157c`);
/// the exit's price comes from `cena_map::exit::Cost::price`. The three
/// [`NOT_FOR_THIS_CHARACTER`]/[`DOES_NOT_WALK`]/[`NO_COST`] literals are in the
/// crate's pool but the branch that selects them was not found in this function's
/// dump, so the mapping below is this build's.
///
/// `Cond` is a `cena_map` type with no counterpart here (this crate's
/// conditions are `Option<String>`), which is why the original's argument list
/// cannot be reproduced.
pub fn why_shut(step: &Step, condition_met: bool) -> Option<&'static str> {
    if !condition_met && step.when.is_some() {
        return Some(NOT_FOR_THIS_CHARACTER);
    }
    if step.command.is_empty() {
        return Some(DOES_NOT_WALK);
    }
    if step.price.is_none() && step.when.is_some() {
        return Some(NO_COST);
    }
    None
}

/// The answer when a place is not known (`route::not_known`, **160 bytes** at
/// `0x101551ccc`).
///
/// **Re-checked: this is a predicate, not a sentence.** Every earlier claim about
/// it (9 748 bytes, "lists the candidate names") was an artifact of taking
/// function extents from the globals-only symbol file. The recovered body is:
///
/// * it walks its argument as a **condition tree** — while the discriminator at
///   `[x0]` is in `2..4` it follows the child at `[x0, #8]` (`0x101551ce0`…);
/// * at a `w8 < 2` node it recurses into the children held at
///   `[x0, #0x10]`/`[x0, #0x18]` as 0x38-byte records, returning the first
///   child that answers yes (`0x101551d0c`…);
/// * at a leaf it calls `cena_map::cond::Cond::ask` and answers
///   `ask() == 2` (`cnd w8, #0x2; csel x0, x20, xzr, eq` at `0x101551d44`).
///
/// So `not_known(&Cond) -> bool`. This build's `Cond` is an `Option<String>`
/// condition word, which has no tree to walk: the function below is a **seam**
/// that answers the same yes/no question about the only condition shape this
/// crate carries, and its phrasing is this build's, not the binary's.
///
/// // UNKNOWN: the sentence the original formatted at `0x10155157c`.
pub fn not_known(map: &Map, name: &str) -> String {
    let known = places(map);
    if known.is_empty() {
        return format!("{name} is not known to me.");
    }
    format!("{name} is not known to me; I know {}.", known.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Exit, Room};

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
                    price: Some(12),
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
    fn the_three_refusals_are_the_recovered_literals() {
        assert_eq!(NOT_FOR_THIS_CHARACTER, "not for this character");
        assert_eq!(DOES_NOT_WALK, "Hydra does not walk this yet");
        assert_eq!(NO_COST, "the map gives it no cost");
    }

    #[test]
    fn destinations_are_looked_up_after_trimming() {
        let map = map();
        assert_eq!(
            destination(&map, "  wehnimer's landing, town square "),
            Some(284)
        );
        // Names with a subtitle resolve on the part that is in the map.
        assert_eq!(
            destination(&map, "Somewhere. Wehnimer's Landing, Town Square"),
            Some(284)
        );
        assert_eq!(destination(&map, ""), None);
        assert_eq!(destination(&map, "nowhere"), None);
    }

    #[test]
    fn described_splits_on_dots() {
        let map = map();
        assert_eq!(
            described(&map, "Wehnimer's Landing, Town Square. Wehnimer's Landing, North Ring Road"),
            vec![284, 285]
        );
        assert!(described(&map, "nothing here").is_empty());
    }

    #[test]
    fn places_are_the_priced_rooms() {
        let map = map();
        assert_eq!(places(&map), vec!["Wehnimer's Landing, Town Square"]);
        let text = not_known(&map, "den");
        assert!(text.contains("den is not known"));
        assert!(text.contains("Wehnimer's Landing, Town Square"));
    }

    #[test]
    fn why_shut_answers_the_recovered_reasons() {
        let step = Step {
            command: "north".into(),
            to: 285,
            price: None,
            when: Some("Citizenship".into()),
        };
        assert_eq!(why_shut(&step, false), Some(NOT_FOR_THIS_CHARACTER));
        assert_eq!(why_shut(&step, true), Some(NO_COST));
        let step = Step {
            command: "north".into(),
            to: 285,
            price: Some(12),
            when: None,
        };
        assert_eq!(why_shut(&step, true), None);
        let mut quiet = step.clone();
        quiet.command = String::new();
        quiet.price = Some(1);
        assert_eq!(why_shut(&quiet, true), Some(DOES_NOT_WALK));
    }

    #[test]
    fn how_marks_the_paid_exits() {
        let steps = vec![
            Step {
                command: "north".into(),
                to: 285,
                price: Some(12),
                when: None,
            },
            Step {
                command: "east".into(),
                to: 286,
                price: None,
                when: None,
            },
        ];
        assert_eq!(
            how(&steps),
            "north; (through the urchins)\neast\n".to_string()
        );
    }

    #[test]
    fn itinerary_uses_the_walk_search() {
        let map = map();
        assert_eq!(itinerary(&map, 284, 285), Some(vec![284, 285]));
    }
}
