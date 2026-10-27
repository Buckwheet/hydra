//! The `search_rooms` routine.
//!
//! **Recovered:** `0x1015542e0`: the body walks the routine's `rooms` list, and for each room that matches the walker's room scans the output chunk (stride `0x90`, `hydra_model::state::lines::ChunkLine::text`) for the routine's search text with `str::contains`; when it finds it, it sets its flag and returns the text at `+0x30`. The returned tags are the niche words `…000a`, `…0009` and `…0002` (see `Next`).
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `search_rooms` solver.
#[derive(Clone, Debug, Default)]
pub struct SearchRooms {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl SearchRooms {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "search_rooms";

    pub fn new(routine: Routine) -> SearchRooms {
        SearchRooms {
            routine,
            step: 0,
        }
    }
}

impl Solver for SearchRooms {
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
        let solver = SearchRooms::new(Routine::named(SearchRooms::KIND));
        assert_eq!(solver.kind(), "search_rooms");
    }
}
