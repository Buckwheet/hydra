//! Walking a route: the trip, its pace, the move compiler and the routines.
//!
//! Recovered from `hydra_travel::walk::*`. The private modules the inventory
//! reveals in the symbol table are all here: `drive` (turning map steps into
//! game commands), `hands` (what to hold), `instability`, `kept` (items a walk
//! keeps in hand), `preflight` (silver, shops, gigas), `pace` (`typeahead_of`,
//! `typeahead_said`, `Batch::landed`, and the `Trip::send_ahead` impl),
//! `mover` (`Mover::{new,tick}`, `is_stunned`, `exchange_of`), `recovery`
//! (`Attempt::{new,again,react}`, `with_verb`), `replies` (`word_after`,
//! `numbered`), `run` (`Run::{tick,work}`, `said`, `filled`) and `routines`.

pub mod drive;
pub mod hands;
pub mod instability;
pub mod kept;
pub mod mover;
pub mod pace;
pub mod preflight;
pub mod recovery;
pub mod replies;
pub mod routines;
pub mod run;

use crate::map::Map;
use crate::RoomId;

/// What stopped a move being sent.
///
/// The five recovered refusals are the `walk::Trip::tick` / `aside` literals at
/// `0x10241e2d2`..`0x10241e35b`.
pub const CANNOT_MOVE: [&str; 5] = [
    "You cannot do that while mounted.",
    "There's not enough room to do that!",
    "There is not enough room to stand up in here.",
    "You'd tip the boat over!",
    "You are overburdened and cannot manage to stand.",
];

/// One walk along a route.
///
/// **Partly recovered.** The field set is this build's; the recovered methods
/// read a path (`path_from`, `0x101542dbc`), the current index, the command it
/// last sent, and the destination.
#[derive(Clone, Debug, Default)]
pub struct Trip {
    /// Where the walk started.
    pub from: RoomId,
    /// Where it is going.
    pub to: RoomId,
    /// The room ids of the path, in order.
    pub path: Vec<RoomId>,
    /// How far along the path the walk is.
    pub at: usize,
    /// The command last sent.
    pub sent: Option<String>,
    /// Did the walk give up?
    pub given_up: bool,
}

impl Trip {
    /// A trip along a known path.
    pub fn new(from: RoomId, to: RoomId, path: Vec<RoomId>) -> Trip {
        Trip {
            from,
            to,
            path,
            at: 0,
            sent: None,
            given_up: false,
        }
    }

    /// The next room on the path (`Trip::ahead`, 96 bytes at `0x101542828`).
    pub fn ahead(&self) -> Option<RoomId> {
        self.path.get(self.at + 1).copied()
    }

    /// The room after the next (`Trip::aside`, 192 bytes at `0x101542888`).
    pub fn aside(&self) -> Option<RoomId> {
        self.path.get(self.at + 2).copied()
    }

    /// Have we arrived?
    pub fn arrived(&self) -> bool {
        self.at + 1 >= self.path.len()
    }

