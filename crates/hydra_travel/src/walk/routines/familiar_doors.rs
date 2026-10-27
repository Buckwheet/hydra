//! The `familiar_doors` routine.
//!
//! **Recovered:** `0x10151f2b0` with `FamiliarDoors::{work,ending}`; the recovered literals are the `tell familiar to watch` / `tell familiar to stop watching` / `tell familiar to return` commands and the `You sense` replies (`0x10241c123`).

use super::{Next, Routine, See, Solver};

/// The `familiar_doors` solver.
#[derive(Clone, Debug, Default)]
pub struct FamiliarDoors {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl FamiliarDoors {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "familiar_doors";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "tell familiar to watch",
        "tell familiar to stop watching",
        "tell familiar to return",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "You sense",
        "You sense confusion",
    ];

    pub fn new(routine: Routine) -> FamiliarDoors {
        FamiliarDoors {
            routine,
            step: 0,
        }
    }
}

impl Solver for FamiliarDoors {
    fn next(&mut self, _see: &See) -> Next {
        // The recovered body issues the sequence above in order; the
        // predicate that decides each step has to come from the room's
        // prose, which is not recoverable.
        match Self::COMMANDS.get(self.step) {
            Some(command) => {
                self.step += 1;
                Next::Say((*command).to_string())
            }
            None => Next::Done,
        }
    }

    fn keep(&mut self, line: &str) -> bool {
        Self::KEEPS.iter().any(|needle| line.contains(needle))
    }

    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_word() {
        let solver = FamiliarDoors::new(Routine::named(FamiliarDoors::KIND));
        assert_eq!(solver.kind(), "familiar_doors");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = FamiliarDoors::new(Routine::named(FamiliarDoors::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, FamiliarDoors::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = FamiliarDoors::new(Routine::named(FamiliarDoors::KIND));
        for needle in FamiliarDoors::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
