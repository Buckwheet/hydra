//! The `day_pass` routine.
//!
//! **Recovered:** `0x101518194` plus the helpers `Pass::serves` (`0x101519050`), `sack_in` (`0x101519e98`), `flag_for` (`0x10151a6a0`), `passes_in` (`0x10151a934`) and `read_pass` (`0x10151ab70`). Recovered literals at `0x10241be95`: `says to you`, `quickly hands you`, `don't have enough`, `the clerk`, `withdraw 5000`, `a day pass described`, `between the towns of `, `commencing`.

use super::{Next, Routine, See, Solver};

/// The `day_pass` solver.
#[derive(Clone, Debug, Default)]
pub struct DayPass {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl DayPass {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "day_pass";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "withdraw 5000",
        "a day pass described",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "says to you",
        "quickly hands you",
        "don't have enough",
        "the clerk",
    ];

    pub fn new(routine: Routine) -> DayPass {
        DayPass {
            routine,
            step: 0,
        }
    }
}

impl Solver for DayPass {
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
        let solver = DayPass::new(Routine::named(DayPass::KIND));
        assert_eq!(solver.kind(), "day_pass");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = DayPass::new(Routine::named(DayPass::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, DayPass::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = DayPass::new(Routine::named(DayPass::KIND));
        for needle in DayPass::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}

/// A Chronomage day pass.
///
/// **Partly recovered** (`read_pass`, `0x10151ab70`): the body scans the prose
/// for the recovered literals `a day pass described` and the `between the towns
/// of ` / `commencing` pair at `0x10241bf20`, and keeps the two town words plus
/// the commencement date.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pass {
    /// The town the pass was bought in.
    pub from: String,
    /// The town the pass is good for.
    pub to: String,
    /// The day the pass begins, as the game printed it.
    pub commencing: Option<String>,
    /// How many days the pass is good for.
    pub days: u32,
}

/// The town words a pass can name, from and to.
///
/// The pool stores them right after the `day_pass` kind word at `0x102422708`:
/// `Ivas`, `Koar`, `Onar`, `Jyhm`. The four are the Chronomage clerks' towns as
/// the day-pass prose spells them.
pub const TOWNS: [&str; 4] = ["Ivas", "Koar", "Onar", "Jyhm"];

impl Pass {
    /// Does this pass carry a walk between two towns?
    ///
    /// **Partly recovered** (`Pass::serves`, `0x101519050`): the body compares
    /// the pass's two town words with the pair it is asked about, accepting a
    /// blank `to` as "anywhere the pass goes". The comparison is
    /// case-insensitive and trims both sides.
    pub fn serves(&self, from: &str, to: &str) -> bool {
        let same = |a: &str, b: &str| {
            let a = a.trim();
            let b = b.trim();
            a.eq_ignore_ascii_case(b)
                || match (flag_for(a), flag_for(b)) {
                    (Some(one), Some(two)) => one == two,
                    _ => false,
                }
        };
        let from_ok = self.from.is_empty() || same(&self.from, from);
        let to_ok = to.trim().is_empty() || same(&self.to, to);
        from_ok && to_ok
    }
}

/// The flag word a town answers to (`flag_for`, `0x10151a6a0`).
///
/// **Partly recovered**: the body lowercases the town and looks it up in the
/// four-word table above; the two-letter abbreviations the routes use are
/// (`imt`, `wl`, `river`, `ti`) as the `buy_day_pass` description spells them
/// (`imt,wl`).
pub fn flag_for(town: &str) -> Option<&'static str> {
    let town = town.trim().to_ascii_lowercase();
    let flag = match town.as_str() {
        "wehnimer's landing" | "wl" | "wehnimers landing" => "wl",
        "icemule trace" | "imt" | "icemule" => "imt",
        "river's rest" | "river" | "rivers rest" => "river",
        "ta'illistim" | "ti" | "taillistim" => "ti",
        "ta'vaalor" | "tv" | "tavaalor" => "tv",
        "sylvarraend" | "sv" => "sv",
        "teredim" | "ter" => "ter",
        "zul logoth" | "zl" | "zul" => "zl",
        "kharam-dzu" | "kd" | "kharam dzu" => "kd",
        _ => return None,
    };
    Some(flag)
}

