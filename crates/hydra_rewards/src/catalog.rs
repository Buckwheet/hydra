//! `hydra_rewards::catalog` — the three `rewards …` entries the GUI's report
//! list shows.
//!
//! Recovered from `catalog::Entry::{command, matches}` (`0x1014d1470`,
//! `0x1014d1504`) and the table itself at `0x104391370` (stride 0x30).

/// One `rewards` sub-command.
///
/// **ESTABLISHED**: three `&'static str`s, 0x30 bytes — *not* the 0x38-byte
/// `Entry` shape `hydra_exp`/`hydra_combat_log` use, because the rewards rows
/// have no `kind` byte: the row after "Store SimuCoins" starts at `+0x30`
/// exactly (`0x104391370`, `0x1043913a0`, `0x1043913d0`, and the next
/// crate's row at `0x104391400`).
///
/// | offset | field | evidence |
/// |---|---|---|
/// | `+0x00`/`+0x08` | [`Entry::name`] | first `{}` of `matches`' `"{} {} {}"` join (`0x1014d1530`, `0x1014d1544`) |
/// | `+0x10`/`+0x18` | [`Entry::sub`] | second `{}` of the same join, and the `{}` of `command` (`ldr x9, [x0, #0x18]`, `0x1014d1480`) |
/// | `+0x20`/`+0x28` | [`Entry::help`] | third `{}` of the join (`0x1014d1538`) |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The display name (`"Login reward"`, …).
    pub name: &'static str,
    /// The sub-command word — `rewards {sub}`; empty for the plain report.
    pub sub: &'static str,
    /// The one-line description.
    pub help: &'static str,
}

impl Entry {
    /// `catalog::Entry::command(&self) -> String` — `@0x1014d1470` (**full**).
    ///
    /// ```text
    /// CATALOG[0].command()  == "rewards"
    /// CATALOG[1].command()  == "rewards all"
    /// CATALOG[2].command()  == "rewards store"
    /// ```
    ///
    /// The emitted body reads the sub-command's length at `[self+0x18]`
    /// (`0x1014d1480`); when it is zero it answers the seven-byte literal
    /// `rewards` (the two `mov`/`movk` immediates `0x61776572` / `0x73647261`
    /// at `0x1014d14c4`, `0x1014d14d0`), otherwise
    /// `format!("rewards {}", self.sub)` (the blob `rewards {}` at
    /// `0x103e6b922`, formatted through `<&str as Display>::fmt` at
    /// `0x1014f032c`).
    pub fn command(&self) -> String {
        if self.sub.is_empty() {
            "rewards".to_string()
        } else {
            format!("rewards {}", self.sub)
        }
    }

    /// `catalog::Entry::matches(&self, query: &str) -> bool` — `@0x1014d1504`
    /// (**full**).
    ///
    /// The search key is `format!("{} {} {}", name, sub, help)` lowercased
    /// once (`<str>::to_lowercase` at `0x1014d1564`, one of the two calls in
    /// the body); every whitespace-separated word of the query is lowercased
    /// (the second call, `0x1014d1f30`-ish tails) and must occur in the key
    /// (`str::contains`: the `memchr` fast path at `0x1014d1f0c` and the
    /// `StrSearcher::new` at the top).  The fold ends with `and w0, w23, #1`
    /// at `0x1014d1f58`, and the word that is not found exits early with
    /// zero.  An empty query matches everything.
    ///
    /// ```text
    /// CATALOG[1].matches("all")        == true
    /// CATALOG[1].matches("EVERYONE")   == true
    /// CATALOG[1].matches("simucoins")  == false
    /// ```
    pub fn matches(&self, query: &str) -> bool {
        let key = format!("{} {} {}", self.name, self.sub, self.help).to_lowercase();
        query
            .split_whitespace()
            .all(|word| key.contains(&word.to_lowercase()))
    }
}

/// The whole catalogue: three rows at `0x104391370`, all strings from
/// `__const` (`0x10213bd68` …).
pub const CATALOG: [Entry; 3] = [
    Entry {
        name: "Login reward",
        sub: "",
        help: "Whether today's login reward is in, and the last ten",
    },
    Entry {
        name: "Login rewards, everyone",
        sub: "all",
        help: "Every character's latest login reward, and whether today's is in",
    },
    Entry {
        name: "Store SimuCoins",
        sub: "store",
        help: "Each account's monthly store SimuCoins: the last claimed, the balance, \
               and when the next is",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_is_the_word_and_the_sub_command() {
        assert_eq!(CATALOG[0].command(), "rewards");
        assert_eq!(CATALOG[1].command(), "rewards all");
        assert_eq!(CATALOG[2].command(), "rewards store");
    }

    #[test]
    fn every_word_of_the_query_must_appear() {
        assert!(CATALOG[0].matches("today"));
        assert!(CATALOG[0].matches("TODAY"));
        assert!(CATALOG[0].matches("login reward"));
        assert!(CATALOG[1].matches("everyone"));
        assert!(CATALOG[1].matches("simucoins") == false);
        assert!(CATALOG[2].matches("simucoins"));
        assert!(CATALOG[2].matches("balance next"));
        assert!(!CATALOG[2].matches("balance everyone"));
    }

    #[test]
    fn an_empty_query_matches_everything() {
        for e in CATALOG {
            assert!(e.matches(""));
            assert!(e.matches("   "));
        }
    }

    #[test]
    fn the_help_text_matches_the_recovered_catalogue() {
        assert_eq!(CATALOG.len(), 3);
        assert_eq!(CATALOG[0].help, "Whether today's login reward is in, and the last ten");
        assert_eq!(
            CATALOG[1].help,
            "Every character's latest login reward, and whether today's is in"
        );
        assert_eq!(
            CATALOG[2].help,
            "Each account's monthly store SimuCoins: the last claimed, the balance, \
             and when the next is"
        );
    }
}
