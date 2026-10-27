//! The `crown_door` routine.
//!
//! **Recovered:** `0x101548d74`: `You open` (`0x10241bf70`), `go door`, and the town words `wehnimer`, `halfling`, `solhaven` and `day pass` that decide which door is opened (`CrownDoor::{cast,push,finish}` at `0x101549888`, `0x101549d28`, `0x10154a148`).

use super::{Next, Routine, See, Solver};

/// The `crown_door` solver.
#[derive(Clone, Debug, Default)]
pub struct CrownDoor {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl CrownDoor {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "crown_door";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "go door",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "You open",
    ];

    pub fn new(routine: Routine) -> CrownDoor {
        CrownDoor {
            routine,
            step: 0,
        }
    }
}

impl Solver for CrownDoor {
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
        let solver = CrownDoor::new(Routine::named(CrownDoor::KIND));
        assert_eq!(solver.kind(), "crown_door");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = CrownDoor::new(Routine::named(CrownDoor::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, CrownDoor::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = CrownDoor::new(Routine::named(CrownDoor::KIND));
        for needle in CrownDoor::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
