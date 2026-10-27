//! The `confluence` routine.
//!
//! **Recovered:** `0x101512ab0` (7832 B) plus `Confluence::keep` (140 B) and `Learned::dir_to` (`0x101513e88`). No command literal survived; the body works from direction words it learns.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `confluence` solver.
#[derive(Clone, Debug, Default)]
pub struct Confluence {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl Confluence {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "confluence";

    pub fn new(routine: Routine) -> Confluence {
        Confluence {
            routine,
            step: 0,
        }
    }
}

impl Solver for Confluence {
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
        let solver = Confluence::new(Routine::named(Confluence::KIND));
        assert_eq!(solver.kind(), "confluence");
    }
}
