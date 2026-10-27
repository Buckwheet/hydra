//! Merging repeated story lines.
//!
//! Recovered from `merge::{MergedHistory::{lines, merge_also, offer},
//! Merger::offer}`.  `Merger::offer` @0x101851a94 is 4.6 KB — the matcher that
//! decides whether a new line continues the previous one.
//!
//! The drop glue names `MergedLine` and `Recent`, so a merged history keeps
//! `Vec<MergedLine>` and a ring of recent raw lines.

/// One merged line: the text plus how many times it was seen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergedLine {
    pub text: String,
    pub times: u32,
}

impl MergedLine {
    /// The text with its repeat count, if it repeated.
    pub fn shown(&self) -> String {
        if self.times > 1 {
            format!("{} (x{})", self.text, self.times)
        } else {
            self.text.clone()
        }
    }
}

/// A merged scrollback.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergedHistory {
    lines: Vec<MergedLine>,
    /// How many distinct raw lines to remember when deciding to merge.
    merge_also: Vec<String>,
}

impl MergedHistory {
    pub fn new() -> MergedHistory {
        MergedHistory::default()
    }

    /// `@0x1018516d0` — the merged lines.
    pub fn lines(&self) -> &[MergedLine] {
        &self.lines
    }

    /// `@0x101851528` — install the additional lines that also merge.
    pub fn merge_also(&mut self, also: Vec<String>) {
        self.merge_also = also;
    }

    /// `@0x10185172c` — offer a line; returns `true` when it merged into the
    /// previous one.
    pub fn offer(&mut self, text: &str) -> bool {
        if let Some(last) = self.lines.last_mut() {
            if last.text == text || self.merge_also.iter().any(|a| a == text) {
                last.times = last.times.saturating_add(1);
                return true;
            }
        }
        self.lines.push(MergedLine {
            text: text.to_string(),
            times: 1,
        });
        false
    }
}

/// The matcher behind `merge`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Merger {
    /// The last `n` raw lines.
    recent: Vec<String>,
    /// How many lines back to look.
    pub depth: usize,
}

impl Merger {
    pub fn new(depth: usize) -> Merger {
        Merger {
            recent: Vec::new(),
            depth: depth.max(1),
        }
    }

    /// `@0x101851a94` — decide whether `text` continues a recent line.
    ///
    /// `// UNKNOWN:` the 4.6 KB body inlines the game's repetition grammar
    /// (`"You also see"`, `"N. "` counters and the creature-name tables); the
    /// rule modelled here is "the same text within `depth` lines".
    pub fn offer(&mut self, text: &str) -> bool {
        let merged = self.recent.iter().any(|r| r == text);
        if self.recent.len() == self.depth {
            self.recent.remove(0);
        }
        self.recent.push(text.to_string());
        merged
    }

    /// The recent window.
    pub fn recent(&self) -> &[String] {
        &self.recent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_lines_merge() {
        let mut h = MergedHistory::new();
        assert!(!h.offer("a gust of wind"));
        assert!(h.offer("a gust of wind"));
        assert!(h.offer("a gust of wind"));
        assert_eq!(h.lines().len(), 1);
        assert_eq!(h.lines()[0].times, 3);
        assert_eq!(h.lines()[0].shown(), "a gust of wind (x3)");
    }

    #[test]
    fn distinct_lines_do_not_merge() {
        let mut h = MergedHistory::new();
        h.offer("one");
        h.offer("two");
        assert_eq!(h.lines().len(), 2);
        assert_eq!(h.lines()[1].shown(), "two");
    }

    #[test]
    fn merge_also_widens_the_match() {
        let mut h = MergedHistory::new();
        h.merge_also(vec!["the wind howls".into()]);
        h.offer("the wind howls");
        assert!(h.offer("the wind howls"));
        // The extra list widens what the *next* line merges with, so a line on
        // the list always merges into the previous one.
        h.offer("a gust");
        assert!(h.offer("the wind howls"));
    }

    #[test]
    fn merger_keeps_a_recent_window() {
        let mut m = Merger::new(2);
        assert!(!m.offer("a"));
        assert!(!m.offer("b"));
        assert!(m.offer("a"));
        assert_eq!(m.recent(), ["b".to_string(), "a".to_string()]);
        assert!(!m.offer("c"));
        assert_eq!(m.recent(), ["a".to_string(), "c".to_string()]);
    }
}
