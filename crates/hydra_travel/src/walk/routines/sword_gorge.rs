//! The `sword_in_the_gorge` routine.
//!
//! **Recovered:** `0x10155a0a0` with `SwordGorge::{new,put,stow,recover_or_rise}` (`0x101560a94`, `0x101560b20`, `0x101560bd4`, `0x1015609d4`): the gorge puzzle; the only literal is its own kind word `sword_in_the_gorge`.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `sword_in_the_gorge` solver.
#[derive(Clone, Debug, Default)]
pub struct SwordGorge {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl SwordGorge {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "sword_in_the_gorge";

    pub fn new(routine: Routine) -> SwordGorge {
        SwordGorge {
            routine,
            step: 0,
        }
    }
}

impl Solver for SwordGorge {
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
        let solver = SwordGorge::new(Routine::named(SwordGorge::KIND));
        assert_eq!(solver.kind(), "sword_in_the_gorge");
    }
}
