//! The `cutter` routine.
//!
//! **Recovered:** `0x101567efc`: `Cutter::{open_next,wait,looked,resume}`, `tickets_in` and `names_the_walker` — the Marshtown / River's Rest ferry cutter. No command literal of its own survived.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `cutter` solver.
#[derive(Clone, Debug, Default)]
pub struct Cutter {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl Cutter {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "cutter";

    pub fn new(routine: Routine) -> Cutter {
        Cutter {
            routine,
            step: 0,
        }
    }
}

impl Solver for Cutter {
    fn next(&mut self, _see: &See) -> Next {
        Next::Wait
    }

    fn keep(&mut self, line: &str) -> bool {
        false
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
        let solver = Cutter::new(Routine::named(Cutter::KIND));
        assert_eq!(solver.kind(), "cutter");
    }
}
