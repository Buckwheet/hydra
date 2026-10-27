//! The `labyrinth_entry` routine.
//!
//! **Recovered:** `0x10156a770`: `0x102420760` onward — `touch green leaf`, `push bronze gate`, `You touch the `, `You shouldn't mess with that while`, `The shimmering leaves on the wall begin to fade, blending once again into the mural.`, `gold leaf` / `blighted leaf` / `outstretched hand`, and the `recite Listen to the wind in the forest,…` verse.

use super::{Next, Routine, See, Solver};

/// The `labyrinth_entry` solver.
#[derive(Clone, Debug, Default)]
pub struct LabyrinthEntry {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl LabyrinthEntry {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "labyrinth_entry";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "touch green leaf",
        "push bronze gate",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "You touch the ",
        "You shouldn't mess with that while",
        "The shimmering leaves on the wall begin to fade, blending once again into the mural.",
        "gold leaf",
        "blighted leaf",
        "outstretched hand",
    ];

    pub fn new(routine: Routine) -> LabyrinthEntry {
        LabyrinthEntry {
            routine,
            step: 0,
        }
    }
}

impl Solver for LabyrinthEntry {
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
        let solver = LabyrinthEntry::new(Routine::named(LabyrinthEntry::KIND));
        assert_eq!(solver.kind(), "labyrinth_entry");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = LabyrinthEntry::new(Routine::named(LabyrinthEntry::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, LabyrinthEntry::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = LabyrinthEntry::new(Routine::named(LabyrinthEntry::KIND));
        for needle in LabyrinthEntry::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
