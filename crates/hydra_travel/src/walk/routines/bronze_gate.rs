//! The `bronze_gate` routine.
//!
//! **Recovered:** `0x101569128`: the gate answers `creak loudly as they give way` and `It's opened wide enough to slip through now` / `through a massive bronze gate.` (`0x102420953`), and a failed push answers `A translucent force slams into the gate, but it remains unaffected`.

use super::{Next, Routine, See, Solver};

/// The `bronze_gate` solver.
#[derive(Clone, Debug, Default)]
pub struct BronzeGate {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl BronzeGate {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "bronze_gate";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "push bronze gate",
        "batter gate",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "creak loudly as they give way",
        "It's opened wide enough to slip through now",
        "through a massive bronze gate.",
        "A translucent force slams into the gate, but it remains unaffected",
    ];

    pub fn new(routine: Routine) -> BronzeGate {
        BronzeGate {
            routine,
            step: 0,
        }
    }
}

impl Solver for BronzeGate {
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
        let solver = BronzeGate::new(Routine::named(BronzeGate::KIND));
        assert_eq!(solver.kind(), "bronze_gate");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = BronzeGate::new(Routine::named(BronzeGate::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, BronzeGate::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = BronzeGate::new(Routine::named(BronzeGate::KIND));
        for needle in BronzeGate::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
