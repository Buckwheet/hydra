//! Swimming: the strokes a walk uses to cross a water room.
//!
//! Recovered from `hydra_travel::swim::{toward,strokes}`
//! (`0x101532e88`, `0x101533020`).

/// The eight directions the swim table knows, in the order the static
/// `&[&str]` array at `0x10443a128` stores them, as (pointer, length) pairs:
///
/// | index | literal at | length | word |
/// |---|---|---|---|
/// | 0 | `0x10241d746` | 5 | `south` |
/// | 1 | `0x10241d85b` | 5 | `north` |
/// | 2 | `0x10241da79` | 9 | `southwest` |
/// | 3 | `0x10241d754` | 9 | `northeast` |
/// | 4 | `0x10241d75d` | 4 | `west` |
/// | 5 | `0x10241d905` | 4 | `east` |
/// | 6 | `0x10241d74b` | 9 | `northwest` |
/// | 7 | `0x10241d76b` | 9 | `southeast` |
pub const DIRECTIONS: [&str; 8] = [
    "south",
    "north",
    "southwest",
    "northeast",
    "west",
    "east",
    "northwest",
    "southeast",
];

/// Which way to swim for a given exit direction.
///
/// **ESTABLISHED** (`0x101532e88`): the body dispatches on the direction's byte
/// length (`4`, `5`, `9`) and compares the bytes inline. The table is stored as
/// *opposite pairs* — `(south, north)`, `(southwest, northeast)`, `(west,
/// east)`, `(northwest, southeast)` — and the recovered `"south"` arm returns
/// element **1** (`ldp x0, x1, [x8, #0x10]`, i.e. `north`), so the answer is the
/// opposite word of the pair. Every other arm follows the same pairing.
pub fn toward(direction: &str) -> Option<&'static str> {
    let direction = direction.trim();
    let pairs = [
        ("south", "north"),
        ("southwest", "northeast"),
        ("west", "east"),
        ("northwest", "southeast"),
    ];
    for (a, b) in pairs {
        if direction.eq_ignore_ascii_case(a) {
            return Some(b);
        }
        if direction.eq_ignore_ascii_case(b) {
            return Some(a);
        }
    }
    None
}

/// The commands that carry a swimmer across.
///
/// **Partly recovered** (`0x101533020`, 8644 bytes): the body is a state machine
/// over the room's prose. The recovered literals inside it include the swim
/// verb, the roundtime notice (`Cast Roundtime`) and the fatigue/drowning
/// messages; the exact stroke count and the failure predicate are not
/// recoverable from the instruction stream. This build issues `swim <toward>`
/// once per stroke until the room changes, which is the shape the recovered
/// roundtime handling implies.
pub fn strokes(toward: &str) -> Vec<String> {
    vec![format!("swim {toward}")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_the_recovered_eight() {
        assert_eq!(
            DIRECTIONS,
            [
                "south",
                "north",
                "southwest",
                "northeast",
                "west",
                "east",
                "northwest",
                "southeast"
            ]
        );
    }

    #[test]
    fn toward_gives_the_opposite_of_the_pair() {
        // Recovered: "south" returns table element 1, which is "north".
        assert_eq!(toward("south"), Some("north"));
        assert_eq!(toward("north"), Some("south"));
        assert_eq!(toward("southwest"), Some("northeast"));
        assert_eq!(toward("west"), Some("east"));
        assert_eq!(toward("northwest"), Some("southeast"));
        assert_eq!(toward("up"), None);
    }

    #[test]
    fn strokes_name_the_swim_command() {
        assert_eq!(strokes("north"), vec!["swim north".to_string()]);
    }
}
