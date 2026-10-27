//! The `cstats` command: its word grammar, and the dispatch into the reports.

use crate::report::{self, Scope};
use crate::store::Store;
use crate::{analytics, detail, listing};

/// What a `cstats` line asks for.
///
/// `command::answer` (0x101393dc0) dispatches on the first byte with a jump
/// table of 17 entries (0x10241455e, base 0x101393f00) and `parse` builds the
/// value.  Variants `0`–`16` are the table's order; the later ones are the
/// discriminants `parse` hands to the same tail (`0x11` for `barkskin`,
/// `0x14` at 0x10139358c).
///
/// `// UNKNOWN:` the variant *names* are not in the binary; each is named after
/// the report it reaches.  `Help` is variant `0x15` — the only one whose payload
/// is read as a `String` by both callers (`run` and
/// `hydra_table::cstats_command`).
#[derive(Clone, Debug, PartialEq)]
pub enum Asked {
    /// No words: the quick actions.
    List,
    /// `cstats [full] [<id>|<scope>]`
    Session { scope: Scope, full: bool },
    /// `cstats sessions [SCOPE]`
    Sessions(Scope),
    /// `cstats all | last <n>`
    Aggregate(Scope),
    /// `cstats abilities [SCOPE]`
    Abilities(Scope),
    /// `cstats attacks [SCOPE] [recent [<n>]] [page <p>] [per <n>]`
    Attacks(listing::Listing),
    /// `cstats attack #<id>`
    AttackDetail(i64),
    /// `cstats attack <name> [<n>]`
    AttackByName {
        name: String,
        nth: Option<i64>,
        scope: Scope,
    },
    /// `cstats creatures [SCOPE] …`
    Creatures(listing::Listing),
    /// `cstats creature <id>|<noun> [<n>]`
    Creature {
        id: Option<i64>,
        noun: Option<String>,
        nth: Option<i64>,
        full: bool,
    },
    /// `cstats tree <id>`
    Tree(i64),
    /// `cstats flare [<name>]`
    Flare { name: Option<String>, scope: Scope },
    /// `cstats flare <name> alongside`
    Alongside { name: String, scope: Scope },
    /// `cstats def [<noun>]`
    Defense { noun: Option<String>, scope: Scope },
    /// `cstats hp [<noun>|<id>]`
    Hp { which: Option<String>, scope: Scope },
    /// `cstats blind|control|family [SCOPE]` and `aim`, `accuracy`, `ttk`
    Analytics { which: Analytics, scope: Scope },
    /// `cstats compare [<id> <id>]`
    Compare { first: Option<i64>, second: Option<i64> },
    /// `cstats barkskin`
    Barkskin,
    /// `cstats intercepts [SCOPE]`
    Intercepts(Scope),
    /// `cstats effects [SCOPE]`
    Effects(Scope),
    /// `cstats gear [SCOPE]`
    Gear(Scope),
    /// Anything else: the words to say back.
    Message(String),
}

/// The `analytics` sub-reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Analytics {
    /// `blind`
    Blind,
    /// `control`
    Control,
    /// `family`
    Family,
    /// `aim [<location>]`
    Aim,
    /// `accuracy`
    Accuracy,
    /// `ttk`
    Ttk,
}

impl Analytics {
    pub fn word(self) -> &'static str {
        match self {
            Analytics::Blind => "blind",
            Analytics::Control => "control",
            Analytics::Family => "family",
            Analytics::Aim => "aim",
            Analytics::Accuracy => "accuracy",
            Analytics::Ttk => "ttk",
        }
    }
}

