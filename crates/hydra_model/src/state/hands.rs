//! Held items.
//!
//! `Hand::read` (4260 B) folds `<held>`/`<right>`/`<left>` blocks in; its
//! literals include the bounty/skinning texts (`"It appears they have a
//! creature problem they'd like you t…"`), because the same reader also picks
//! up the task text.

/// One hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HandSide {
    Left,
    Right,
}

/// What the character is holding (`Hand` in the recovered API).
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Hand {
    pub left: Option<String>,
    pub right: Option<String>,
}

impl Hand {
    /// @0x0000000101bbaedc (4260 B)
    pub fn read(&mut self, line: &str) -> bool {
        let (slot, value) = if let Some((_, rest)) = line.split_once("<left>") {
            (HandSide::Left, inner(rest))
        } else if let Some((_, rest)) = line.split_once("<right>") {
            (HandSide::Right, inner(rest))
        } else if let Some((_, rest)) = line.split_once("<held>") {
            (HandSide::Right, inner(rest))
        } else {
            return false;
        };
        let target = match slot {
            HandSide::Left => &mut self.left,
            HandSide::Right => &mut self.right,
        };
        if *target == value {
            return false;
        }
        *target = value;
        true
    }

    pub fn left(&self) -> Option<&str> {
        self.left.as_deref()
    }

    pub fn right(&self) -> Option<&str> {
        self.right.as_deref()
    }

    pub fn empty(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }
}

fn inner(rest: &str) -> Option<String> {
    let text = rest.split("</").next().unwrap_or(rest).trim();
    if text.is_empty() || text.eq_ignore_ascii_case("nothing") || text.eq_ignore_ascii_case("empty")
    {
        None
    } else {
        Some(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hands_are_tracked_separately() {
        let mut hands = Hand::default();
        assert!(hands.read("<left>a shield</left>"));
        assert!(hands.read("<right>a sword</right>"));
        assert_eq!(hands.left(), Some("a shield"));
        assert_eq!(hands.right(), Some("a sword"));
        assert!(!hands.empty());
    }

    #[test]
    fn empty_hands_are_recorded_as_none() {
        let mut hands = Hand::default();
        hands.read("<right>a sword</right>");
        assert!(hands.read("<right>empty</right>"));
        assert_eq!(hands.right(), None);
    }

    #[test]
    fn unrelated_lines_change_nothing() {
        let mut hands = Hand::default();
        assert!(!hands.read("You stand still."));
        assert!(hands.empty());
    }
}
