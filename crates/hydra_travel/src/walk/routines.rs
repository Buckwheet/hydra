//! The puzzle routines: the little state machines that get a walk through a
//! room the map alone cannot cross.
//!
//! Recovered from `hydra_travel::walk::routines::*`. The important shapes:
//!
//! * `Solver` is a trait with two methods — `next` (symbols at
//!   `<T as Solver>::next` for every routine) and `keep` (the default impl
//!   monomorphisations at `0x101523b00`, `0x1015337e4`, `0x101557bac`,
//!   `0x10156cc08`…). `keep` is what tells the walker whether a line of output
//!   belongs to the routine.
//! * `Next` is the answer of [`Solver::next`]; it is dropped 561 times in the
//!   crate and stored in a `VecDeque<Next>` (`drop_in_place<VecDeque<
//!   hydra_travel::walk::routines::Next>>` at `0x1015970b8`).
//! * `Routine` is the *data* a map room carries (`cena_map::routine::Routine`).
//!   Its field names come from the serializer at `0x101572368`:
//!   `kind`, `rooms`, `hands_free_in`, `by_uid`, `enter`, `batter`, `puzzle`,
//!   `route`, `errand`, `starts`, `landmarks`, `after`, `noun`, `open`.
//!   The `kind` words are the 31 literals at `0x1024223ed`..`0x10242264d`.
//! * `solver_for` (`0x10156b7a0`) builds the solver for one room: it looks the
//!   room up (`cena_map::map::Map::room`, `0x10156b7d8`), then dispatches twice
//!   through the two jump tables at `0x1024207c0` (byte offsets) and
//!   `0x1024207d0` (u16 offsets), calls `day_pass::DayPass::new` and
//!   `sword_gorge::SwordGorge::new`, and matches the `Isle of Four Winds` /
//!   `Mist Harbor` literals at `0x102420c61`.
//!
//! The original returned the solver through the `Next`-shaped niche encoding
//! (`isize::MIN + n`, `n` in `1..=0xb`); this build returns a boxed
//! `dyn Solver`, which is the same thing without the hand-rolled vtable.

pub mod altar_levers;
pub mod bridge_wheel;
pub mod bronze_gate;
pub mod colour_barrier;
pub mod confluence;
pub mod crown_door;
pub mod cutter;
pub mod day_pass;
pub mod eye_spy_runes;
pub mod familiar_doors;
pub mod flight_of_steps;
pub mod giant;
pub mod guild_password;
pub mod labyrinth_entry;
pub mod minotaur_maze;
pub mod mirror;
pub mod mural_of_deities;
pub mod patrol;
pub mod ring_wedges;
pub mod rolaren_gate;
pub mod rune_staircase;
pub mod search_rooms;
pub mod seeking;
pub mod signposts;
pub mod sword_gorge;
pub mod three_pillars;
pub mod trinket;
pub mod vaalorn_door;
pub mod workshop_pillars;

use crate::RoomId;

/// The `kind` words a map routine can carry, in the order the literals appear
/// in the pool at `0x1024223ed`..`0x10242264d`.
pub const KINDS: [&str; 31] = [
    "confluence",
    "minotaur_maze",
    "signposts",
    "seeking",
    "trinket",
    "guild_password",
    "flight_of_steps",
    "search_rooms",
    "mirror",
    "ring_wedges",
    "bronze_gate",
    "colour_barrier",
    "patrol",
    "day_pass",
    "giant_to_rivers_rest",
    "giant_from_rivers_rest",
    "cutter_from_marshtown",
    "cutter_from_rivers_rest",
    "sword_in_the_gorge",
    "rolaren_gate",
    "rune_staircase",
    "three_pillars",
    "vaalorn_door",
    "altar_levers",
    "eye_spy_runes",
    "crown_door",
    "bridge_wheel",
    "familiar_doors",
    "labyrinth_entry",
    "mural_of_deities",
    "workshop_pillars",
];

/// One landmark of a routine ("walk to the noun, then open it").
///
/// The field names are the serializer's (`0x1024224bd`): `noun`, `open`,
/// `enter`, `after`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Landmark {
    pub noun: String,
    pub open: Option<String>,
    pub enter: Option<String>,
    pub after: Option<String>,
}

/// The routine data attached to a room (`cena_map::routine::Routine`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Routine {
    pub kind: String,
    pub rooms: Vec<RoomId>,
    pub hands_free_in: bool,
    pub by_uid: bool,
    pub enter: Option<String>,
    pub batter: Option<String>,
    pub puzzle: Option<String>,
    pub route: Option<String>,
    pub errand: Option<String>,
    pub starts: Vec<String>,
    pub landmarks: Vec<Landmark>,
    pub after: Option<String>,
    pub noun: Option<String>,
    pub open: Option<String>,
}

impl Routine {
    /// A routine of one kind and nothing else.
    pub fn named(kind: &str) -> Routine {
        Routine {
            kind: kind.to_string(),
            ..Routine::default()
        }
    }