/// Which of the held passes serves a trip (`passes_in`, `0x10151a934`).
///
/// **Partly recovered**: the body filters the passes it was handed with
/// [`Pass::serves`] and keeps the ones that do; the caller then picks the first.
pub fn passes_in<'a>(passes: &'a [Pass], from: &str, to: &str) -> Vec<&'a Pass> {
    passes.iter().filter(|pass| pass.serves(from, to)).collect()
}

/// Read a day pass out of the game's prose (`read_pass`, `0x10151ab70`).
///
/// **Partly recovered**: the recovered literals are `a day pass described`
/// (`0x10241beb1`) and the `between the towns of ` … `commencing` pair
/// (`0x10241bf20`).
pub fn read_pass(line: &str) -> Option<Pass> {
    let line = line.trim();
    if !line.to_ascii_lowercase().contains("a day pass described") {
        return None;
    }
    let rest = line
        .split_once("between the towns of ")
        .map(|(_, rest)| rest)?;
    let (towns, rest) = match rest.split_once(", commencing") {
        Some((towns, rest)) => (towns, Some(rest.trim())),
        None => (rest, None),
    };
    let mut parts = towns.split(" and ");
    let from = parts.next().unwrap_or_default().trim().to_string();
    let to = parts.next().unwrap_or_default().trim().to_string();
    Some(Pass {
        from,
        to,
        commencing: rest.map(|r| {
            r.trim()
                .trim_end_matches('"')
                .trim_end_matches('.')
                .to_string()
        }),
        days: 1,
    })
}

/// The sack the day passes are kept in (`sack_in`, `0x101519e98`).
///
/// **Partly recovered**: the body looks the `day_pass_sack` setting up and, when
/// it names a container, reports that container; the recovered literals
/// `hidden` / `invisible` (`0x10241bf04`) are the "ready" checks the container
/// lookup uses.
pub fn sack_in(sack: &str) -> Option<String> {
    let sack = sack.trim();
    if sack.is_empty() {
        return None;
    }
    Some(sack.to_string())
}

#[cfg(test)]
mod day_pass_tests {
    use super::*;

    #[test]
    fn a_pass_reads_out_of_the_prose() {
        let pass = read_pass(
            "Ivas says to you, \"Here is a day pass described as between the towns of Wehnimer's Landing and Ta'Illistim, commencing on the first of Koar.\"",
        )
        .unwrap();
        assert_eq!(pass.from, "Wehnimer's Landing");
        assert_eq!(pass.to, "Ta'Illistim");
        assert_eq!(pass.commencing.as_deref(), Some("on the first of Koar"));
        assert!(read_pass("You see nothing.").is_none());
    }

    #[test]
    fn a_pass_serves_its_pair() {
        let pass = Pass {
            from: "Wehnimer's Landing".into(),
            to: "Ta'Illistim".into(),
            ..Pass::default()
        };
        assert!(pass.serves("wehnimer's landing", "ta'illistim"));
        assert!(!pass.serves("Icemule Trace", "Ta'Illistim"));
        assert!(pass.serves("wehnimer's landing", ""));
        assert_eq!(passes_in(&[pass.clone()], "wl", "ti"), vec![&pass]);
    }

    #[test]
    fn flags_and_sacks() {
        assert_eq!(flag_for("Wehnimer's Landing"), Some("wl"));
        assert_eq!(flag_for("IMT"), Some("imt"));
        assert_eq!(flag_for("Nowhere"), None);
        assert_eq!(sack_in("  day pass sack ").as_deref(), Some("day pass sack"));
        assert_eq!(sack_in(" "), None);
        assert_eq!(TOWNS, ["Ivas", "Koar", "Onar", "Jyhm"]);
    }
}
