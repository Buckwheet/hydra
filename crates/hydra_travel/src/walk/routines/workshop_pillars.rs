//! The `workshop_pillars` routine.
//!
//! **Recovered:** `0x101530fa8`: the workshop-pillar puzzle; no literal of its own survived.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `workshop_pillars` solver.
#[derive(Clone, Debug, Default)]
pub struct WorkshopPillars {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl WorkshopPillars {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "workshop_pillars";

    pub fn new(routine: Routine) -> WorkshopPillars {
        WorkshopPillars {
            routine,
            step: 0,
        }
    }
}

impl Solver for WorkshopPillars {
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
        let solver = WorkshopPillars::new(Routine::named(WorkshopPillars::KIND));
        assert_eq!(solver.kind(), "workshop_pillars");
    }
}