    pub fn with_rooms(kind: &str, rooms: &[RoomId]) -> Routine {
        Routine {
            kind: kind.to_string(),
            rooms: rooms.to_vec(),
            ..Routine::default()
        }
    }
}

/// A snapshot of what the walker can see.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct See {
    /// The room the character is standing in, when it is known.
    pub room: Option<RoomId>,
    /// The last lines of output.
    pub lines: Vec<String>,
    /// The character's name, for routines that have to say it.
    pub name: String,
}

impl See {
    pub fn at(room: RoomId) -> See {
        See {
            room: Some(room),
            ..See::default()
        }
    }

    pub fn with(lines: &[&str]) -> See {
        See {
            lines: lines.iter().map(|l| l.to_string()).collect(),
            ..See::default()
        }
    }

    /// Does the output contain `needle`?
    pub fn heard(&self, needle: &str) -> bool {
        self.lines.iter().any(|line| line.contains(needle))
    }

    /// The first line that contains `needle`.
    pub fn line(&self, needle: &str) -> Option<&str> {
        self.lines
            .iter()
            .find(|line| line.contains(needle))
            .map(|line| line.as_str())
    }
}

/// What a solver wants next.
///
/// **ESTABLISHED (shape)**: the returned value is 32 bytes — a discriminant plus
/// a `String` — and the non-`String` variants use the niche word
/// `isize::MIN + n` (`search_rooms::SearchRooms::next` writes
/// `0x800000000000000A`, `0x8000000000000009` and `0x8000000000000002`, and
/// `solver_for`'s intermediate tags run `…0001` to `…000b`). The variant *names*
/// are not recoverable; the names below are this build's, and the doc comments
/// record which recovered tag each corresponds to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// Send this command and keep going (the `String` variants).
    Say(String),
    /// Two commands at once (e.g. `prepare` then `cast`).
    SayBoth(String, String),
    /// Nothing to do this turn; wait for more output (`MIN + 2`, the tag
    /// `search_rooms::next` returns when it runs out of rooms).
    Wait,
    /// The room is done; carry on with the walk (`MIN + 9`, the tag
    /// `search_rooms::next` returns when its search flag is already set).
    Done,
    /// The routine cannot be solved from here (`MIN + 10`).
    Stuck,
}

impl Next {
    /// The command(s) this step carries.
    pub fn commands(&self) -> Vec<&str> {
        match self {
            Next::Say(one) => vec![one.as_str()],
            Next::SayBoth(a, b) => vec![a.as_str(), b.as_str()],
            _ => vec![],
        }
    }
}

/// A puzzle solver.
pub trait Solver {
    /// The next step.
    fn next(&mut self, see: &See) -> Next;

    /// Does this line of output belong to the routine?
    ///
    /// The original's default impls (`0x101523b00`, `0x1015337e4`, …) return
    /// `false`; every solver that cares overrides it.
    fn keep(&mut self, _line: &str) -> bool {
        false
    }

    /// The routine's kind word.
    fn kind(&self) -> &'static str;
}

/// Lines the walker has already answered, kept so a routine does not answer
/// twice.
///
/// `routines::Seen::answered` (`0x10156ca94`) walks a `Vec<ChunkLine>` (stride
/// `0x90`), calls `ChunkLine::text` on each and reports whether any line
/// contains the needle (`is_contained_in` at `0x101567888`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    pub lines: Vec<String>,
}

