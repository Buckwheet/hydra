//! The `colour_barrier` routine.
//!
//! **Recovered:** `0x1015544b8`: the three commands `go grotto`, `touch crystal` and `go barrier` are the literals at `0x10241f280`..`0x10241f296`.

use super::{Next, Routine, See, Solver};

/// The `colour_barrier` solver.
#[derive(Clone, Debug, Default)]
pub struct ColourBarrier {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl ColourBarrier {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "colour_barrier";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "go grotto",
        "touch crystal",
        "go barrier",
    ];

    pub fn new(routine: Routine) -> ColourBarrier {
        ColourBarrier {
            routine,
            step: 0,
        }
    }
}

impl Solver for ColourBarrier {
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
        let solver = ColourBarrier::new(Routine::named(ColourBarrier::KIND));
        assert_eq!(solver.kind(), "colour_barrier");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = ColourBarrier::new(Routine::named(ColourBarrier::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, ColourBarrier::COMMANDS.to_vec());
    }
}
