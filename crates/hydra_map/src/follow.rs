//! Which room the walker is in, and the ways out of it.
//!
//! `room_of` (`0x1015b3304`) turns the game's room block into a map room id and
//! `ways_out_of` (`0x1015b28cc`) lists a room's exits. `Follower` caches the
//! answer: it only re-places the walker when the room-change counter moved.

use crate::cena::{title_from_subtitle, Located, Map, RoomId};

/// One way out of a room.
///
/// `ways_out_of` walks the room's exits (`+0xc8`/`+0xd0`, element size `0xb8`),
/// folds each exit's condition text with `trim_matches`, compares the resulting
/// word against `"east"`/`"west"` (`0x7473_6165` and that value with `0x412`
/// or-ed in, at `0x1015b2930`) and inserts what survives into a `BTreeMap` — so
/// the answer is de-duplicated by word and ordered by it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WayOut {
    /// The word or command the game accepts.
    pub word: String,
    /// Where it leads, as a map room id.
    pub to: RoomId,
    /// `cena_map::exit::Cost::price`, when the exit charges.
    pub price: Option<u32>,
    /// `cena_map::cond::Cond`, when the exit only opens under a condition.
    pub when: Option<String>,
}

/// The ways out of room `id`.
///
/// An unknown room, or a room with no exits, gives an empty vector — the
/// early return at `0x1015b2900` and the empty-`Vec` niche at `0x1015b3144`.
pub fn ways_out_of(map: &Map, id: RoomId) -> Vec<WayOut> {
    let Some(room) = map.room(id) else {
        return Vec::new();
    };
    let mut out = std::collections::BTreeMap::new();
    for exit in &room.exits {
        let word = exit.word.trim().to_string();
        if word.is_empty() {
            continue;
        }
        out.entry(word.clone()).or_insert_with(|| WayOut {
            word,
            to: exit.to,
            price: exit.price,
            when: exit.when.clone(),
        });
    }
    out.into_values().collect()
}

/// What the game said about the room the walker is standing in.
///
/// **Departure.** The original read this straight out of
/// `hydra_model::state::room::Room` (`0x1015b3334`..`0x1015b33a0`):
///
/// * `state.room` at `+0xa70`: a styled string whose plain text is split by
///   `cena_map::locate::title_from_subtitle`;
/// * `state.room` at `+0xa88`: the room's own styled text (`Runs::plain`);
/// * `Room::component(&self, "room exits")` — a ten-byte component name at
///   `0x10245becc`;
/// * `+0xa58`/`+0xa60`/`+0xa68`: the room number as text, parsed as an `i64`
///   with an optional sign;
/// * `+0x1340`: a `u32` counter that changes whenever the room block does.
///
/// This build takes those facts as an argument instead of reaching into a
/// `GameState`: the recovered `hydra_model::state::room::Room` keeps its
/// components in a `BTreeMap` and has neither styled runs nor the counter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Standing {
    /// The room's name, as the game printed it (`"Title, Subtitle"`).
    pub name: String,
    /// The `"room exits"` component, when the room block carried one.
    pub exits: Option<String>,
    /// The room number, when the game printed one.
    pub number: Option<i64>,
    /// The room-change counter (`+0x1340`).
    pub generation: u32,
}

impl Standing {
    /// Build one from the things the game prints.
    pub fn new(name: &str, exits: Option<&str>, number: Option<i64>, generation: u32) -> Standing {
        Standing {
            name: name.to_string(),
            exits: exits.map(|s| s.to_string()),
            number,
            generation,
        }
    }
}

/// Which room the walker is in.
///
/// **Partly recovered.** `room_of` (`0x1015b3304`) takes four arguments —
/// `(map, state, hint, generation)` — and hands a 0x60-byte record to
/// `cena_map::Map::locate` (`0x1015b357c`) built from the facts above. Two of
/// those arguments are the cached room and the counter it was cached at: when
/// they still agree the `hint` is used, otherwise the room is looked up afresh
/// (the `csinc` chain at `0x1015b36d4`). The *order* in which `locate` weighs
/// number, name and exits lives in `cena_map` — see [`crate::cena::Map::locate`].
pub fn room_of(map: &Map, standing: &Standing, hint: RoomId, generation: u32) -> Option<RoomId> {
    if hint != 0 && generation == standing.generation && map.room(hint).is_some() {
        return Some(hint);
    }
    let (title, subtitle) = title_from_subtitle(&standing.name);
    let at = Located {
        number: standing.number.filter(|n| *n != 0),
        title: (!title.is_empty()).then_some(title),
        subtitle: (!subtitle.is_empty()).then_some(subtitle),
        exits: standing.exits.clone(),
    };
    map.locate(&at)
}

/// Where the follower has placed the walker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Where {
    /// "Where you are is not known yet." (`0x10245bf22`).
    #[default]
    Unknown,
    In(RoomId),
}

/// What the map has to say about where the walker is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub at: Where,
    /// The ways out of the room, when it is known.
    pub ways_out: Vec<WayOut>,
    /// The area the room is drawn in, when the map knows it.
    pub area: Option<String>,
}

