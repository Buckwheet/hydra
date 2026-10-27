//! The `eye_spy_runes` routine.
//!
//! **Recovered:** `0x10152f68c`: `tell eye to return` (`0x10241da82`) and `Waited ten minutes for the mana to cast Eye Spy.` (`0x10241d713`); the rune words (`wy'zio`, `ag'loenaro`, `deir'cos`, `beiron'fyn`, `quiss'fyn`, `erikar'fyn`, `lorae'tyr`, `shien'tyr`, `vakragrk'tyr`) follow in the pool.

use super::{Next, Routine, See, Solver};

/// The `eye_spy_runes` solver.
#[derive(Clone, Debug, Default)]
pub struct EyeSpyRunes {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl EyeSpyRunes {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "eye_spy_runes";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "tell eye to return",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "Waited ten minutes for the mana to cast Eye Spy.",
    ];

    pub fn new(routine: Routine) -> EyeSpyRunes {
        EyeSpyRunes {
            routine,
            step: 0,
        }
    }
}

impl Solver for EyeSpyRunes {
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
        let solver = EyeSpyRunes::new(Routine::named(EyeSpyRunes::KIND));
        assert_eq!(solver.kind(), "eye_spy_runes");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = EyeSpyRunes::new(Routine::named(EyeSpyRunes::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, EyeSpyRunes::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = EyeSpyRunes::new(Routine::named(EyeSpyRunes::KIND));
        for needle in EyeSpyRunes::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
