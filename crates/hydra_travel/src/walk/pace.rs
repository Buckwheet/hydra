//! How far ahead of the game a walk sends its moves.
//!
//! Recovered from `hydra_travel::walk::pace::{typeahead_of, typeahead_said,
//! Batch::landed}` (`0x10156aff4`, `0x10156b3cc`, `0x10156b67c`) and
//! `hydra_travel::walk::pace::<impl Trip>::send_ahead` (`0x101543c30`).

/// The `typeahead` setting read as a count (`typeahead_of`,
/// `0x10156aff4`).
///
/// **ESTABLISHED (literals)**: the setting's own description says *How many
/// moves the game holds beyond the one it is doing: plain moves go 1 + this at a
/// time. 0 for one move at a time.* (`0x10241d004`), and its default is `2`.
pub fn typeahead_of(setting: Option<u32>) -> u32 {
    setting.unwrap_or(2).min(9)
}

/// The confirmation a typeahead change prints (`typeahead_said`,
/// `0x10156b3cc`).
pub fn typeahead_said(setting: u32) -> String {
    format!("typeahead is {setting}.")
}

/// A batch of moves sent ahead of the game's answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    /// The moves in flight.
    pub moves: Vec<String>,
    /// How many have landed.
    pub landed: u32,
}

impl Batch {
    /// How many moves may be in flight: `1 + typeahead`.
    pub fn width(setting: u32) -> usize {
        1 + typeahead_of(Some(setting)) as usize
    }

    /// Add a move if there is room.
    pub fn push(&mut self, command: &str, setting: u32) -> bool {
        if self.moves.len() >= Batch::width(setting) {
            return false;
        }
        self.moves.push(command.to_string());
        true
    }

    /// The game answered one move (`Batch::landed`, `0x10156b67c`).
    pub fn landed(&mut self) -> Option<String> {
        if self.moves.is_empty() {
            return None;
        }
        self.landed += 1;
        Some(self.moves.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typeahead_defaults_to_two() {
        assert_eq!(typeahead_of(None), 2);
        assert_eq!(typeahead_of(Some(0)), 0);
        assert_eq!(typeahead_of(Some(99)), 9);
        assert_eq!(typeahead_said(4), "typeahead is 4.");
        assert_eq!(Batch::width(2), 3);
    }

    #[test]
    fn a_batch_holds_one_plus_typeahead() {
        let mut batch = Batch::default();
        assert!(batch.push("north", 1));
        assert!(batch.push("north", 1));
        assert!(!batch.push("north", 1));
        assert_eq!(batch.landed().as_deref(), Some("north"));
        assert_eq!(batch.landed(), Some("north".to_string()));
        assert_eq!(batch.landed(), None);
    }
}