/// The help table.
///
/// Recovered verbatim from the block at 0x102414701: each entry is the word
/// column, padded, followed by its description.  `parse` hands it back as the
/// payload of [`Asked::Message`] when the line asks for help, and quotes
/// `` `{}` is not a cstats word; cstats help lists them. `` otherwise.
pub const HELP: [(&str, &str); 21] = [
    ("cstats", "the latest hunt's report"),
    ("cstats <id>", "that hunt's report;"),
    ("cstats full [<id>]", "every creature listed"),
    (
        "cstats sessions [SCOPE]",
        "the hunts, a line each (the last ten without a scope)",
    ),
    ("cstats all | last <n>", "every hunt added up, or the last n"),
    (
        "cstats abilities [SCOPE]",
        "damage by ability, the flares, and the attacks against you",
    ),
    (
        "cstats attacks [SCOPE] [recent [<n>]] [page <p>] [per <n>]",
        "every attack name by damage, a page at a time; recent, the newest first",
    ),
    (
        "cstats attack [<name> [<n>]]",
        "the nth instance of an attack (the latest by default); attack #<id> by row",
    ),
    (
        "cstats creatures [SCOPE] [recent [<n>]] [page <p>] [per <n>]",
        "every creature by damage, a page at a time; recent, the newest first",
    ),
    (
        "cstats creature <id>|<noun> [<n>]",
        "a creature's sequences; creature full <id>, its whole tree",
    ),
    ("cstats tree <id>", "a spawn tree by its root attack"),
    (
        "cstats flare [<name>]",
        "a flare's proc rate, spread against the eligible, statuses and damage; a barrier's, what it stopped; flare <name> alongside, what fires with it",
    ),
    ("cstats def [<noun>]", "DS, TD and UDF readings for a creature"),
    ("cstats hp [<noun>|<id>]", "HP estimates for the creatures killed"),
    (
        "cstats blind|control|family [SCOPE]",
        "control and photo/bloom coverage",
    ),
    (
        "cstats aim [<location>] [SCOPE]",
        "aimed against unaimed; the location is assumed",
    ),
    (
        "cstats accuracy|ttk [SCOPE]",
        "hit rate, and kill efficiency",
    ),
    (
        "cstats compare [<id> <id>]",
        "two hunts side by side (the latest two by default)",
    ),
    (
        "cstats barkskin",
        "each bark cast: its absorbs, what they took, whether it held",
    ),
    (
        "cstats intercepts [SCOPE]",
        "every barrier together: of the attacks that would have hit, how many were stopped, by barrier and by attacker",
    ),
    (
        "cstats gear [SCOPE]",
        "what the swings were made with: each item held and the flares seen on it, the gauntlets' flares, the items worn",
    ),
];

/// The footer of the help block (0x102414c??).
pub const HELP_FOOTER: &str = "SCOPE: a hunt id, all, last <n>, or between <id> <id>; the latest hunt by default.";

/// The help text as one block: each `word` padded to 28 columns, then its
/// description, then the footer.
pub fn help() -> String {
    let mut out = String::new();
    for (word, text) in HELP {
        out.push_str(word);
        for _ in word.len()..28 {
            out.push(' ');
        }
        out.push_str(text);
        out.push('\n');
    }
    out.push_str(HELP_FOOTER);
    out
}

/// The words of a `cstats` line, `cstats` itself already removed by [`run`].
///
/// Recovered from 0x1013924bc: the line is split on whitespace, every word is
/// lower-cased in place (the SIMD byte loop at 0x101392684 subtracting
/// `0x41`/adding `0x20`), the session scope is taken from the tail with
/// [`scope_from`], and the first word selects the report through a jump table on
/// the *word length* (`1..=10`, table at 0x10241454a).
pub fn parse(line: &str) -> Asked {
    let words: Vec<String> = line
        .split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect();
    let first = match words.first() {
        Some(w) => w.as_str(),
        // A bare `cstats` lists the quick actions.
        None => return Asked::List,
    };
    let rest = &words[1..];
    let scope = scope_from(rest).unwrap_or(Scope::Last);
    let listing = || listing::Listing {
        scope: scope.clone(),
        ..listing::Listing::default()
    };
    match first {
        "help" | "menu" => Asked::Message(help()),
        "hunts" | "sessions" => Asked::Sessions(scope),
        "session" => Asked::Session {
            scope,
            full: words.iter().any(|w| w == "full"),
        },
        "all" => Asked::Aggregate(Scope::All),
        "last" => match scope {
            Scope::LastN(n) => Asked::Aggregate(Scope::LastN(n)),
            // 0x1024146e2: "say how many: cstats last <n>."
            _ => Asked::Message(String::from("say how many: cstats last <n>.")),
        },
        "abilities" => Asked::Abilities(scope),
        "attacks" => Asked::Attacks(listing()),
        "attack" => attack_asked(rest, scope),
        "creatures" => Asked::Creatures(listing()),
        "creature" => creature_asked(rest, scope),
        "tree" => match rest.first().and_then(|w| w.parse::<i64>().ok()) {
            Some(id) => Asked::Tree(id),
            // 0x1024146c2: " say the root: cstats tree <id>."
            None => Asked::Message(String::from(" say the root: cstats tree <id>.")),
        },
        "flare" => match rest.first() {
            None => Asked::Flare {
                name: None,
                scope,
            },
            Some(name) if rest.iter().any(|w| w == "alongside") => Asked::Alongside {
                name: name.clone(),
                scope,
            },
            Some(name) => Asked::Flare {
                name: Some(name.clone()),
                scope,
            },
        },
        "def" | "defense" => Asked::Defense {
            noun: rest.first().cloned(),
            scope,
        },
        "hp" => Asked::Hp {
            which: rest.first().cloned(),
            scope,
        },
        "blind" | "control" | "family" | "aim" | "accuracy" | "ttk" => Asked::Analytics {
            which: match first {
                "blind" => Analytics::Blind,
                "control" => Analytics::Control,
                "family" => Analytics::Family,
                "aim" => Analytics::Aim,
                "accuracy" => Analytics::Accuracy,
                _ => Analytics::Ttk,
            },
            scope,
        },
        "compare" => {
            let ids: Vec<i64> = rest.iter().filter_map(|w| w.parse().ok()).collect();
            Asked::Compare {
                first: ids.first().copied(),
                second: ids.get(1).copied(),
            }
        }
        "barkskin" => Asked::Barkskin,
        "intercepts" => Asked::Intercepts(scope),
        "effects" => Asked::Effects(scope),
        "gear" => Asked::Gear(scope),
        "full" => Asked::Session { scope, full: true },
        other => Asked::Message(format!("`{}` is not a cstats word; cstats help lists them.", other)),
    }
}

