//! Multi-pattern index over the crit-table `pattern` column.
//!
//! Recovered: `MatchIndex::build` (`0x101b9b0d4`) walks the pattern slice and
//! pushes one 64-byte record per pattern into a `Vec` — a compiled
//! `regex::Regex` plus an optional second one — it does *not* build a
//! `RegexSet` (the `RegexSetBuilder` symbols in the binary belong to
//! `hydra_lookout`). For every pattern it asks
//! `str::contains("(?!.*removes skull.)")` (`0x101b9b1cc`, 20-byte needle at
//! `0x1039f894`) and, when that literal is present, replaces it with the empty
//! string (`str::replace` with `to` = `{ptr: 1, len: 0}`, `0x101b9b1f8`) and
//! separately compiles the 16-byte literal `.*removes skull.` (`0x1039f840`)
//! with [`crate::pattern::compile_gated`] as a *negative gate*; the pattern
//! itself then also goes through `compile_gated`. `Option<Regex>` is `None`
//! when that build fails, which means "no gate".
//!
//! `matches` (`0x101b9bd88`) lowercases the line (`str::to_lowercase`) and
//! reports every index whose pattern matches and whose gate does not.
//!
//! The one row that uses this is the embedded table's
//! `slash/head/3`: `^(?!.*removes skull.)Blow to head.` — it must not fire on
//! `^Blow to head removes skull.` (`impact/head/9`), which is the row before it
//! in table order.

use crate::pattern::{compile, compile_gated, Pattern};

/// The look-ahead literal `MatchIndex::build` special-cases
/// (`0x1039f894`, 20 bytes).
pub const SKULL_GATE: &str = "(?!.*removes skull.)";

/// The 16-byte gate it compiles instead (`0x1039f840`).
pub const SKULL_GATE_INNER: &str = ".*removes skull.";

/// A compiled set of crit-message patterns.
#[derive(Clone, Debug, Default)]
pub struct MatchIndex {
    /// The compiled `pattern` column, in table order.
    patterns: Vec<Pattern>,
    /// One optional negative gate per pattern.
    gates: Vec<Option<Pattern>>,
}

impl MatchIndex {
    /// Compile every pattern (case-insensitively, as the original lowercased
    /// both sides).
    pub fn build<S: AsRef<str>>(patterns: &[S]) -> MatchIndex {
        let mut compiled = Vec::with_capacity(patterns.len());
        let mut gates = Vec::with_capacity(patterns.len());
        for pattern in patterns {
            let mut source = pattern.as_ref().to_string();
            let mut gate = None;
            if source.contains(SKULL_GATE) {
                gate = compile_gated(SKULL_GATE_INNER);
                source = source.replace(SKULL_GATE, "");
            }
            compiled.push(compile(&format!("(?i){source}")));
            gates.push(gate);
        }
        MatchIndex {
            patterns: compiled,
            gates,
        }
    }

    /// Indices (into the slice passed to [`MatchIndex::build`]) of every
    /// pattern that matches `line`.
    pub fn matches(&self, line: &str) -> Vec<usize> {
        let line = line.to_lowercase();
        let mut out = Vec::new();
        for (index, (pattern, gate)) in self.patterns.iter().zip(self.gates.iter()).enumerate() {
            if !pattern.is_match(&line) {
                continue;
            }
            if gate.as_ref().is_some_and(|gate| gate.is_match(&line)) {
                continue;
            }
            out.push(index);
        }
        out
    }

    /// The number of indexed patterns.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// How many patterns compiled.
    pub fn compiled(&self) -> usize {
        self.patterns.iter().filter(|p| p.is_ok()).count()
    }

    /// How many patterns carry a negative gate (one, in the crit table).
    pub fn gated(&self) -> usize {
        self.gates.iter().filter(|gate| gate.is_some()).count()
    }

    /// The compiled pattern at `index`, gate included.
    pub fn pattern(&self, index: usize) -> Option<&Pattern> {
        self.patterns.get(index)
    }

    /// The negative gate compiled for `index`, when it has one.
    pub fn gate(&self, index: usize) -> Option<&Pattern> {
        self.gates.get(index).and_then(Option::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reports_indices_of_matching_patterns() {
        let index = MatchIndex::build(&[
            "^Too bad .*? ducked.",
            "^Acid burn on the right ear.",
            "^Burn to cheek.",
        ]);
        assert_eq!(index.len(), 3);
        assert_eq!(index.compiled(), 3);
        assert_eq!(index.gated(), 0);
        assert_eq!(index.matches("Acid burn on the right ear."), vec![1]);
        assert_eq!(index.matches("Too bad you ducked."), vec![0]);
        assert!(index.matches("nothing here").is_empty());
    }

    #[test]
    fn matching_is_case_insensitive() {
        let index = MatchIndex::build(&["^Acid burn on the right ear."]);
        assert_eq!(index.matches("acid burn on the right ear."), vec![0]);
        assert_eq!(index.matches("ACID BURN ON THE RIGHT EAR."), vec![0]);
    }

    #[test]
    fn overlapping_patterns_all_report() {
        let index = MatchIndex::build(&["^Head freezes", "^Head freezes solid", ".*solid.*"]);
        assert_eq!(index.matches("Head freezes solid"), vec![0, 1, 2]);
    }

    #[test]
    fn the_recovered_skull_gate_is_honoured() {
        // exactly the pair from the embedded table (rows 793 and 1368)
        let index = MatchIndex::build(&[
            "^Blow to head removes skull.",
            "^(?!.*removes skull.)Blow to head.",
        ]);
        assert_eq!(index.len(), 2);
        assert_eq!(index.compiled(), 2);
        assert_eq!(index.gated(), 1);
        // the look-ahead was stripped, so the row still compiles
        assert_eq!(
            index.gate(1).map(|gate| gate.source.as_str()),
            Some(SKULL_GATE_INNER)
        );
        assert_eq!(
            index.pattern(1).map(|pattern| pattern.source.as_str()),
            Some("(?i)^Blow to head.")
        );
        // both rows fire on the skull-cracking line
        assert_eq!(index.matches("Blow to head removes skull."), vec![0]);
        // only the plain row fires otherwise
        assert_eq!(index.matches("Blow to head."), vec![1]);
    }

    #[test]
    fn a_failing_gate_leaves_the_row_ungated() {
        // `compile_gated` is only ever handed the recovered 16-byte literal, so
        // this documents the `None` case rather than a reachable one
        assert!(compile_gated(SKULL_GATE_INNER).is_some());
        let index = MatchIndex::build(&["^plain", "^plain"]);
        assert_eq!(index.gate(0), None);
        assert_eq!(index.matches("plain"), vec![0, 1]);
    }
}