/// Caches the walker's placement.
///
/// The three `u32`s the binary keeps at `+0x78`, `+0x7c` and `+0x80` are the
/// discriminant (`1` once placed), the room-change counter and the room id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Follower {
    pub at: Where,
    /// The counter value the placement was made at (`+0x7c`).
    pub generation: u32,
}

impl Follower {
    /// @0x1015b3690 — re-place the walker when the room changed, then report
    /// what the map knows.
    ///
    /// **Partly recovered.** The original also called `Atlas::scene_of`,
    /// `Atlas::progress`, `sailing::Navigator::fix` and `next_door`, and put the
    /// results into a `View` carrying a `MinimapView`; the drawing half is a UI
    /// seam (see `NOTES.md`) and is not reproduced. What is reproduced is the
    /// placement rule and the ways out.
    pub fn view(&mut self, map: &Map, standing: &Standing) -> View {
        let (hint, generation) = match self.at {
            Where::Unknown => (0, 0),
            Where::In(id) => (id, self.generation),
        };
        match room_of(map, standing, hint, generation) {
            Some(id) => {
                self.at = Where::In(id);
                self.generation = standing.generation;
                let area = map.room(id).and_then(|r| r.area().map(|a| a.to_string()));
                View {
                    at: Where::In(id),
                    ways_out: ways_out_of(map, id),
                    area,
                }
            }
            None => {
                self.at = Where::Unknown;
                View {
                    at: Where::Unknown,
                    ways_out: Vec::new(),
                    area: None,
                }
            }
        }
    }

    /// @0x1015b3fb4 — the ways out of the room the walker was last placed in.
    ///
    /// The `cmp w9, #1` at the head is the discriminant check: when the walker
    /// has not been placed, the answer is an empty vector (`x8 = 8` and three
    /// zero stores, i.e. the empty-`Vec` niche).
    pub fn ways_out(&self, map: &Map) -> Vec<WayOut> {
        match self.at {
            Where::In(id) => ways_out_of(map, id),
            Where::Unknown => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cena::{Exit, Room};

    fn map() -> Map {
        Map::from_rooms(vec![
            Room {
                id: 7116001,
                title: "Sleeping Drake Harbor".into(),
                note: Some("Pier One".into()),
                areas: vec!["North Blue".into()],
                exits: vec![
                    Exit {
                        word: "north".into(),
                        to: 7116224,
                        price: None,
                        when: None,
                    },
                    Exit {
                        word: " north ".into(),
                        to: 7116225,
                        price: None,
                        when: None,
                    },
                    Exit {
                        word: "go gate".into(),
                        to: 7116226,
                        price: Some(10),
                        when: Some("daytime".into()),
                    },
                ],
                ..Default::default()
            },
            Room {
                id: 7116224,
                title: "The Tenebrous Cauldron".into(),
                ..Default::default()
            },
        ])
    }

    #[test]
    fn ways_out_are_trimmed_deduped_and_ordered() {
        let w = ways_out_of(&map(), 7116001);
        let words: Vec<&str> = w.iter().map(|x| x.word.as_str()).collect();
        assert_eq!(words, vec!["go gate", "north"]);
        assert_eq!(w[0].price, Some(10));
        assert_eq!(w[0].when.as_deref(), Some("daytime"));
        assert_eq!(w[1].to, 7116224, "the first exit with that word wins");
        assert!(ways_out_of(&map(), 1).is_empty());
    }

    #[test]
    fn room_of_prefers_a_still_valid_hint() {
        let m = map();
        let standing = Standing::new("Sleeping Drake Harbor, Pier One", None, None, 4);
        assert_eq!(room_of(&m, &standing, 7116224, 4), Some(7116224));
        // A stale generation means the hint is not trusted.
        assert_eq!(room_of(&m, &standing, 7116224, 3), Some(7116001));
        // And a known number wins over the name.
        let numbered = Standing::new("Nowhere", None, Some(7116224), 9);
        assert_eq!(room_of(&m, &numbered, 0, 0), Some(7116224));
        let unknown = Standing::new("Nowhere at all", None, None, 9);
        assert_eq!(room_of(&m, &unknown, 0, 0), None);
    }

    #[test]
    fn a_follower_places_the_walker_and_lists_the_ways_out() {
        let m = map();
        let mut f = Follower::default();
        assert!(f.ways_out(&m).is_empty());
        let view = f.view(
            &m,
            &Standing::new("Sleeping Drake Harbor, Pier One", None, None, 1),
        );
        assert_eq!(view.at, Where::In(7116001));
        assert_eq!(view.area.as_deref(), Some("North Blue"));
        assert_eq!(view.ways_out.len(), 2);
        assert_eq!(f.ways_out(&m).len(), 2);
        // Losing the room clears the placement.
        let lost = f.view(&m, &Standing::new("Somewhere Else", None, None, 2));
        assert_eq!(lost.at, Where::Unknown);
        assert!(f.ways_out(&m).is_empty());
    }
}
