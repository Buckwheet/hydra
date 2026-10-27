//! The `flight_of_steps` routine.
//!
//! **Recovered:** `0x101520b7c`: `a flight of ` (`0x10241c3b4`) and the four climbs `climb steps`, `climb second steps`, `climb third steps`, `climb fourth steps`; `climb_for` (`0x1015239ac`) picks one from the room prose.

use super::{Next, Routine, See, Solver};

/// The `flight_of_steps` solver.
#[derive(Clone, Debug, Default)]
pub struct FlightOfSteps {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl FlightOfSteps {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "flight_of_steps";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "climb steps",
        "climb second steps",
        "climb third steps",
        "climb fourth steps",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "a flight of ",
    ];

    pub fn new(routine: Routine) -> FlightOfSteps {
        FlightOfSteps {
            routine,
            step: 0,
        }
    }
}

impl Solver for FlightOfSteps {
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
        let solver = FlightOfSteps::new(Routine::named(FlightOfSteps::KIND));
        assert_eq!(solver.kind(), "flight_of_steps");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = FlightOfSteps::new(Routine::named(FlightOfSteps::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, FlightOfSteps::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = FlightOfSteps::new(Routine::named(FlightOfSteps::KIND));
        for needle in FlightOfSteps::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
