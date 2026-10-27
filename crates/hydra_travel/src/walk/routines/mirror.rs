//! The `mirror` routine.
//!
//! **Recovered:** `0x10156892c`: `turn mirror left`, `turn mirror right`, `tilt mirror`, `flips around`, `from the base of the statue onto the center of the statue`, and the two `The mirror won't turn any farther to the …` refusals (`0x102420750`, `0x102420890`).

use super::{Next, Routine, See, Solver};

/// The `mirror` solver.
#[derive(Clone, Debug, Default)]
pub struct Mirror {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl Mirror {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "mirror";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "turn mirror left",
        "tilt mirror",
        "turn mirror right",
        "tilt mirror",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "flips around",
        "The mirror won't turn any farther to the left",
        "The mirror won't turn any farther to the right",
    ];

    pub fn new(routine: Routine) -> Mirror {
        Mirror {
            routine,
            step: 0,
        }
    }
}

impl Solver for Mirror {
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
        let solver = Mirror::new(Routine::named(Mirror::KIND));
        assert_eq!(solver.kind(), "mirror");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = Mirror::new(Routine::named(Mirror::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, Mirror::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = Mirror::new(Routine::named(Mirror::KIND));
        for needle in Mirror::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