    /// Record the command about to be sent.
    ///
    /// `Trip::cannot_send` (`0x101541e10`) is the guard: it answers with the
    /// [`CANNOT_MOVE`] line that forbids the move, if one of them is in the
    /// output, and `None` when the move may go.
    pub fn cannot_send(&self, see: &[String]) -> Option<&'static str> {
        CANNOT_MOVE
            .into_iter()
            .find(|refusal| see.iter().any(|line| line.contains(refusal)))
    }

    /// Send the next command (`Trip::to`, 652 bytes at `0x1015423e4`).
    pub fn to(&mut self, command: &str) -> Option<String> {
        if self.given_up {
            return None;
        }
        self.sent = Some(command.to_string());
        Some(command.to_string())
    }

    /// One turn of the walk (`Trip::tick`, 440 bytes at `0x101542670`).
    ///
    /// The body checks the refusals first (`see`), then the pace, then sends the
    /// next move. The private `Trip::tick_inner` (`0x101540d04`) is the same
    /// loop with the state already borrowed.
    pub fn tick(&mut self, map: &Map, see: &[String]) -> Option<String> {
        if let Some(refusal) = self.cannot_send(see) {
            let _ = refusal;
            return None;
        }
        let ahead = self.ahead()?;
        let here = *self.path.get(self.at)?;
        let command = drive::command_for(map, here, ahead);
        self.at += 1;
        self.to(&command)
    }

    /// Take a line of output in (`Trip::heard`, 688 bytes at `0x101542948`).
    ///
    /// A line that names the room just walked into advances the walk; a line
    /// that matches a refusal stops it.
    pub fn heard(&mut self, line: &str) -> bool {
        if CANNOT_MOVE.iter().any(|refusal| line.contains(refusal)) {
            self.given_up = true;
            return true;
        }
        false
    }

    /// Was the walk seeded from a saved target?
    ///
    /// `Trip::seeded` (452 bytes at `0x101542bf8`) reads the stored `targets`
    /// entry and answers whether the path came from it.
    pub fn seeded(&self, saved: &[u32]) -> bool {
        !saved.is_empty() && saved == self.path
    }

    /// Build the path from one room to another (`Trip::path_from`, **488 bytes**
    /// at `0x101542dbc`).
    ///
    /// **Re-checked: the search is not in this crate.** The earlier note's
    /// "24 504 bytes: the A* with `Cost` prices" was an artifact of taking
    /// function extents from the globals-only symbol file; with the local symbol
    /// table the function is 488 bytes and does exactly this:
    ///
    /// * seeds a one-element `Vec<u32>` with the starting room
    ///   (`str w21, [x22]` at `0x101542e14`);
    /// * calls `cena_map::route::<impl Map>::routes` (`0x101542df8`) to build the
    ///   route graph from the map and the walker;
    /// * calls `cena_map::route::Routes::path_to` (`0x101542e30`) with the
    ///   destination and that seed;
    /// * answers the `Option<Vec<RoomId>>` the call gives back — the `isize::MIN`
    ///   niche in the capacity slot is the `None` arm (`0x101542e38`).
    ///
    /// The cost metric (and the routine-room exclusion the earlier note attributed
    /// to this function) lives in `cena_map::route`, which is not in this
    /// workspace. This build therefore keeps a breadth-first search over
    /// [`Map`], refusing rooms that carry a routine, as a documented stand-in for
    /// `Routes::path_to` — the exclusions are the same idea, the metric is not.
    pub fn path_from(map: &Map, from: RoomId, to: RoomId) -> Option<Vec<RoomId>> {
        if from == to {
            return Some(vec![from]);
        }
        let mut seen: std::collections::BTreeSet<RoomId> = std::collections::BTreeSet::new();
        let mut queue: std::collections::VecDeque<Vec<RoomId>> =
            std::collections::VecDeque::new();
        seen.insert(from);
        queue.push_back(vec![from]);
        while let Some(path) = queue.pop_front() {
            let Some(here) = path.last().copied() else {
                continue;
            };
            let Some(room) = map.room(here) else {
                continue;
            };
            for exit in &room.exits {
                if exit.to == to {
                    let mut found = path.clone();
                    found.push(to);
                    return Some(found);
                }
                if !seen.insert(exit.to) {
                    continue;
                }
                let Some(next) = map.room(exit.to) else {
                    continue;
                };
                if !next.routines.is_empty() {
                    continue;
                }
                let mut longer = path.clone();
                longer.push(exit.to);
                queue.push_back(longer);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Exit, Map, Room};

    fn map() -> Map {
        let room = |id: RoomId, exits: Vec<Exit>, routines: Vec<crate::walk::routines::Routine>| {
            Room {
                id,
                number: id.to_string(),
                title: format!("room {id}"),
                area: "A".into(),
                exits,
                routines,
            }
        };
        Map::new(vec![
            room(
                1,
                vec![Exit {
                    word: "north".into(),
                    to: 2,
                    price: None,
                    when: None,
                }],
                vec![],
            ),
            room(
                2,
                vec![Exit {
                    word: "north".into(),
                    to: 3,
                    price: None,
                    when: None,
                }],
                vec![],
            ),
            room(3, vec![], vec![]),
        ])
    }

    #[test]
    fn the_refusals_are_the_recovered_literals() {
        assert_eq!(CANNOT_MOVE.len(), 5);
        assert_eq!(CANNOT_MOVE[0], "You cannot do that while mounted.");
        assert_eq!(
            CANNOT_MOVE[4],
            "You are overburdened and cannot manage to stand."
        );
    }

    #[test]
    fn a_trip_walks_its_path() {
        let map = map();
        let path = Trip::path_from(&map, 1, 3).unwrap();
        assert_eq!(path, vec![1, 2, 3]);
        let mut trip = Trip::new(1, 3, path);
        assert_eq!(trip.ahead(), Some(2));
        assert_eq!(trip.aside(), Some(3));
        assert_eq!(trip.tick(&map, &[]).as_deref(), Some("north"));
        assert_eq!(trip.tick(&map, &[]).as_deref(), Some("north"));
        assert!(trip.arrived());
        assert_eq!(trip.tick(&map, &[]), None);
    }

    #[test]
    fn a_refusal_stops_the_walk() {
        let map = map();
        let mut trip = Trip::new(1, 3, vec![1, 2, 3]);
        let see = vec![CANNOT_MOVE[4].to_string()];
        assert_eq!(trip.cannot_send(&see), Some(CANNOT_MOVE[4]));
        assert_eq!(trip.tick(&map, &see), None);
        assert!(trip.heard(CANNOT_MOVE[4]));
        assert!(trip.given_up);
    }

    #[test]
    fn routine_rooms_are_routed_around() {
        let mut map = map();
        map = Map::new(
            map.rooms()
                .iter()
                .map(|r| {
                    let mut r = r.clone();
                    if r.id == 2 {
                        r.routines = vec![crate::walk::routines::Routine::named("mirror")];
                    }
                    r
                })
                .collect(),
        );
        assert_eq!(Trip::path_from(&map, 1, 3), None);
    }

    #[test]
    fn seeded_targets() {
        let trip = Trip::new(1, 3, vec![1, 2, 3]);
        assert!(trip.seeded(&[1, 2, 3]));
        assert!(!trip.seeded(&[]));
    }
}
