//! `hydra_exp::catalog` — the `exp` command catalogue entries.
//!
//! An [`Entry`] is three `&'static str`s plus a byte:
//!
//! | offset | field | evidence |
//! |---|---|---|
//! | `+0x00`/`+0x08` | [`Entry::name`] | first argument of the `"{} {} {}"` join in `matches` (`0x1012ec3a4`) |
//! | `+0x10`/`+0x18` | [`Entry::sub`] | loaded by `command` (`ldp x24, x23, [x23, #0x10]`) to build `exp {sub}` |
//! | `+0x20`/`+0x28` | [`Entry::help`] | third argument of the same join |
//! | `+0x30` | [`Entry::kind`] | `ldrb w8, [x20, #0x30]` in `hydra_gui::reports::Reports::command_for`, a four-way `match` |
//!
//! The table itself lives in `hydra_gui`'s report catalogue: eleven 56-byte
//! entries starting at `0x104391108`, immediately followed by reports of other
//! crates. [`REPORTS`] holds exactly those eleven entries (the hydra_exp slice).

/// One `exp` sub-command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The display name (`"Today"`, `"Last pulses"`, …).
    pub name: &'static str,
    /// The sub-command word (`"today"`, `"last"`, …) — `exp {sub}`.
    pub sub: &'static str,
    /// The one-line description.
    pub help: &'static str,
    /// What argument the command wants. `0` for the ten reports that take none,
    /// `1` for `"report"` (a span). `// UNKNOWN:` the binary has four cases
    /// (`hydra_gui`'s `command_for` matches `0`, `1`, `2`, `3`); only `0` and
    /// `1` occur in hydra_exp's own entries.
    pub kind: u8,
}

impl Entry {
    /// `catalog::Entry::command(&self, rest: &str) -> String` —
    /// `@0x1012ec18c` (**full**).
    ///
    /// Builds the command line for this entry: the literal prefix `exp` (three
    /// bytes, allocated first), then ` {sub}` when the sub-command is non-empty,
    /// then ` {rest}` when `rest.trim()` is non-empty. The argument is trimmed
    /// with the outlined `str::trim` at `0x1012fad10`.
    ///
    /// ```text
    /// command("  10  ")   == "exp today 10"
    /// command("   ")      == "exp today"
    /// ```
    pub fn command(&self, rest: &str) -> String {
        let rest = rest.trim();
        let mut out = String::from("exp");
        if !self.sub.is_empty() {
            out.push(' ');
            out.push_str(self.sub);
        }
        if !rest.is_empty() {
            out.push(' ');
            out.push_str(rest);
        }
        out
    }

    /// `catalog::Entry::matches(&self, query: &str) -> bool` —
    /// `@0x1012ec37c` (**full**).
    ///
    /// The search key is `format!("{} {} {}", name, sub, help)` lowercased once
    /// (`to_lowercase`, `0x1012eb538c`); every whitespace-separated word of the
    /// query is lowercased and must occur in that key (`str::contains`, which
    /// the binary inlines: a `memchr`/`memcmp` fast path for one- and two-byte
    /// needles and `core::str::pattern::StrSearcher` above that). Word splitting
    /// uses `char::is_whitespace`, whose table is the bitmask
    /// `0x1_0000_3e00` for ASCII plus the `0x103c82d67` byte table for the
    /// Unicode spaces.
    ///
    /// An empty query matches everything (`all` over no words).
    pub fn matches(&self, query: &str) -> bool {
        let key = format!("{} {} {}", self.name, self.sub, self.help).to_lowercase();
        query
            .split_whitespace()
            .all(|word| key.contains(&word.to_lowercase()))
    }
}

/// The eleven `exp` report entries, recovered from `0x104391108`
/// (stride `0x38`, all strings from `__const`/`__cstring`).
pub const REPORTS: [Entry; 11] = [
    Entry {
        name: "Today",
        sub: "today",
        help: "Today's totals, since midnight on the game's clock",
        kind: 0,
    },
    Entry {
        name: "Hour",
        sub: "hour",
        help: "The last hour's gains and rate",
        kind: 0,
    },
    Entry {
        name: "Daily",
        sub: "daily",
        help: "Ten days of totals, a line each",
        kind: 0,
    },
    Entry {
        name: "Last pulses",
        sub: "last",
        help: "The last ten pulses",
        kind: 0,
    },
    Entry {
        name: "Instant",
        sub: "instant",
        help: "The last ten bounties, brooch rubs and lumps",
        kind: 0,
    },
    Entry {
        name: "Timing",
        sub: "timing",
        help: "How far apart the pulses came",
        kind: 0,
    },
    Entry {
        name: "Level",
        sub: "level",
        help: "The level reached, what is left, and when at today's pace",
        kind: 0,
    },
    Entry {
        name: "Favor",
        sub: "favor",
        help: "Voln favor as last seen",
        kind: 0,
    },
    Entry {
        name: "Resource",
        sub: "resource",
        help: "The profession's resource points as last seen",
        kind: 0,
    },
    Entry {
        name: "Suffused",
        sub: "suffused",
        help: "Suffused experience as last seen",
        kind: 0,
    },
    Entry {
        name: "Between times",
        sub: "report",
        help: "The whole report over a span: from one time to another",
        kind: 1,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        REPORTS[0]
    }

    #[test]
    fn command_builds_the_typed_line() {
        assert_eq!(entry().command(""), "exp today");
        assert_eq!(entry().command("   "), "exp today");
        assert_eq!(entry().command(" 10 "), "exp today 10");
        assert_eq!(REPORTS[10].command("13:05 15:02"), "exp report 13:05 15:02");
    }

    #[test]
    fn matches_requires_every_word() {
        assert!(entry().matches("today"));
        assert!(entry().matches("TODAY"));
        // The key contains name, sub and help, so description words match too.
        assert!(entry().matches("midnight"));
        assert!(entry().matches("today midnight"));
        assert!(!entry().matches("today hourly"));
        assert!(!entry().matches("level"));
    }

    #[test]
    fn empty_query_matches() {
        assert!(entry().matches(""));
        assert!(entry().matches("   "));
    }

    #[test]
    fn matches_the_recovered_catalogue() {
        assert!(REPORTS[3].matches("last pulses"));
        assert!(REPORTS[4].matches("bounties"));
        assert!(REPORTS[10].matches("span"));
        assert!(!REPORTS[10].matches("today"));
        assert_eq!(REPORTS[10].kind, 1);
        assert_eq!(REPORTS.len(), 11);
    }
}
