//! The `altar_levers` routine.
//!
//! **Recovered:** `0x101549344` (11800 B): the body is a large lever state machine; no command literal of its own survived in `__cstring` (the strings around it belong to `shopping`), so the recovered part is only the trait shape.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `altar_levers` solver.
#[derive(Clone, Debug, Default)]
pub struct AltarLevers {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl AltarLevers {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "altar_levers";

    pub fn new(routine: Routine) -> AltarLevers {
        AltarLevers {
            routine,
            step: 0,
        }
    }
}

impl Solver for AltarLevers {
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
        let solver = AltarLevers::new(Routine::named(AltarLevers::KIND));
        assert_eq!(solver.kind(), "altar_levers");
    }
}
