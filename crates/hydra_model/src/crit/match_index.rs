//! Multi-pattern index over the crit-table `pattern` column.
//!
//! # Departure from the original
//!
//! `MatchIndex::build` compiled the patterns with
//! `regex::builders::string::RegexSetBuilder::build` (a `RegexSet`, so one pass
//! over the line reports every matching pattern), plus a `RegexBuilder` gated
//! by `dfa_size_limit` for the special `.*removes skull.` /
//! `(?!.*removes skull.)` rule, and lowercased everything with
//! `str::to_lowercase`. `matches` then walked the capture set.
//!
//! This build has no `regex`; [`crate::pattern::compile`] provides a matcher
//! for the syntax the embedded table uses, and the index is a plain `Vec` of
//! compiled patterns scanned in order. The observable result — *which* table
//! rows match a crit message — is the same for that subset.
//!
//! The `.*removes skull.` / `(?!.*removes skull.)` literals that
//! `MatchIndex::build` references are *part of the table's own patterns*: some
//! rows are stored as `^(?!.*removes skull.)Blow to head.` and the engine is
//! expected to honour the look-ahead. [`crate::pattern`] implements it as a
//! zero-width assertion, so no extra gate is needed here.

use crate::pattern::{compile, Pattern};

/// A compiled set of crit-message patterns.
#[derive(Clone, Debug, Default)]
pub struct MatchIndex {
    patterns: Vec<Pattern>,
}

impl MatchIndex {
    /// Compile every pattern (case-insensitively, as the original lowercased
    /// both sides).
    pub fn build<S: AsRef<str>>(patterns: &[S]) -> MatchIndex {
        let patterns = patterns
            .iter()
            .map(|p| compile(&format!("(?i){}", p.as_ref())))
            .collect();
        MatchIndex { patterns }
    }

    /// Indices (into the slice passed to [`MatchIndex::build`]) of every
    /// pattern that matches `line`.
    pub fn matches(&self, line: &str) -> Vec<usize> {
        let line = line.to_lowercase();
        self.patterns
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_match(&line))
            .map(|(i, _)| i)
            .collect()
    }

    /// The number of indexed patterns.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// How many patterns could be compiled by this engine.
    pub fn compiled(&self) -> usize {
        self.patterns.iter().filter(|p| p.is_ok()).count()
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
}