/// `cstats attack [<name> [<n>]]` / `cstats attack #<id>`.
fn attack_asked(rest: &[String], scope: Scope) -> Asked {
    let first = match rest.first() {
        Some(w) => w,
        None => {
            return Asked::Message(String::from(
                "`attack` needs a name; attack #<id> by row",
            ))
        }
    };
    if let Some(number) = first.strip_prefix('#') {
        return match number.parse::<i64>() {
            Ok(id) => Asked::AttackDetail(id),
            // 0x103e69195: "`{}` is not an attack id."
            Err(_) => Asked::Message(format!("`{}` is not an attack id.", number)),
        };
    }
    Asked::AttackByName {
        name: first.clone(),
        nth: rest.get(1).and_then(|w| w.parse().ok()),
        scope,
    }
}

/// `cstats creature <id>|<noun> [<n>]`.
fn creature_asked(rest: &[String], _scope: Scope) -> Asked {
    let mut full = false;
    let mut which: Option<&String> = None;
    for word in rest {
        match word.as_str() {
            "full" => full = true,
            _ => {
                if which.is_none() {
                    which = Some(word);
                }
            }
        }
    }
    let which = match which {
        Some(w) => w,
        None => {
            return Asked::Message(String::from(
                "cstats creature <id>|<noun>: the top 20 by damage",
            ))
        }
    };
    match which.parse::<i64>() {
        Ok(id) => Asked::Creature {
            id: Some(id),
            noun: None,
            nth: None,
            full,
        },
        Err(_) => Asked::Creature {
            id: None,
            noun: Some(which.clone()),
            nth: rest.iter().filter_map(|w| w.parse().ok()).next(),
            full,
        },
    }
}

/// The session scope at the tail of a word list.
///
/// Recovered from 0x101391784: the *last* word is examined first —
/// `between <a> <b>` (the bytes `b`,`e`,`t`,`w`,`e`,`e`,`n` are compared at
/// 0x101391990) yields a range, `last [<n>]` (0x101391d48) yields the last *n*
/// (`n` defaulting to 1), a plain decimal — with an optional leading `+`/`-`,
/// which is why the sign characters are compared — yields that one session, and
/// anything else is `None`.
pub fn scope_from(words: &[String]) -> Option<Scope> {
    // The keywords win over a bare number: `attacks recent 5` is not session 5.
    for i in 0..words.len() {
        match words[i].as_str() {
            "between" => {
                let a = words.get(i + 1).and_then(|w| number(w))?;
                let b = words.get(i + 2).and_then(|w| number(w))?;
                return Some(Scope::Between(a, b));
            }
            "last" => {
                let n = words.get(i + 1).and_then(|w| number(w)).unwrap_or(1);
                return Some(Scope::LastN(n));
            }
            "all" => return Some(Scope::All),
            _ => {}
        }
    }
    // A lone number is that one session.
    if words.len() == 1 {
        return number(&words[0]).map(Scope::One);
    }
    None
}

