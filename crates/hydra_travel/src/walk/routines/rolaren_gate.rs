//! The `rolaren_gate` routine.
//!
//! **Recovered:** `0x101569c28` with `RolarenGate::leave`: `open rolaren gate`, `go rolaren gate`, `climb rolaren gate`, `That is already open`, `It appears to be locked`, `The gate vibrates slightly but nothing else happens`, `A translucent force slams into the gate, but it remains unaffected`, and the long `You can't get through the rolaren gate: …` refusal (`0x102420953`, `0x102420c85`).

use super::{Next, Routine, See, Solver};

/// The `rolaren_gate` solver.
#[derive(Clone, Debug, Default)]
pub struct RolarenGate {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl RolarenGate {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "rolaren_gate";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "open rolaren gate",
        "go rolaren gate",
        "climb rolaren gate",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "That is already open",
        "It appears to be locked",
        "The gate vibrates slightly but nothing else happens",
        "A translucent force slams into the gate, but it remains unaffected",
    ];

    pub fn new(routine: Routine) -> RolarenGate {
        RolarenGate {
            routine,
            step: 0,
        }
    }
}

impl Solver for RolarenGate {
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
        Self::KEEPS.iter().any(|needle| line.contains(needle))
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
        let solver = RolarenGate::new(Routine::named(RolarenGate::KIND));
        assert_eq!(solver.kind(), "rolaren_gate");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = RolarenGate::new(Routine::named(RolarenGate::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, RolarenGate::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = RolarenGate::new(Routine::named(RolarenGate::KIND));
        for needle in RolarenGate::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
