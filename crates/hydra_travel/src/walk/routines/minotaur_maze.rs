//! The `minotaur_maze` routine.
//!
//! **Recovered:** `0x10151ed40` with `MinotaurMaze::choosing`: the familiar commands and the `You sense confusion` / `You sense understanding` / `just squeezed between the stone doors` / `Obvious exits` / `Obvious paths` literals at `0x10241c123`.

use super::{Next, Routine, See, Solver};

/// The `minotaur_maze` solver.
#[derive(Clone, Debug, Default)]
pub struct MinotaurMaze {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl MinotaurMaze {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "minotaur_maze";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "tell familiar to watch",
        "tell familiar to return",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "You sense confusion",
        "You sense understanding",
        "just squeezed between the stone doors",
        "Obvious exits",
        "Obvious paths",
    ];

    pub fn new(routine: Routine) -> MinotaurMaze {
        MinotaurMaze {
            routine,
            step: 0,
        }
    }
}

impl Solver for MinotaurMaze {
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
        let solver = MinotaurMaze::new(Routine::named(MinotaurMaze::KIND));
        assert_eq!(solver.kind(), "minotaur_maze");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = MinotaurMaze::new(Routine::named(MinotaurMaze::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, MinotaurMaze::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = MinotaurMaze::new(Routine::named(MinotaurMaze::KIND));
        for needle in MinotaurMaze::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
