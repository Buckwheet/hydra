//! Movement-line classification.
//!
//! `movement::classify` (25348 B at `0x101b94dd0`) is the largest pure
//! classifier outside `spells::expr`. Recovered evidence: it builds a
//! `Regex` from `pattern::ascii` output, uses `Regex::create_captures`, walks
//! the capture list with `DebugList`, and its literals include the direction
//! verbs plus one movement phrase
//! (`"Elbow shatters under sudden decompression.\nvacuum\tleft_ar…"` — the
//! crit table bleeding into the same `__const` run).
//!
//! `// UNKNOWN:` the phrase table. The recovered function is regex-driven over
//! a data table that is not reachable here, so this module classifies only the
//! directions and the self/other distinction, which the literals establish.

use crate::state::departure::{is_direction, DIRECTIONS};

/// Who moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mover {
    SelfOnly,
    Other,
    Unknown,
}

/// One movement reading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Movement {
    pub mover: Mover,
    /// The direction, when it is one of the recovered compass words.
    pub direction: Option<&'static str>,
    /// The mover's name, when it is not the player.
    pub who: Option<String>,
}

/// @0x0000000101b94dd0 (25348 B)
pub fn classify(line: &str) -> Option<Movement> {
    let line = line.trim();
    let (mover, rest) = if let Some(rest) = line.strip_prefix("You ") {
        (Mover::SelfOnly, rest)
    } else {
        (Mover::Other, line)
    };
    // The direction is the last compass word on the line.
    let direction = line
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_lowercase())
        .filter_map(|word| {
            DIRECTIONS
                .iter()
                .copied()
                .find(|direction| *direction == word)
        })
        .next_back();
    if direction.is_none() && !is_direction(rest.split_whitespace().last().unwrap_or("")) {
        return None;
    }
    let who = if mover == Mover::Other {
        line.split_whitespace()
            .take_while(|word| !matches!(*word, "leaves" | "arrives" | "walks" | "runs" | "goes"))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        String::new()
    };
    Some(Movement {
        mover,
        direction,
        who: (!who.is_empty()).then_some(who),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_movement_is_recognised() {
        let movement = classify("You walk north.").unwrap();
        assert_eq!(movement.mover, Mover::SelfOnly);
        assert_eq!(movement.direction, Some("north"));
    }

    #[test]
    fn other_movement_carries_the_name() {
        let movement = classify("A scruffy kobold leaves south.").unwrap();
        assert_eq!(movement.mover, Mover::Other);
        assert_eq!(movement.direction, Some("south"));
        assert_eq!(movement.who.as_deref(), Some("A scruffy kobold"));
    }

    #[test]
    fn prose_without_a_direction_is_ignored() {
        assert_eq!(classify("You stand still."), None);
    }

    #[test]
    fn every_direction_is_accepted() {
        for direction in DIRECTIONS {
            assert!(classify(&format!("You walk {direction}.")).is_some());
        }
    }
}
