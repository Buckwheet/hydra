//! The `ring_wedges` routine.
//!
//! **Recovered:** `0x101548fb8`: `turn ring`, `look altar`, `go step`, `go passage` and `go opening` (`0x10241e6ce`).

use super::{Next, Routine, See, Solver};

/// The `ring_wedges` solver.
#[derive(Clone, Debug, Default)]
pub struct RingWedges {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl RingWedges {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "ring_wedges";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "turn ring",
        "look altar",
        "go step",
        "go passage",
        "go opening",
    ];

    pub fn new(routine: Routine) -> RingWedges {
        RingWedges {
            routine,
            step: 0,
        }
    }
}

impl Solver for RingWedges {
    fn next(&mut self, _see: &See) -> Next {
        // The recovered body issues the sequence above in order; the
        // predicate that decides each step has to come from the room's
        // prose, which is not recoverable.
        match Self::COMMANDS.get(self.step) {
            Some(command) => {
                self.step += 1;
                Next::Say((*command).to_string())
            }
            None => Next::Done,
        }
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
        let solver = RingWedges::new(Routine::named(RingWedges::KIND));
        assert_eq!(solver.kind(), "ring_wedges");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = RingWedges::new(Routine::named(RingWedges::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, RingWedges::COMMANDS.to_vec());
    }
}