impl Seen {
    pub fn push(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    pub fn answered(&self, needle: &str) -> bool {
        self.lines.iter().any(|line| line.contains(needle))
    }
}

/// Does the walker carry one of these nouns (`routines::holds`,
/// `0x10156cb54`)?
pub fn holds(hands: &[Option<String>], noun: &str) -> bool {
    hands
        .iter()
        .flatten()
        .any(|held| held.eq_ignore_ascii_case(noun))
}

/// Build the solver for one room.
///
/// **Partly recovered** (`0x10156b7a0`): the room lookup, the two jump tables
/// and the `DayPass::new` / `SwordGorge::new` calls are established, and so is
/// the `Isle of Four Winds` / `Mist Harbor` string test; the mapping from the
/// intermediate tag (`isize::MIN + 1 … + 0xb`, twelve arms) to the concrete
/// solver could not be enumerated from the jump tables, so the dispatch below is
/// on the routine's `kind` word instead — the same information the map data
/// carries.
pub fn solver_for(
    map: &crate::map::Map,
    room: RoomId,
    routine: &Routine,
) -> Option<Box<dyn Solver>> {
    // The recovered body starts by looking the room up; a routine for a room the
    // map does not have cannot be solved.
    let place = map.room(room)?;
    let kind = routine
        .kind
        .strip_prefix("giant_")
        .or_else(|| routine.kind.strip_prefix("cutter_"))
        .map(|rest| {
            if kind_starts_with_giant(routine) {
                "giant"
            } else {
                "cutter"
            }
        })
        .unwrap_or(routine.kind.as_str());
    let _ = place;
    let solver: Box<dyn Solver> = match kind {
        "confluence" => Box::new(confluence::Confluence::new(routine.clone())),
        "minotaur_maze" => Box::new(minotaur_maze::MinotaurMaze::new(routine.clone())),
        "signposts" => Box::new(signposts::Signposts::new(routine.clone())),
        "seeking" => Box::new(seeking::Seeking::new(routine.clone())),
        "trinket" => Box::new(trinket::Trinket::new(routine.clone())),
        "guild_password" => Box::new(guild_password::GuildPassword::new(routine.clone())),
        "flight_of_steps" => Box::new(flight_of_steps::FlightOfSteps::new(routine.clone())),
        "search_rooms" => Box::new(search_rooms::SearchRooms::new(routine.clone())),
        "mirror" => Box::new(mirror::Mirror::new(routine.clone())),
        "ring_wedges" => Box::new(ring_wedges::RingWedges::new(routine.clone())),
        "bronze_gate" => Box::new(bronze_gate::BronzeGate::new(routine.clone())),
        "colour_barrier" => Box::new(colour_barrier::ColourBarrier::new(routine.clone())),
        "patrol" => Box::new(patrol::Patrol::new(routine.clone())),
        "day_pass" => Box::new(day_pass::DayPass::new(routine.clone())),
        "giant" => Box::new(giant::Giant::new(routine.clone())),
        "cutter" => Box::new(cutter::Cutter::new(routine.clone())),
        "sword_in_the_gorge" => Box::new(sword_gorge::SwordGorge::new(routine.clone())),
        "rolaren_gate" => Box::new(rolaren_gate::RolarenGate::new(routine.clone())),
        "rune_staircase" => Box::new(rune_staircase::RuneStaircase::new(routine.clone())),
        "three_pillars" => Box::new(three_pillars::ThreePillars::new(routine.clone())),
        "vaalorn_door" => Box::new(vaalorn_door::VaalornDoor::new(routine.clone())),
        "altar_levers" => Box::new(altar_levers::AltarLevers::new(routine.clone())),
        "eye_spy_runes" => Box::new(eye_spy_runes::EyeSpyRunes::new(routine.clone())),
        "crown_door" => Box::new(crown_door::CrownDoor::new(routine.clone())),
        "bridge_wheel" => Box::new(bridge_wheel::BridgeWheel::new(routine.clone())),
        "familiar_doors" => Box::new(familiar_doors::FamiliarDoors::new(routine.clone())),
        "labyrinth_entry" => Box::new(labyrinth_entry::LabyrinthEntry::new(routine.clone())),
        "mural_of_deities" => Box::new(mural_of_deities::MuralOfDeities::new(routine.clone())),
        "workshop_pillars" => {
            Box::new(workshop_pillars::WorkshopPillars::new(routine.clone()))
        }
        _ => return None,
    };
    Some(solver)
}

fn kind_starts_with_giant(routine: &Routine) -> bool {
    routine.kind.starts_with("giant_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Map, Room};

    #[test]
    fn the_thirty_one_kind_words_are_the_pool_literals() {
        assert_eq!(KINDS.len(), 31);
        assert_eq!(KINDS[0], "confluence");
        assert_eq!(KINDS[13], "day_pass");
        assert_eq!(KINDS[18], "sword_in_the_gorge");
        assert_eq!(KINDS[30], "workshop_pillars");
        for kind in KINDS {
            assert!(!kind.is_empty());
        }
    }

    #[test]
    fn solver_for_builds_each_kind() {
        let map = Map::new(vec![Room {
            id: 7,
            number: "7".into(),
            title: "Talondown Arena".into(),
            area: "Talondown".into(),
            exits: vec![],
            routines: vec![],
        }]);
        for kind in KINDS {
            let routine = Routine::named(kind);
            let solver = solver_for(&map, 7, &routine)
                .unwrap_or_else(|| panic!("no solver for {kind}"));
            assert!(!solver.kind().is_empty());
        }
        // A room the map does not have has no solver, and neither has an
        // unknown kind.
        assert!(solver_for(&map, 99, &Routine::named("day_pass")).is_none());
        assert!(solver_for(&map, 7, &Routine::named("nope")).is_none());
    }

    #[test]
    fn next_carries_its_commands() {
        assert_eq!(Next::Say("look".into()).commands(), vec!["look"]);
        assert_eq!(
            Next::SayBoth("prepare 506".into(), "cast 506".into()).commands(),
            vec!["prepare 506", "cast 506"]
        );
        assert!(Next::Wait.commands().is_empty());
    }

    #[test]
    fn seen_remembers_and_holds_finds() {
        let mut seen = Seen::default();
        seen.push("A translucent force slams into the gate.");
        assert!(seen.answered("translucent force"));
        assert!(!seen.answered("gate opens"));
        assert!(holds(&[None, Some("cheap gem".into())], "cheap gem"));
        assert!(!holds(&[None], "cheap gem"));
    }

    #[test]
    fn see_snapshots_output() {
        let see = See::with(&["You touch the mural.", "The face falls quiet."]);
        assert!(see.heard("face falls quiet"));
        assert_eq!(see.line("mural"), Some("You touch the mural."));
        assert_eq!(See::at(284).room, Some(284));
    }
}
