//! Turning a map step into a game command.
//!
//! Recovered from `hydra_travel::walk::drive::{routine_name,gone,is_a_move}`
//! (`0x10153831c`, `0x101538570`, `0x101538650`) and
//! `hydra_travel::walk::drive::preflight::halted` (`0x101557924`).

pub mod preflight;

use crate::map::Map;
use crate::RoomId;

/// The name a routine is shown under.
///
/// **ESTABLISHED** (`0x10153831c`): the body serialises the routine to a
/// `serde_json::Value` (inlined `Routine::serialize` + `SerializeMap`), indexes
/// it with the 4-byte key `name` (`0x10206b3a0`) through
/// `<str as Index>::index_into`, and:
///
/// * falls back to the literal **`way`** (the three bytes `w`,`a`,`y` stored
///   with `strh`+`strb` at `0x101538378`) when the value is missing or is not a
///   string;
/// * otherwise copies the string, mapping byte `0x5f` (`_`) to `0x20` (space) —
///   the `movi.16b v0, #0x5f` / `movi.16b v1, #0x20` / `cmeq` / `bit` loop at
///   `0x101538484`.
pub fn routine_name(kind: &str) -> String {
    if kind.is_empty() {
        return "way".to_string();
    }
    kind.replace('_', " ")
}

/// The command that walks from one room to the next.
///
/// **ESTABLISHED (shape)**: `is_a_move` (`0x101538650`, 31 KB) is the big
/// compare chain that turns the map's step kind into the game's word — the
/// literals it matches are the direction words, the `climb`/`go`/`swim`/`jump`
/// verbs and the door nouns. The exact per-step dispatch is not recoverable from
/// the instruction stream, so this build takes the exit word the map stores,
/// which is what the chain resolves to for a plain step.
pub fn command_for(map: &Map, from: RoomId, to: RoomId) -> String {
    if let Some(room) = map.room(from) {
        if let Some(exit) = room.exits.iter().find(|exit| exit.to == to) {
            return exit.word.clone();
        }
    }
    String::new()
}

/// Did the step go wrong (`drive::gone`, 224 bytes at `0x101538570`)?
///
/// The body only checks that the room changed; it allocates nothing and returns
/// a byte.
pub fn gone(from: RoomId, to: Option<RoomId>) -> bool {
    to != Some(from)
}

/// Should the walk halt here (`drive::preflight::halted`, 1364 bytes at
/// `0x101557924`)?
///
/// **Partly recovered**: the body formats the two messages at `0x10241f52c`
/// (`it needs a name: .go2 save den`) and matches the room's prose; this build
/// halts on the recovered "cannot move" refusals.
pub fn halted(see: &[String]) -> Option<&'static str> {
    crate::walk::CANNOT_MOVE
        .into_iter()
        .find(|refusal| see.iter().any(|line| line.contains(refusal)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Exit, Map, Room};

    #[test]
    fn routine_names_lose_their_underscores() {
        assert_eq!(routine_name("sword_in_the_gorge"), "sword in the gorge");
        assert_eq!(routine_name("day_pass"), "day pass");
        assert_eq!(routine_name(""), "way");
    }

    #[test]
    fn a_step_uses_the_maps_own_word() {
        let map = Map::new(vec![Room {
            id: 1,
            number: "1".into(),
            title: "One".into(),
            area: "A".into(),
            exits: vec![Exit {
                word: "go gate".into(),
                to: 2,
                price: None,
                when: None,
            }],
            routines: vec![],
        }]);
        assert_eq!(command_for(&map, 1, 2), "go gate");
        assert_eq!(command_for(&map, 1, 3), "");
        assert!(gone(1, Some(2)));
        assert!(!gone(1, Some(1)));
    }

    #[test]
    fn halted_reports_the_refusal() {
        let see = vec!["You'd tip the boat over!".to_string()];
        assert_eq!(halted(&see), Some("You'd tip the boat over!"));
        assert_eq!(halted(&[]), None);
    }
}
