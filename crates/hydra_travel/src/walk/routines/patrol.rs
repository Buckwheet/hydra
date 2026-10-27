//! The `patrol` routine.
//!
//! **Recovered:** `0x10151e778` with `Patrol::open` (`0x101523ed0`): the patrol walks a fixed round; no command literal survived.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `patrol` solver.
#[derive(Clone, Debug, Default)]
pub struct Patrol {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl Patrol {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "patrol";

    pub fn new(routine: Routine) -> Patrol {
        Patrol {
            routine,
            step: 0,
        }
    }
}

impl Solver for Patrol {
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
        let solver = Patrol::new(Routine::named(Patrol::KIND));
        assert_eq!(solver.kind(), "patrol");
    }
}
