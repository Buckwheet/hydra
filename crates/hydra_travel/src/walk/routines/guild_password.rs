//! The `guild_password` routine.
//!
//! **Recovered:** `0x101520864`: `You are currently speaking Guildspeak` (`0x10241c583`) and the `rogue_password` setting; the password itself is spoken with `say`.
//!
//! // UNKNOWN: no command literal for this routine survived in
//! `__cstring`, so `next` reports [`Next::Wait`] rather than guess a
//! command. `keep` still answers for the lines it does recognise.

use super::{Next, Routine, See, Solver};

/// The `guild_password` solver.
#[derive(Clone, Debug, Default)]
pub struct GuildPassword {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl GuildPassword {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "guild_password";

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "You are currently speaking Guildspeak",
    ];

    pub fn new(routine: Routine) -> GuildPassword {
        GuildPassword {
            routine,
            step: 0,
        }
    }
}

impl Solver for GuildPassword {
    fn next(&mut self, _see: &See) -> Next {
        Next::Wait
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
        let solver = GuildPassword::new(Routine::named(GuildPassword::KIND));
        assert_eq!(solver.kind(), "guild_password");
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = GuildPassword::new(Routine::named(GuildPassword::KIND));
        for needle in GuildPassword::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}
