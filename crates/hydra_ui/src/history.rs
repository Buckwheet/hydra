//! Command history (the up/down arrows in the input box).
//!
//! Recovered from `hydra_ui::history::*`:
//!
//! * `Kept` is a `serde` struct with **2** elements — `lines` and
//!   `max_length` (`struct Kept with 2 elements` in the pool), fetched by the
//!   two 24-byte getters `Kept::lines` @0x10193a738 and
//!   `Kept::max_length` @0x10193a720.
//! * `History::ago` @0x10193a750 is 44 bytes: how far back the cursor sits.
//! * `History::walk` @0x10193a77c moves the cursor.
//! * `History::enter` @0x10193a8f4 records a sent line.
//! * `History::set_kept` @0x10193ab38 installs the retention policy and trims.

/// How much history to keep.
///
/// The default is 200 lines; the value is what the settings page's "history"
/// row writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kept {
    /// How many lines to keep.
    pub lines: usize,
    /// The longest line to store at all.
    pub max_length: usize,
}

impl Default for Kept {
    fn default() -> Self {
        Kept {
            lines: 200,
            max_length: 1024,
        }
    }
}

impl Kept {
    /// `@0x10193a738` — the number of lines kept.
    pub fn lines(&self) -> usize {
        self.lines
    }

    /// `@0x10193a720` — the longest stored line.
    pub fn max_length(&self) -> usize {
        self.max_length
    }
}

/// The scrollback over sent commands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    /// Oldest first.
    lines: Vec<String>,
    /// The cursor: `lines.len()` means "not browsing".
    at: usize,
    kept: Kept,
}

impl History {
    pub fn new() -> History {
        History {
            lines: Vec::new(),
            at: 0,
            kept: Kept::default(),
        }
    }

    /// `@0x10193a750` — how many entries back the cursor is.
    ///
    /// Returns `0` when the cursor is at the end (not browsing).
    pub fn ago(&self) -> usize {
        self.lines.len().saturating_sub(self.at)
    }

    /// `@0x10193a8f4` — record a sent line and move the cursor to the end.
    ///
    /// Blank lines and repeats of the previous entry are skipped, and a line
    /// longer than [`Kept::max_length`] is not stored.
    pub fn enter(&mut self, line: &str) {
        self.at = self.lines.len();
        if line.trim().is_empty() || line.len() > self.kept.max_length {
            return;
        }
        if self.lines.last().map(|l| l == line).unwrap_or(false) {
            return;
        }
        self.lines.push(line.to_string());
        self.trim();
        self.at = self.lines.len();
    }

    /// `@0x10193a77c` — move the cursor by `delta` (`-1` = older).
    ///
    /// Returns the line to put in the input box, or `None` when there is
    /// nothing to offer.
    pub fn walk(&mut self, delta: isize) -> Option<&str> {
        if self.lines.is_empty() {
            return None;
        }
        let len = self.lines.len() as isize;
        let at = (self.at as isize + delta).clamp(0, len);
        self.at = at as usize;
        if self.at == self.lines.len() {
            None
        } else {
            Some(&self.lines[self.at])
        }
    }

    /// `@0x10193ab38` — install the retention policy and trim to it.
    pub fn set_kept(&mut self, kept: Kept) {
        self.kept = kept;
        self.lines.retain(|l| l.len() <= kept.max_length);
        self.trim();
        self.at = self.at.min(self.lines.len());
    }

    /// The retention policy in force.
    pub fn kept(&self) -> Kept {
        self.kept
    }

    /// The stored lines, oldest first.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    fn trim(&mut self) {
        let max = self.kept.lines.max(1);
        if self.lines.len() > max {
            let drop = self.lines.len() - max;
            self.lines.drain(..drop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_defaults_and_getters() {
        let k = Kept::default();
        assert_eq!(k.lines(), 200);
        assert_eq!(k.max_length(), 1024);
    }

    #[test]
    fn entering_records_lines() {
        let mut h = History::new();
        h.enter("look");
        h.enter("score");
        assert_eq!(h.lines(), ["look".to_string(), "score".to_string()]);
        assert_eq!(h.ago(), 0);
    }

    #[test]
    fn blank_and_repeated_lines_are_skipped() {
        let mut h = History::new();
        h.enter("look");
        h.enter("   ");
        h.enter("look");
        assert_eq!(h.lines().len(), 1);
    }

    #[test]
    fn walking_moves_the_cursor() {
        let mut h = History::new();
        h.enter("one");
        h.enter("two");
        h.enter("three");
        assert_eq!(h.walk(-1), Some("three"));
        assert_eq!(h.walk(-1), Some("two"));
        assert_eq!(h.walk(-1), Some("one"));
        // Already at the oldest entry.
        assert_eq!(h.walk(-1), Some("one"));
        assert_eq!(h.walk(1), Some("two"));
        assert_eq!(h.walk(1), Some("three"));
        // Past the end: nothing to offer.
        assert_eq!(h.walk(1), None);
        assert_eq!(h.ago(), 0);
    }

    #[test]
    fn walking_an_empty_history_is_none() {
        let mut h = History::new();
        assert_eq!(h.walk(-1), None);
        assert_eq!(h.walk(1), None);
    }

    #[test]
    fn kept_trims_and_filters() {
        let mut h = History::new();
        for i in 0..10 {
            h.enter(&format!("line{i}"));
        }
        h.enter(&"x".repeat(40));
        h.set_kept(Kept {
            lines: 3,
            max_length: 16,
        });
        // The long line is dropped, then only three remain.
        assert_eq!(h.lines().len(), 3);
        assert_eq!(
            h.lines(),
            ["line7".to_string(), "line8".to_string(), "line9".to_string()]
        );
        assert!(!h.lines().iter().any(|l| l.len() > 16));
    }

    #[test]
    fn ago_counts_back_from_the_end() {
        let mut h = History::new();
        h.enter("a");
        h.enter("b");
        h.walk(-1); // cursor on "b"
        assert_eq!(h.ago(), 1);
        h.walk(-1); // cursor on "a"
        assert_eq!(h.ago(), 2);
    }
}
