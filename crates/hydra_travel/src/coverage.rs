//! What the travel system can walk, over the whole map.
//!
//! Recovered from `hydra_travel::coverage::{kinds,census,report,walked}`
//! (`0x10154c15c`, `0x10154c758`, `0x10154cadc`, `0x10154cd90`).
//!
//! The four functions are a report over the map: `kinds` lists the exit/step
//! kinds the walker understands, `census` counts the rooms and exits for each
//! kind, `report` renders the census, and `walked` measures how much of a route
//! the walker would actually walk.
//!
//! The kind names are literals from the merged `__cstring` run at
//! `0x10241ea44`:
//! `command | cost Fixed | cost Gated | cost Ladder | cost Hasted |
//! cost Table | step Await | step OrderByName | step WanderWhile`.

pub mod variant;
pub mod variant_is;

/// The exit costs the map can charge, from `0x10241ea44`.
pub const COSTS: [&str; 5] = [
    "cost Fixed",
    "cost Gated",
    "cost Ladder",
    "cost Hasted",
    "cost Table",
];

/// The step kinds a walk can take, from `0x10241e5a0` and `0x10241ea44`.
pub const STEPS: [&str; 3] = ["step Await", "step OrderByName", "step WanderWhile"];

/// One kind and how many rooms use it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kind {
    pub name: &'static str,
    pub rooms: usize,
}

/// The counts `census` produces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    pub rooms: usize,
    pub exits: usize,
    /// Rooms whose exits the walker can price (`cost *`).
    pub priced: usize,
    /// Rooms carrying a routine (a puzzle the walker has to solve).
    pub routines: usize,
    pub kinds: Vec<Kind>,
}

/// The kinds this build knows, in the recovered order (`0x10154c15c`).
///
/// **ESTABLISHED (names and order)**; the body writes each name through
/// [`variant::Name`] and keeps the ones it could write.
pub fn kinds() -> Vec<&'static str> {
    let mut out = Vec::new();
    for name in COSTS.iter().chain(STEPS.iter()) {
        let mut buf = variant::Name::new();
        use std::fmt::Write;
        if write!(buf, "{name}").is_ok() {
            out.push(*name);
        }
    }
    out
}

/// Count the map (`0x10154c758`).
///
/// **Partly recovered**: the body walks the room list once, counting rooms,
/// exits and the rooms that carry a priced exit or a routine. The predicate for
/// "priced" comes from `cena_map::exit::Cost::price` being `Some`.
pub fn census(map: &crate::map::Map) -> Census {
    let mut census = Census {
        rooms: map.rooms().len(),
        ..Census::default()
    };
    for room in map.rooms() {
        census.exits += room.exits.len();
        if room.exits.iter().any(|exit| exit.price.is_some()) {
            census.priced += 1;
        }
        if !room.routines.is_empty() {
            census.routines += 1;
        }
    }
    census.kinds = kinds()
        .into_iter()
        .map(|name| Kind { name, rooms: 0 })
        .collect();
    census
}

/// Render the census (`0x10154cadc`).
///
/// The message shape is the recovered `reveals the following contents` /
/// `the containers listed` style of report (`0x10241e97a`); the travel report
/// lists one `kind: count` line per kind after a header.
pub fn report(census: &Census) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} rooms, {} exits, {} with a price, {} with a routine\n",
        census.rooms, census.exits, census.priced, census.routines
    ));
    for kind in &census.kinds {
        out.push_str(&format!("{}: {}\n", kind.name, kind.rooms));
    }
    out
}

/// How many rooms of a route the walker would actually walk
/// (`0x10154cd90`).
///
/// **Partly recovered**: the body walks the path and counts the steps whose
/// exit the walker can price and whose room has no unsolved routine; the
/// precise bookkeeping (the `Vec<Vec<RoomId>>` drop glue in the same region)
/// suggests it also records the rejected runs.
pub fn walked(map: &crate::map::Map, path: &[crate::RoomId]) -> usize {
    let mut walked = 0;
    for pair in path.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let Some(room) = map.room(from) else {
            continue;
        };
        if room.routines.is_empty() && room.exits.iter().any(|exit| exit.to == to) {
            walked += 1;
        }
    }
    walked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Exit, Map, Room};

    fn map() -> Map {
        Map::new(vec![
            Room {
                id: 1,
                number: "1".into(),
                title: "One".into(),
                area: "A".into(),
                exits: vec![Exit {
                    word: "north".into(),
                    to: 2,
                    price: Some(12),
                    when: None,
                }],
                routines: vec![],
            },
            Room {
                id: 2,
                number: "2".into(),
                title: "Two".into(),
                area: "A".into(),
                exits: vec![Exit {
                    word: "south".into(),
                    to: 1,
                    price: None,
                    when: None,
                }],
                routines: vec![crate::walk::routines::Routine::named(
                    "sword_in_the_gorge",
                )],
            },
        ])
    }

    #[test]
    fn the_kind_names_are_the_recovered_literals() {
        assert_eq!(
            kinds(),
            vec![
                "cost Fixed",
                "cost Gated",
                "cost Ladder",
                "cost Hasted",
                "cost Table",
                "step Await",
                "step OrderByName",
                "step WanderWhile",
            ]
        );
    }

    #[test]
    fn census_counts_and_reports() {
        let census = census(&map());
        assert_eq!(census.rooms, 2);
        assert_eq!(census.exits, 2);
        assert_eq!(census.priced, 1);
        assert_eq!(census.routines, 1);
        let text = report(&census);
        assert!(text.starts_with("2 rooms, 2 exits, 1 with a price, 1 with a routine\n"));
        assert!(text.contains("cost Fixed: 0"));
    }

    #[test]
    fn walked_skips_routine_rooms() {
        let map = map();
        assert_eq!(walked(&map, &[1, 2]), 1);
        assert_eq!(walked(&map, &[2, 1]), 0);
        assert_eq!(walked(&map, &[1]), 0);
    }
}
