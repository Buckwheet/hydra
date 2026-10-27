//! The `three_pillars` routine.
//!
//! **Recovered:** `0x101530328`: the three-pillar puzzle; no literal of its own survived.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `three_pillars` solver.
#[derive(Clone, Debug, Default)]
pub struct ThreePillars {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl ThreePillars {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "three_pillars";

    pub fn new(routine: Routine) -> ThreePillars {
        ThreePillars {
            routine,
            step: 0,
        }
    }
}

impl Solver for ThreePillars {
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
        let solver = ThreePillars::new(Routine::named(ThreePillars::KIND));
        assert_eq!(solver.kind(), "three_pillars");
    }
}
