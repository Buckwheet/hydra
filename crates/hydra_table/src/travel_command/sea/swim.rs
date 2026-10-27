//! Swimming a sea crossing.
//!
//! Recovered from `hydra_table::travel_command::sea::swim` (`0x10120ea58`,
//! `0x10120ebf8`).  [`stroke_again`] is recovered completely — it is a
//! direction→canonical-direction table in `__DATA` at `0x10442a570` … `0x10442a5e0`
//! with `north` as the fallthrough (`0x10120eb88` returns the 5-byte piece at
//! `0x102407f57`).  [`crossing`] is the 17 KB crossing state machine and needs
//! `hydra_travel::route::{destination, itinerary}` and `hydra_map::ocean::ocean`,
//! none of which is reconstructed.

/// The swim directions, in the order the binary's `&'static str` table holds
/// them (`0x10442a570` … `0x10442a5e0`).
///
/// **ESTABLISHED**: the pairs are `north`, `northeast`, `east`, `southeast`,
/// `south`, `southwest`, `west`, `northwest`.
pub const DIRECTIONS: [&str; 8] = [
    "north",
    "northeast",
    "east",
    "southeast",
    "south",
    "southwest",
    "west",
    "northwest",
];

/// The direction used when the stroke names none of the eight.
///
/// **ESTABLISHED** (`0x10120eb88`): `0x102407f57` with length 5 = `north`.
pub const DEFAULT_DIRECTION: &str = "north";

/// The travel setting that asks the swimmer to conserve strength.
///
/// **ESTABLISHED**: the 8-byte literal at `0x1024077a8` next to `swimming`
/// (`0x1024077a0`), used by [`crossing`].
pub const CONSERVE: &str = "conserve";

/// The travel setting that enables swimming at all.
///
/// **ESTABLISHED**: the 8-byte literal at `0x1024077a0`.
pub const SWIMMING: &str = "swimming";

/// The canonical direction for a swim stroke, or `north`.
///
/// **ESTABLISHED** (`stroke_again`, `0x10120ea58`): a `match` on the string's
/// length — `4` compares `east`/`west`, `5` compares `north`/`south`, `9`
/// compares `northeast`/`southeast`/`southwest`/`northwest` (each as one 8-byte
/// word plus the ninth byte) — and every arm returns the *same* `&'static str`
/// from the table.  Anything else returns `north`.
///
/// So "stroke again" means "keep stroking in the same direction"; the function
/// exists to turn a parsed direction into the static string the walker stores.
pub fn stroke_again(direction: &str) -> &'static str {
    match direction.len() {
        4 => match direction {
            "east" => "east",
            "west" => "west",
            _ => DEFAULT_DIRECTION,
        },
        5 => match direction {
            "north" => "north",
            "south" => "south",
            _ => DEFAULT_DIRECTION,
        },
        9 => match direction {
            "northeast" => "northeast",
            "southeast" => "southeast",
            "southwest" => "southwest",
            "northwest" => "northwest",
            _ => DEFAULT_DIRECTION,
        },
        _ => DEFAULT_DIRECTION,
    }
}

/// One crossing of the open sea.
///
/// **ESTABLISHED** (`crossing`, `0x10120ebf8`): the crossing asks
/// `hydra_travel::route::destination` (`0x10154ebc4`) then
/// `hydra_travel::route::itinerary` (`0x1015517e4`) four times over, and reads
/// the `swimming`/`conserve` travel settings.
///
/// **INFERRED**: the crossing is modelled as the list of strokes it types, one
/// per room of sea, all in [`stroke_again`]'s direction; `conserve` drops every
/// other stroke (the original's "conserve" setting does something of that
/// shape — the exact rule is not recoverable without `hydra_travel`).
pub fn crossing(direction: &str, rooms: u32, conserve: bool) -> Vec<String> {
    let stroke = stroke_again(direction);
    let mut strokes = Vec::new();
    let mut left = rooms;
    while left > 0 {
        strokes.push(stroke.to_string());
        left -= 1;
        if conserve && left > 0 {
            // Swim, pause, swim: the pause is what conserves strength.
            left -= 1;
        }
    }
    strokes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_direction_is_canonicalised_to_itself() {
        for direction in DIRECTIONS {
            assert_eq!(stroke_again(direction), direction);
        }
    }

    #[test]
    fn an_unknown_direction_falls_back_to_north() {
        assert_eq!(stroke_again("up"), DEFAULT_DIRECTION);
        assert_eq!(stroke_again(""), "north");
        assert_eq!(stroke_again("NORTH"), "north", "the comparison is exact");
        assert_eq!(stroke_again("easterly"), "north");
    }

    #[test]
    fn the_direction_table_is_the_recovered_one() {
        assert_eq!(DIRECTIONS.len(), 8);
        assert_eq!(DIRECTIONS[0], "north");
        assert_eq!(DIRECTIONS[7], "northwest");
        assert_eq!(DEFAULT_DIRECTION.len(), 5, "the fallback piece is 5 bytes");
    }

    #[test]
    fn a_crossing_strokes_once_per_room() {
        assert_eq!(crossing("south", 3, false), vec!["south", "south", "south"]);
        assert_eq!(crossing("up", 2, false), vec!["north", "north"]);
        assert!(crossing("east", 0, false).is_empty());
        // Conserving halves the strokes it takes.
        assert_eq!(crossing("east", 4, true).len(), 2);
    }
}
