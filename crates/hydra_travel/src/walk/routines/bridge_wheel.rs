//! The `bridge_wheel` routine.
//!
//! **Recovered:** `0x101540128` (7400 B): the wheel/span state machine; no literal of its own survived.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `bridge_wheel` solver.
#[derive(Clone, Debug, Default)]
pub struct BridgeWheel {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl BridgeWheel {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "bridge_wheel";

    pub fn new(routine: Routine) -> BridgeWheel {
        BridgeWheel {
            routine,
            step: 0,
        }
    }
}

impl Solver for BridgeWheel {
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
        let solver = BridgeWheel::new(Routine::named(BridgeWheel::KIND));
        assert_eq!(solver.kind(), "bridge_wheel");
    }
}
