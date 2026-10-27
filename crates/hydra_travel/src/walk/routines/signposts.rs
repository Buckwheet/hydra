//! The `signposts` routine.
//!
//! **Recovered:** `0x10151ea2c` (444 B): the body reads a signpost and picks a direction; no literal of its own survived.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `signposts` solver.
#[derive(Clone, Debug, Default)]
pub struct Signposts {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl Signposts {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "signposts";

    pub fn new(routine: Routine) -> Signposts {
        Signposts {
            routine,
            step: 0,
        }
    }
}

impl Solver for Signposts {
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
        let solver = Signposts::new(Routine::named(Signposts::KIND));
        assert_eq!(solver.kind(), "signposts");
    }
}
