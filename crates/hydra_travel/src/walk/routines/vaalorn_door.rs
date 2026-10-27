//! The `vaalorn_door` routine.
//!
//! **Recovered:** `0x10152f2bc`: `open door`, `There doesn't seem to be any way to do that.` and `The Vaalorn door opens only for a gem, and you are not holding one. Hold a cheap gem in your right hand and …` (`0x10241d657`).

use super::{Next, Routine, See, Solver};

/// The `vaalorn_door` solver.
#[derive(Clone, Debug, Default)]
pub struct VaalornDoor {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl VaalornDoor {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "vaalorn_door";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "open door",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "There doesn't seem to be any way to do that.",
        "The Vaalorn door opens only for a gem, and you are not holding one.",
    ];

    pub fn new(routine: Routine) -> VaalornDoor {
        VaalornDoor {
            routine,
            step: 0,
        }
    }
}

impl Solver for VaalornDoor {
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
        let solver = VaalornDoor::new(Routine::named(VaalornDoor::KIND));
        assert_eq!(solver.kind(), "vaalorn_door");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = VaalornDoor::new(Routine::named(VaalornDoor::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, VaalornDoor::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = VaalornDoor::new(Routine::named(VaalornDoor::KIND));
        for needle in VaalornDoor::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
