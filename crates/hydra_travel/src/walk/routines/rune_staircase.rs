//! The `rune_staircase` routine.
//!
//! **Recovered:** `0x1015308e4` with `RuneStaircase::work` and `letter_after`: `look staircase`, `go staircase`, `you could easily scramble up them to the ledge above`, `The last one, labeled `, `is directly under the `, `look runes`, `You touch the … rune.`, `rune seems to have shifted position.`, `Nothing happens` (`0x10241d7e1`, `0x10241db10`).

use super::{Next, Routine, See, Solver};

/// The `rune_staircase` solver.
#[derive(Clone, Debug, Default)]
pub struct RuneStaircase {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl RuneStaircase {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "rune_staircase";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "look staircase",
        "go staircase",
        "look runes",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "You touch the ",
        "rune seems to have shifted position.",
        "It is now lined up under the ",
        "Nothing happens",
    ];

    pub fn new(routine: Routine) -> RuneStaircase {
        RuneStaircase {
            routine,
            step: 0,
        }
    }
}

impl Solver for RuneStaircase {
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
        let solver = RuneStaircase::new(Routine::named(RuneStaircase::KIND));
        assert_eq!(solver.kind(), "rune_staircase");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = RuneStaircase::new(Routine::named(RuneStaircase::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, RuneStaircase::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = RuneStaircase::new(Routine::named(RuneStaircase::KIND));
        for needle in RuneStaircase::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