/// A signed decimal, as `scope_from` and the `<id>` arguments take them.
fn number(word: &str) -> Option<i64> {
    let digits = word.strip_prefix(['+', '-']).unwrap_or(word);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

/// A reply: the lines to show, or one message.
pub type Answer = Result<Vec<String>, String>;

/// Run a `cstats` line against the ledger.
///
/// Recovered from 0x101392108: the line is trimmed, leading `.`/`;` and spaces
/// are skipped, the first word must be `cstats` (six bytes compared
/// case-insensitively against `c`,`s`,`t`,`a`,`t`,`s`), and the rest goes to
/// [`parse`] and then [`answer`].  Anything else is
/// `{} is not a combat report.` (0x103e69146).
pub fn run(store: &Store, character: &str, line: &str) -> Answer {
    const NOT_A_REPORT: &str = "{} is not a combat report.";
    let trimmed = line.trim();
    let rest = trimmed.trim_start_matches(['.', ';']);
    let rest = rest.trim_start();
    let (word, tail) = match rest.split_once(char::is_whitespace) {
        Some((w, tail)) => (w, tail),
        None => (rest, ""),
    };
    if !word.eq_ignore_ascii_case("cstats") {
        return Err(NOT_A_REPORT.replace("{}", rest));
    }
    answer(store, character, &parse(tail))
}

/// Show one parsed command.
///
/// Recovered from 0x101393dc0: open a reader, then match the [`Asked`] and call
/// one report; the payload is `Ok(lines)` when a report ran and `Err(message)`
/// when it could not.  A failure to open the ledger prints the driver's own
/// message, which is why the error arm formats the error with `Display`.
pub fn answer(store: &Store, character: &str, asked: &Asked) -> Answer {
    let lines = match asked {
        Asked::List => Ok(report::quick_actions()),
        Asked::Session { scope, full } => report::session(store, character, scope, *full),
        Asked::Sessions(scope) => report::sessions(store, character, scope),
        Asked::Aggregate(scope) => report::aggregate(store, character, scope),
        Asked::Abilities(scope) => report::abilities(store, character, scope),
        Asked::Attacks(listing) => listing::attacks(store, character, listing),
        Asked::AttackDetail(id) => detail::attack_detail(store, *id, false),
        Asked::AttackByName { name, nth, scope } => {
            detail::attack_by_name(store, character, name, *nth, scope)
        }
        Asked::Creatures(listing) => listing::creatures(store, character, listing),
        Asked::Creature {
            id,
            noun,
            nth,
            full,
        } => detail::creature(store, character, *id, noun.as_deref(), *nth, *full),
        Asked::Tree(id) => detail::tree(store, *id),
        Asked::Flare { name, scope } => analytics::flare(store, character, name.as_deref(), scope),
        Asked::Alongside { name, scope } => analytics::alongside(store, character, name, scope),
        Asked::Defense { noun, scope } => {
            analytics::defense(store, character, noun.as_deref(), scope)
        }
        Asked::Hp { which, scope } => analytics::hp(store, character, which.as_deref(), scope),
        Asked::Analytics { which, scope } => analytics::analytics(store, character, *which, scope),
        Asked::Compare { first, second } => {
            analytics::compare(store, character, *first, *second)
        }
        Asked::Barkskin => analytics::barkskin(store, character),
        Asked::Intercepts(scope) => analytics::intercepts(store, character, scope),
        Asked::Effects(scope) => analytics::effects(store, character, scope),
        Asked::Gear(scope) => analytics::gear(store, character, scope),
        Asked::Message(text) => return Err(text.clone()),
    };
    lines.map_err(|e| e.message().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_cstats_lists_quick_actions() {
        assert_eq!(parse(""), Asked::List);
    }

    #[test]
    fn words_are_lowercased_and_scoped() {
        assert_eq!(parse("SESSIONS ALL"), Asked::Sessions(Scope::All));
        assert_eq!(
            parse("sessions last 3"),
            Asked::Sessions(Scope::LastN(3))
        );
        assert_eq!(
            parse("sessions between 2 4"),
            Asked::Sessions(Scope::Between(2, 4))
        );
        assert_eq!(parse("session 7"), Asked::Session {
            scope: Scope::One(7),
            full: false
        });
    }

    #[test]
    fn attack_forms() {
        assert_eq!(parse("attack #12"), Asked::AttackDetail(12));
        assert_eq!(
            parse("attack #x"),
            Asked::Message(String::from("`x` is not an attack id."))
        );
        assert_eq!(
            parse("attack jab 2"),
            Asked::AttackByName {
                name: String::from("jab"),
                nth: Some(2),
                scope: Scope::Last
            }
        );
    }

    #[test]
    fn unknown_word_is_reported() {
        assert_eq!(
            parse("wibble"),
            Asked::Message(String::from("`wibble` is not a cstats word; cstats help lists them."))
        );
    }

    #[test]
    fn tree_without_an_id_says_so() {
        assert_eq!(
            parse("tree"),
            Asked::Message(String::from(" say the root: cstats tree <id>."))
        );
    }

    #[test]
    fn scope_from_reads_the_tail() {
        assert_eq!(scope_from(&["cstats".into()]), None);
        assert_eq!(scope_from(&["all".into()]), Some(Scope::All));
        assert_eq!(scope_from(&["last".into()]), Some(Scope::LastN(1)));
        assert_eq!(
            scope_from(&["sessions".into(), "last".into(), "5".into()]),
            Some(Scope::LastN(5))
        );
        assert_eq!(
            scope_from(&[
                "sessions".into(),
                "between".into(),
                "2".into(),
                "9".into()
            ]),
            Some(Scope::Between(2, 9))
        );
        assert_eq!(scope_from(&["4".into()]), Some(Scope::One(4)));
        assert_eq!(scope_from(&["sessions".into(), "4".into()]), None);
    }

    #[test]
    fn run_rejects_other_commands() {
        let store = Store::new();
        assert_eq!(
            run(&store, "Testchar", "exp report"),
            Err(String::from("exp report is not a combat report."))
        );
        assert_eq!(
            run(&store, "Testchar", ".cstats"),
            Ok(report::quick_actions())
        );
    }

    #[test]
    fn help_block_is_padded_to_28_columns() {
        let text = help();
        let first = text.lines().next().unwrap();
        assert!(first.starts_with("cstats"));
        assert_eq!(&first[28..], "the latest hunt's report");
        assert!(text.ends_with(HELP_FOOTER));
    }
}

#[cfg(test)]
mod answer_tests {
    use super::*;
    use crate::store::{Attack, Hit};

    fn store() -> Store {
        let mut s = Store::new();
        let a = s.hunt_started(1, "Testchar", 1_000_000);
        s.hunt_ended(a, 1_000_600);
        let kobold = s.creature_seen(a, "kobold", "a kobold", 1_000_100);
        let swing = s.attack(Attack {
            hunt_id: a,
            seq: 1,
            occurred_at: 1_000_200,
            name: String::from("jab"),
            attacker: Some(String::from("Testchar")),
            creature_id: Some(kobold),
            ours: true,
            outcome: Some(String::from("hit")),
            ..Attack::default()
        });
        s.hit(Hit {
            attack_id: swing,
            hunt_id: a,
            creature_id: Some(kobold),
            seq: 1,
            damage: 25,
            ..Hit::default()
        });
        s
    }

    #[test]
    fn answer_dispatches_every_family() {
        let s = store();
        // The bare command lists the quick actions.
        assert_eq!(answer(&s, "Testchar", &Asked::List).unwrap(), report::quick_actions());
        // A session report.
        let lines = answer(
            &s,
            "Testchar",
            &Asked::Session {
                scope: Scope::Last,
                full: false,
            },
        )
        .unwrap();
        assert_eq!(lines[0], "=== SESSION REPORT #1 ===");
        // The listings.
        let lines = answer(&s, "Testchar", &Asked::Attacks(listing::Listing::default())).unwrap();
        assert!(lines[0].starts_with("Attacks, newest first"));
        // An unknown id is an error line, not a panic.
        assert_eq!(
            answer(&s, "Testchar", &Asked::Tree(99)),
            Err(String::from("No attack #99."))
        );
        // A message is handed straight back.
        assert_eq!(
            answer(&s, "Testchar", &Asked::Message(String::from("help me"))),
            Err(String::from("help me"))
        );
    }

    #[test]
    fn accuracy_counts_the_swings() {
        let s = store();
        let lines = answer(
            &s,
            "Testchar",
            &Asked::Analytics {
                which: Analytics::Accuracy,
                scope: Scope::Last,
            },
        )
        .unwrap();
        assert_eq!(lines[1], "1 of 1 attempts (100.0%)");
    }
}
