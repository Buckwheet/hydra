//! The `trinket` routine.
//!
//! **Recovered:** `0x101559560` with `trinket::{named,at_hand}` and `Trinket::{note,take_out}`: the Mist Harbor trinket is named by the `fwi_trinket` setting and taken from its container.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `trinket` solver.
#[derive(Clone, Debug, Default)]
pub struct Trinket {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl Trinket {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "trinket";

    pub fn new(routine: Routine) -> Trinket {
        Trinket {
            routine,
            step: 0,
        }
    }
}

impl Solver for Trinket {
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
        let solver = Trinket::new(Routine::named(Trinket::KIND));
        assert_eq!(solver.kind(), "trinket");
    }
}
