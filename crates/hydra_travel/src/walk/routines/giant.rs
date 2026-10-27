//! The `giant` routine.
//!
//! **Recovered:** `0x101535614`: the amulet messages at `0x10241dd97` (`a sequence`, `a map`, `crystal amulet`, `piece of sun-washed driftwood`, `driftwood`, `You have no crystal amulet for the giant. Buy one, o...`).
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `giant` solver.
#[derive(Clone, Debug, Default)]
pub struct Giant {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl Giant {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "giant";

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "You have no crystal amulet for the giant. Buy one",
    ];

    pub fn new(routine: Routine) -> Giant {
        Giant {
            routine,
            step: 0,
        }
    }
}

impl Solver for Giant {
    fn next(&mut self, _see: &See) -> Next {
        Next::Wait
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
        let solver = Giant::new(Routine::named(Giant::KIND));
        assert_eq!(solver.kind(), "giant");
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = Giant::new(Routine::named(Giant::KIND));
        for needle in Giant::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
