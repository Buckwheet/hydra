//! One row of the help catalogue: a name, the words a user may type, and the
//! usage text.
//!
//! The catalogue table itself is built by `hydra_gui::reports::run`
//! (0x1005a51c8 calls `catalog::Entry::command`), so this crate only owns the
//! type and its two predicates.

/// A catalogue row.  The three fields are `&'static str`s at `+0x00`, `+0x10`
/// and `+0x20`: `Entry::matches` hands those three exact offsets to
/// `format!("{} {} {}", …)` and `Entry::command` reads `+0x10` as the verb.
///
/// `// UNKNOWN:` the field names are not in the binary; the ones below describe
/// how each is used (the `+0x10` field is the one `cstats` takes as its word).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// What the report is called (`+0x00`).
    pub name: &'static str,
    /// The words that select it (`+0x10`).
    pub words: &'static str,
    /// The usage line (`+0x20`).
    pub usage: &'static str,
}

impl Entry {
    /// The command line for this entry: `cstats <words> <rest>`.
    ///
    /// Recovered from 0x101367dec: a `String` holding `cstats` (six bytes,
    /// written as `csta`/`ts`), then `' '` + the `+0x10` field when it is not
    /// empty, then `' '` + the trimmed argument when that is not empty.  The
    /// argument is trimmed with `str::trim_matches`-style whitespace trimming
    /// before it is appended.
    pub fn command(&self, rest: &str) -> String {
        let mut out = String::from("cstats");
        if !self.words.is_empty() {
            out.push(' ');
            out.push_str(self.words);
        }
        let rest = rest.trim();
        if !rest.is_empty() {
            out.push(' ');
            out.push_str(rest);
        }
        out
    }

    /// Does every word of `query` appear somewhere in this entry?
    ///
    /// Recovered from 0x101367fe0: the three fields are joined with spaces and
    /// lower-cased (`format!("{} {} {}", …)` at 0x103e2db91, `<str>::to_lowercase`),
    /// then the query is split on `char::is_whitespace` (the class table at
    /// 0x103c82d67) and every word must be a substring of that haystack —
    /// `str::contains` is inlined, complete with the `simd_contains` fast path.
    /// A word that is not found ends the scan with `false`; all words found is
    /// `true`, and an empty query is `true`.
    pub fn matches(&self, query: &str) -> bool {
        let hay = format!("{} {} {}", self.name, self.words, self.usage).to_lowercase();
        query
            .split_whitespace()
            .all(|word| hay.contains(&word.to_lowercase()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUNTS: Entry = Entry {
        name: "Hunts",
        words: "hunts",
        usage: "cstats hunts [all|last <n>]",
    };

    #[test]
    fn command_appends_verb_and_trimmed_argument() {
        assert_eq!(HUNTS.command(""), "cstats hunts");
        assert_eq!(HUNTS.command("   "), "cstats hunts");
        assert_eq!(HUNTS.command("  last 3 "), "cstats hunts last 3");
    }

    #[test]
    fn command_without_words_is_bare_cstats() {
        let bare = Entry {
            name: "Combat",
            words: "",
            usage: "",
        };
        assert_eq!(bare.command(""), "cstats");
        assert_eq!(bare.command("all"), "cstats all");
    }

    #[test]
    fn matches_requires_every_word_as_substring() {
        assert!(HUNTS.matches("hunts"));
        assert!(HUNTS.matches("HUNTS"));
        assert!(HUNTS.matches("hunts last"));
        assert!(HUNTS.matches(""));
        // "last" is only in the usage text, so it still matches; "creatures"
        // appears nowhere.
        assert!(!HUNTS.matches("hunts creatures"));
    }
}
