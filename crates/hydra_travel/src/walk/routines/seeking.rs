//! The `seeking` routine.
//!
//! **Recovered:** `0x10152e974` with `Seeking::landed`: the `a sequence` / `a map` / `crystal amulet` / `piece of sun-washed driftwood` noun list and the giant's amulet message at `0x10241dd97`.

use super::{Next, Routine, See, Solver};

/// The `seeking` solver.
#[derive(Clone, Debug, Default)]
pub struct Seeking {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl Seeking {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "seeking";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "symbol of seeking",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "You have no crystal amulet for the giant. Buy one",
    ];

    pub fn new(routine: Routine) -> Seeking {
        Seeking {
            routine,
            step: 0,
        }
    }
}

impl Solver for Seeking {
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
        let solver = Seeking::new(Routine::named(Seeking::KIND));
        assert_eq!(solver.kind(), "seeking");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = Seeking::new(Routine::named(Seeking::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, Seeking::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = Seeking::new(Routine::named(Seeking::KIND));
        for needle in Seeking::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
