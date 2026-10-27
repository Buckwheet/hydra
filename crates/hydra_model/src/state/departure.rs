//! Departure/movement messages.
//!
//! `classify` (14496 B) is one of the largest classifiers: it matches the
//! direction phrases and calls `is_direction` (372 B). Its literals are the
//! movement phrases (`"You walk north."`, …) which live in the same
//! `__const` run as the direction names.
//!
//! `// UNKNOWN:` the full phrase table — only the directions and the generic
//! verbs are recovered here.

/// The compass directions the game uses.
pub const DIRECTIONS: [&str; 10] = [
    "north", "northeast", "east", "southeast", "south", "southwest", "west", "northwest", "up",
    "down",
];

/// @0x0000000101b? (372 B)
pub fn is_direction(word: &str) -> bool {
    let word = word.trim().to_ascii_lowercase();
    DIRECTIONS.contains(&word.as_str()) || word == "out" || word == "in"
}

/// A classified departure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Departure {
    /// The character left in this direction.
    SelfMoved(&'static str),
    /// A creature left in this direction.
    CreatureMoved { name: String, direction: String },
    /// Something arrived.
    Arrived(String),
}

/// @0x0000000101c005ec (14496 B)
pub fn classify(line: &str) -> Option<Departure> {
    let line = line.trim();
    let rest = line
        .strip_prefix("You walk ")
        .or_else(|| line.strip_prefix("You go "))
        .or_else(|| line.strip_prefix("You run "))?;
    let word = rest
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !is_direction(&word) {
        return None;
    }
    let direction = DIRECTIONS
        .iter()
        .copied()
        .find(|d| *d == word)
        .unwrap_or("north");
    Some(Departure::SelfMoved(direction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_recovered_directions_are_directions() {
        for direction in DIRECTIONS {
            assert!(is_direction(direction));
            assert!(is_direction(&direction.to_uppercase()));
        }
        assert!(is_direction("out"));
        assert!(!is_direction("sideways"));
    }

    #[test]
    fn walking_lines_are_classified() {
        assert_eq!(
            classify("You walk north."),
            Some(Departure::SelfMoved("north"))
        );
        assert_eq!(
            classify("You run southeast."),
            Some(Departure::SelfMoved("southeast"))
        );
        assert_eq!(classify("You walk sideways."), None);
        assert_eq!(classify("You stand still."), None);
    }
}
