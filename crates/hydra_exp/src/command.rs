//! `hydra_exp::command` — the `exp` command: parse it, read it, run it.
//!
//! The binary's entry points (all `@0x1012c…`):
//!
//! | fn | address | return shape |
//! |---|---|---|
//! | [`parse`] | `0x1012cbe60` | 32-byte `Result<Parsed, String>` (tag + 24-byte payload) |
//! | [`parse_time`] | `0x1012ca8f4` | `Option<i64>` |
//! | [`run`] | `0x1012cb100` | same 32-byte shape as `read` |
//! | [`read`] | `0x1012cb42c` | tag `1` = one `String`, tag `0` = a `Vec<String>` |
//! | [`answer`] | `0x1012cc990` | as `read` |
//!
//! `read` and `answer` both open the ledger (`hydra_ledger::Ledger::reader`),
//! map a `rusqlite::error::Error` to its `Display` text (tag `1`) and otherwise
//! dispatch on the byte at `&Parsed + 0` — `read` through the ten-entry jump
//! table at `0x10240cff0`, `answer` through a chain of compares for the four
//! mutating variants and a `bl read` for everything else.
//!
//! [`Source`] is the seam those two functions read and write through. It is
//! this crate's public API (siblings and the tests pass their own
//! implementations), and [`crate::ledger::LedgerSource`] is the real one: it
//! drives `hydra_ledger::Ledger::{reader, writer, prune_pulses}` and maps
//! `rusqlite`-era errors to their `Display` text, which is why the trait's error
//! type is `String` (see `NOTES.md`).

use hydra_net::eastern;

use crate::report;
use crate::row::{Pulse, Snapshot, Source as PulseSource, Who};

/// Where a command reads from and writes to.
///
/// The four readers are the `hydra_ledger::reader::Reader` methods the binary
/// calls (`last_pulses` `0x10168b5d8`, `pulses_between`, `latest_snapshots`
/// `0x10168c174`, `snapshots` `0x10168cdac`); the three writers are the ones the
/// mutating commands need.
pub trait Source {
    /// The newest `limit` pulses, oldest first (`Reader::last_pulses`).
    fn last_pulses(&self, limit: usize) -> Result<Vec<Pulse>, String>;
    /// Pulses in `[from, to]` (`Reader::pulses_between`).
    fn pulses_between(&self, from: i64, to: i64) -> Result<Vec<Pulse>, String>;
    /// The most recent snapshot of every kind (`Reader::latest_snapshots`).
    fn latest_snapshots(&self) -> Result<Vec<Snapshot>, String>;
    /// Snapshots of one kind in `[from, to]` (`Reader::snapshots`).
    fn snapshots(&self, which: u8, from: i64, to: i64) -> Result<Vec<Snapshot>, String>;
    /// Append a pulse (`hydra_ledger::writer::Writer::try_record`,
    /// `0x10169398c`); the error is the writer's `Closed` `Display` text.
    fn try_record(&mut self, pulse: Pulse) -> Result<(), String>;
    /// `hydra_ledger::Ledger::prune_pulses(drop_all)` (`0x10168e4ec`): with
    /// `false` it drops the rows that gained nothing, with `true` every row of
    /// this character. Returns how many rows went.
    fn prune_pulses(&mut self, drop_all: bool) -> Result<usize, String>;
}

/// What `exp <text>` means. The binary's `Parsed` is 24 bytes: `read`
/// dispatches on ten discriminants (`0x0`…`0x9`) and `answer` on four more
/// (`0xa` `addgain`, `0xb` `clean`, `0xc` `reset`, `0xd` `confirm_reset`).
/// The variants below cover the twelve help lines (`0x10442dc00`) and every
/// report call that was recovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// `exp` alone / `exp help`: the twelve usage lines.
    Help,
    /// `exp session` — pulses of the last day.
    Session,
    /// `exp today` — pulses since midnight.
    Today,
    /// `exp hour` — pulses of the last hour.
    Hour,
    /// `exp daily` — ten days of totals.
    Daily,
    /// `exp last` / `exp instant` (`instant` also filters and keeps ten).
    Last {
        /// `true` for `exp instant`.
        instant: bool,
    },
    /// `exp timing`.
    Timing,
    /// `exp level`.
    Level,
    /// `exp favor` / `exp resource` / `exp suffused` (`which` = 0/1/2).
    Snapshots {
        /// Index into [`report::SNAPSHOT_KINDS`].
        which: u8,
    },
    /// `exp report <from> <to>`.
    Report {
        /// Inclusive start (unix seconds).
        from: i64,
        /// Inclusive end (unix seconds).
        to: i64,
    },
    /// `exp addgain <n>`.
    AddGain(i64),
    /// `exp clean`.
    Clean,
    /// `exp reset` — only asks; it prints the confirmation prompt.
    Reset,
    /// `exp confirm_reset` — the second step `reset` asks for.
    ConfirmReset,
}

/// What a command produces: either one line to print, or a report.
///
/// The binary returns a 32-byte tagged value; tag `1` carries a single `String`
/// (the error text, the help text, "… is not an experience report.") and tag `0`
/// carries a `Vec<String>` (`0x1012cb4ec` / `0x1012cb5d0`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Print one line.
    Say(String),
    /// Print a report.
    Lines(Vec<String>),
}

impl Reply {
    /// The lines of either variant, for callers that just print.
    pub fn into_lines(self) -> Vec<String> {
        match self {
            Reply::Say(line) => vec![line],
            Reply::Lines(lines) => lines,
        }
    }
}

/// The twelve usage lines, verbatim from the table at `0x10442dc00`
/// (eleven `exp …` lines plus the closing note).
pub const HELP: [&str; 12] = [
    "exp session            this session: since the first pulse within the last day",
    "exp today              today's totals, since midnight on the game's clock",
    "exp hour               the last hour's gains and rate",
    "exp daily              ten days of totals, a line each",
    "exp last               the last ten pulses; exp instant, the last ten bounties, brooch rubs and lumps",
    "exp timing             how far apart the pulses came",
    "exp level              the level reached, what is left, and when at today's pace",
    "exp favor              Voln favor as last seen; exp resource, the profession's points",
    "exp report <from> <to>  the whole report over a span: times as 13:05, \"07-20 13:05\" or \"2026-07-20 13:05:52\"",
    "exp addgain <n>        a gain the bar did not state, typed in",
    "exp clean              drop rows with no gain; exp reset, every row of this character's (asks first)",
    "Nothing is ever sent to the game: the bars the game sends on its own are what is recorded.",
];

/// `command::parse(rest: &str, now: i64) -> Result<Parsed, String>` —
/// `@0x1012cbe60` (**partly**).
///
/// Trims the text, then takes the first word — a `"…"` quoted argument when the
/// text starts with `"` (`cmp w9, #0x22`, searched with `core::str::pattern`),
/// otherwise everything up to the first `char::is_whitespace`. A bare `exp`
/// comes back as `Err(help)`, which is the branch `run` prints as-is
/// (tag `1` + a `String`); everything else is matched against the command words
/// of the help table.
///
/// `// UNKNOWN:` the exact word→variant mapping for the four commands whose
/// bodies live in `answer` (`addgain`/`clean`/`reset` plus the bare `exp`).
pub fn parse(rest: &str, now: i64) -> Result<Parsed, String> {
    let text = rest.trim();
    let (word, tail) = split_word(text);
    if word.is_empty() {
        return Err(HELP.join("\n"));
    }
    let parsed = match word.to_ascii_lowercase().as_str() {
        "session" => Parsed::Session,
        "today" => Parsed::Today,
        "hour" => Parsed::Hour,
        "daily" => Parsed::Daily,
        "last" => Parsed::Last { instant: false },
        "instant" => Parsed::Last { instant: true },
        "timing" => Parsed::Timing,
        "level" => Parsed::Level,
        "favor" => Parsed::Snapshots { which: 0 },
        "resource" => Parsed::Snapshots { which: 1 },
        "suffused" => Parsed::Snapshots { which: 2 },
        "report" => {
            let (from, to) = split_pair(tail);
            match (parse_time(from, now), parse_time(to, now)) {
                (Some(from), Some(to)) => Parsed::Report { from, to },
                _ => return Err(HELP[8].to_string()),
            }
        }
        "addgain" => match tail.trim().parse::<i64>() {
            Ok(gain) => Parsed::AddGain(gain),
            Err(_) => return Err(HELP[9].to_string()),
        },
        "clean" => Parsed::Clean,
        "reset" => Parsed::Reset,
        "confirm_reset" => Parsed::ConfirmReset,
        "help" => Parsed::Help,
        _ => return Err(format!("{} is not an experience report.", word)),
    };
    Ok(parsed)
}

/// Split the first whitespace-delimited word from the rest, honouring a leading
/// `"…"` quote (`0x1012cbea4`, the `movi.2s #0x22` compare).
fn split_word(text: &str) -> (&str, &str) {
    if let Some(rest) = text.strip_prefix('"') {
        return match rest.find('"') {
            Some(end) => (&rest[..end], &rest[end + 1..]),
            None => (rest, ""),
        };
    }
    let end = text
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    (&text[..end], &text[end..])
}

/// Split the two arguments of `exp report`: quoted, or the first two words.
fn split_pair(tail: &str) -> (&str, &str) {
    let tail = tail.trim();
    if tail.starts_with('"') {
        let (first, rest) = split_word(tail);
        let (second, _) = split_word(rest.trim());
        return (first, second);
    }
    let (first, rest) = split_word(tail);
    // A span may be written as `"07-20 13:05" "07-22 15:02"`, i.e. two quoted
    // pairs; when unquoted the last two whitespace fields are the span.
    let words: Vec<&str> = rest.split_whitespace().collect();
    match words.len() {
        0 => (first, ""),
        1 => (first, words[0]),
        _ => {
            let n = words.len();
            (words[n - 2], words[n - 1])
        }
    }
}

/// `command::parse_time(text: &str, now: i64) -> Option<i64>` —
/// `@0x1012ca8f4` (**partly**).
///
/// Turns one of the three accepted spellings into unix seconds: `13:05`,
/// `07-20 13:05` or `2026-07-20 13:05:52` (the help line at `0x10442dc80` lists
/// exactly these). The binary calls `hydra_net::eastern::date(now)` and
/// `eastern::midnight(&date)` once each, so the day comes from `now` unless the
/// text carries one; the clock may omit the seconds.
///
/// `// UNKNOWN:` whether a bare date (no clock) is accepted and whether `now`'s
/// year is always used for the `MM-DD` form; this build requires a clock and
/// takes the year from `now`.
pub fn parse_time(text: &str, now: i64) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut fields = text.split_whitespace();
    let first = fields.next()?;
    let second = fields.next();
    if fields.next().is_some() {
        return None;
    }
    let today = eastern::date(now);
    match second {
        None => {
            let (hour, minute, second) = parse_clock(first)?;
            Some(eastern::midnight(&today) + hour * 3_600 + minute * 60 + second)
        }
        Some(clock) => {
            let date = parse_date(first, &today)?;
            let (hour, minute, second) = parse_clock(clock)?;
            Some(eastern::midnight(&date) + hour * 3_600 + minute * 60 + second)
        }
    }
}

/// `YYYY-MM-DD` (via `eastern::parse`) or `MM-DD` in `today`'s year.
fn parse_date(text: &str, today: &eastern::Date) -> Option<eastern::Date> {
    if let Some(date) = eastern::parse(text) {
        return Some(date);
    }
    let mut fields = text.split('-');
    let month: i64 = fields.next()?.parse().ok()?;
    let day: i64 = fields.next()?.parse().ok()?;
    if fields.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(eastern::Date::new(today.year, month, day))
}

/// `H:MM` or `H:MM:SS`, each field one or two digits.
fn parse_clock(text: &str) -> Option<(i64, i64, i64)> {
    let mut fields = text.split(':');
    let hour: i64 = fields.next()?.parse().ok()?;
    let minute: i64 = fields.next()?.parse().ok()?;
    let second: i64 = match fields.next() {
        Some(second) => second.parse().ok()?,
        None => 0,
    };
    if fields.next().is_some() {
        return None;
    }
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..60).contains(&second) {
        return None;
    }
    Some((hour, minute, second))
}

/// `command::run(src, who, text, now) -> Reply` — `@0x1012cb100`
/// (**full** for the prologue, partly for the dispatch).
///
/// The text is trimmed, then leading `';'`/`'.'` characters are dropped
/// (`0x3b`/`0x2e`), then whitespace; the first word must be exactly three bytes
/// long and equal to `exp` ignoring ASCII case (`0x65`/`0x78`/`0x70` compared
/// after a case fold). Anything else yields
/// `"{first word} is not an experience report."` (blob `0x103e64748`).
pub fn run(src: &dyn Source, who: &Who, text: &str, now: i64) -> Reply {
    let trimmed = text.trim();
    let after = trimmed.trim_start_matches([';', '.']).trim_start();
    let (word, rest) = split_word(after);
    let is_exp = word.len() == 3
        && word
            .bytes()
            .zip(b"exp")
            .all(|(byte, want)| byte.eq_ignore_ascii_case(want));
    if !is_exp {
        return Reply::Say(format!("{} is not an experience report.", word));
    }
    match parse(rest, now) {
        Ok(parsed) => read(src, who, &parsed, now),
        Err(message) => Reply::Say(message),
    }
}

/// `command::read(src, who, parsed, now) -> Reply` — `@0x1012cb42c` (**full**
/// for the dispatch, partly for the query bounds).
///
/// The window each report gets was read straight out of the jump targets:
///
/// | command | rows |
/// |---|---|
/// | `session` | `pulses_between(now - 86_400, i64::MAX)` |
/// | `today` | `pulses_between(midnight(now), i64::MAX)` |
/// | `hour` | `pulses_between(now - 3_600, i64::MAX)` |
/// | `daily` | `pulses_between(now - 864_000, i64::MAX)` |
/// | `last` | `last_pulses(10)` |
/// | `instant` | `last_pulses(1_000)`, keep the bounty/brooch/instant rows, keep ten |
/// | `timing` | `last_pulses(1_000)` |
/// | `level` | `last_pulses(1)` plus `pulses_between(midnight(now), i64::MAX)` |
/// | `favor`/`resource`/`suffused` | `latest_snapshots()` + `snapshots(which, …)` |
/// | `report` | `pulses_between(from, to + 1)` |
pub fn read(src: &dyn Source, who: &Who, parsed: &Parsed, now: i64) -> Reply {
    let name = who.name.as_str();
    let midnight = eastern::midnight(&eastern::date(now));
    match parsed {
        Parsed::Help => Reply::Lines(HELP.iter().map(|line| line.to_string()).collect()),
        Parsed::Snapshots { which } => {
            // The binary fetches `latest_snapshots()` (0x10168c174) for the
            // "as of" rows and `snapshots(which, since, until)`
            // (0x10168cdac) for today's movement.
            let latest = match src.latest_snapshots() {
                Ok(snaps) => snaps,
                Err(message) => return Reply::Say(message),
            };
            let today = match src.snapshots(*which, midnight, i64::MAX) {
                Ok(snaps) => snaps,
                Err(message) => return Reply::Say(message),
            };
            Reply::Lines(report::snapshots(*which, &latest, &today))
        }
        // The three mutating commands are `answer`'s business (the binary's
        // `read` never sees them: they carry a 24-byte payload of their own);
        // showing the usage line is the safest answer here.
        Parsed::AddGain(_) => Reply::Lines(vec![HELP[9].to_string()]),
        Parsed::Clean => Reply::Lines(vec![HELP[10].to_string()]),
        Parsed::Reset | Parsed::ConfirmReset => Reply::Lines(vec![HELP[10].to_string()]),
        _ => {
            let fetched = match parsed {
                Parsed::Session => src.pulses_between(now - 86_400, i64::MAX),
                Parsed::Today => src.pulses_between(midnight, i64::MAX),
                Parsed::Hour => src.pulses_between(now - 3_600, i64::MAX),
                Parsed::Daily => src.pulses_between(now - 864_000, i64::MAX),
                Parsed::Last { instant } => src.last_pulses(if *instant { 1_000 } else { 10 }),
                Parsed::Timing => src.last_pulses(1_000),
                Parsed::Level => src.last_pulses(1),
                Parsed::Report { from, to } => src.pulses_between(*from, to.saturating_add(1)),
                _ => Ok(Vec::new()),
            };
            let pulses = match fetched {
                Ok(pulses) => pulses,
                Err(message) => return Reply::Say(message),
            };
            let lines = match parsed {
                Parsed::Session => report::session(name, &pulses),
                Parsed::Today => report::today(name, &pulses),
                Parsed::Hour => report::hour(name, &pulses),
                Parsed::Daily => report::daily(&pulses, now),
                Parsed::Last { instant } => {
                    let rows: Vec<Pulse> = if *instant {
                        let mut kept: Vec<Pulse> = pulses
                            .iter()
                            .filter(|pulse| pulse.source != PulseSource::Normal)
                            .cloned()
                            .collect();
                        kept.truncate(10);
                        kept
                    } else {
                        pulses
                    };
                    report::last(name, &rows, *instant)
                }
                Parsed::Timing => report::timing(&pulses),
                Parsed::Level => {
                    let newest = pulses.last();
                    let today = match src.pulses_between(midnight, i64::MAX) {
                        Ok(today) => today,
                        Err(message) => return Reply::Say(message),
                    };
                    report::level(name, newest, &today, now)
                }
                Parsed::Report { from, to } => report::range(name, &pulses, *from, *to),
                _ => Vec::new(),
            };
            Reply::Lines(lines)
        }
    }
}

/// `command::answer(src, who, parsed, now) -> Reply` — `@0x1012cc990`
/// (**full**).
///
/// A second dispatcher that `run` does not use: it opens the ledger reader
/// (`hydra_ledger::Ledger::reader`, tag `3` = the `rusqlite` error) and then
/// switches on the *same* `Parsed` discriminant (`ldrb w8, [x24]`) for the four
/// mutating commands; everything else falls through to [`read`]
/// (`0x1012cb42c`, the `bl` at `0x1012ccc10`). The four arms, verbatim:
///
/// * `addgain n` — `Reader::last_pulses(1)` for the current absorbed figures
///   (`Pulse + 0xa0`/`+0xa8`, zero when there is no row yet: the cold block at
///   `0x1012cce7c` sets both to 0), then a fresh row
///   `{ at: now, exp, axp, exp_gained: n, axp_gained: 0, bonus: 0, source: 4 }`
///   (`strb w8, #0x4` at `+0xc8` — source `manual`) through
///   `Writer::try_record`. Reply: `Added an experience gain of {n} by hand.`
///   (`0x103e647ac`), or the writer's `Closed` text.
/// * `clean` — `prune_pulses(false)`; reply
///   `Dropped {n} rows with no experience gained.` (`0x103e647d5`).
/// * `reset` — no write at all: the two-line prompt
///   `This drops every experience row of this character's, for good.`
///   (`0x10240d4b4`, 62 B) and `To go on, say: exp confirm_reset`
///   (`0x10240d4f2`, 32 B).
/// * `confirm_reset` — `prune_pulses(true)`; reply
///   `Dropped {n} rows; the next bar starts over.` (`0x103e64801`).
pub fn answer(src: &mut dyn Source, who: &Who, parsed: &Parsed, now: i64) -> Reply {
    match parsed {
        Parsed::AddGain(gain) => {
            let latest = src.last_pulses(1).unwrap_or_default();
            let (exp, axp) = match latest.first() {
                Some(pulse) => (pulse.exp, pulse.axp),
                None => (0, 0),
            };
            let pulse = Pulse {
                at: now,
                exp,
                axp,
                exp_gained: *gain,
                axp_gained: 0,
                bonus: 0,
                source: PulseSource::Manual,
                ..Pulse::default()
            };
            match src.try_record(pulse) {
                Ok(()) => Reply::Say(format!("Added an experience gain of {} by hand.", gain)),
                Err(message) => Reply::Say(message),
            }
        }
        Parsed::Clean => match src.prune_pulses(false) {
            Ok(dropped) => Reply::Say(format!(
                "Dropped {} rows with no experience gained.",
                dropped
            )),
            Err(message) => Reply::Say(message),
        },
        Parsed::Reset => Reply::Lines(vec![
            "This drops every experience row of this character's, for good.".to_string(),
            "To go on, say: exp confirm_reset".to_string(),
        ]),
        Parsed::ConfirmReset => match src.prune_pulses(true) {
            Ok(dropped) => Reply::Say(format!(
                "Dropped {} rows; the next bar starts over.",
                dropped
            )),
            Err(message) => Reply::Say(message),
        },
        other => read(src, who, other, now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::Source as PulseSource;

    /// A ledger stub: pulses handed straight back.
    #[derive(Default)]
    struct Fake {
        pulses: Vec<Pulse>,
        snaps: Vec<Snapshot>,
        last: Option<usize>,
        between: Vec<(i64, i64)>,
    }

    impl Source for Fake {
        fn last_pulses(&self, limit: usize) -> Result<Vec<Pulse>, String> {
            let mut tail = self.pulses.clone();
            if tail.len() > limit {
                tail.drain(..tail.len() - limit);
            }
            Ok(tail)
        }
        fn pulses_between(&self, from: i64, to: i64) -> Result<Vec<Pulse>, String> {
            Ok(self
                .pulses
                .iter()
                .filter(|p| p.at >= from && p.at <= to)
                .cloned()
                .collect())
        }
        fn latest_snapshots(&self) -> Result<Vec<Snapshot>, String> {
            Ok(self.snaps.clone())
        }
        fn snapshots(&self, which: u8, from: i64, to: i64) -> Result<Vec<Snapshot>, String> {
            let _ = (which, from, to);
            Ok(self.snaps.clone())
        }
        fn try_record(&mut self, pulse: Pulse) -> Result<(), String> {
            self.pulses.push(pulse);
            Ok(())
        }
        fn prune_pulses(&mut self, drop_all: bool) -> Result<usize, String> {
            let before = self.pulses.len();
            if drop_all {
                self.pulses.clear();
            } else {
                self.pulses.retain(|p| p.exp_gained != 0);
            }
            Ok(before - self.pulses.len())
        }
    }

    /// 2026-07-20 13:05:52 Eastern.
    const T: i64 = 1_784_567_152;

    #[test]
    fn parse_maps_the_help_words() {
        assert_eq!(parse("today", T), Ok(Parsed::Today));
        assert_eq!(parse("  LAST ", T), Ok(Parsed::Last { instant: false }));
        assert_eq!(parse("instant", T), Ok(Parsed::Last { instant: true }));
        assert_eq!(parse("resource", T), Ok(Parsed::Snapshots { which: 1 }));
        assert_eq!(parse("clean", T), Ok(Parsed::Clean));
        assert_eq!(parse("addgain 1200", T), Ok(Parsed::AddGain(1_200)));
        assert!(parse("", T).unwrap_err().starts_with("exp session"));
        assert_eq!(
            parse("wat", T),
            Err("wat is not an experience report.".to_string())
        );
    }

    #[test]
    fn parse_time_accepts_the_three_spellings() {
        // noon today
        let midnight = eastern::midnight(&eastern::date(T));
        assert_eq!(parse_time("13:05", T), Some(midnight + 13 * 3_600 + 300));
        assert_eq!(parse_time("13:05:52", T), Some(midnight + 13 * 3_600 + 352));
        assert_eq!(
            parse_time("07-20 13:05", T),
            Some(midnight + 13 * 3_600 + 300)
        );
        assert_eq!(
            parse_time("2026-07-20 13:05:52", T),
            Some(midnight + 13 * 3_600 + 352)
        );
        assert_eq!(parse_time("25:00", T), None);
        assert_eq!(parse_time("nonsense", T), None);
        assert_eq!(parse_time("", T), None);
    }

    #[test]
    fn run_rejects_other_commands() {
        let src = Fake::default();
        let who = Who::new("Sindar");
        assert_eq!(
            run(&src, &who, "look", T),
            Reply::Say("look is not an experience report.".to_string())
        );
        assert_eq!(
            run(&src, &who, "  .exp   today", T),
            Reply::Lines(crate::report::today("Sindar", &[]))
        );
        // "exp" alone prints the help.
        assert_eq!(run(&src, &who, ".exp", T), Reply::Say(HELP.join("\n")));
    }

    #[test]
    fn answer_records_a_manual_gain_on_top_of_the_latest_row() {
        let mut src = Fake {
            pulses: vec![Pulse {
                at: T - 60,
                exp: 1_000,
                axp: 44,
                exp_gained: 100,
                ..Pulse::default()
            }],
            ..Fake::default()
        };
        let who = Who::new("Sindar");
        assert_eq!(
            answer(&mut src, &who, &Parsed::AddGain(500), T),
            Reply::Say("Added an experience gain of 500 by hand.".to_string())
        );
        let recorded = src.pulses.last().expect("a row");
        assert_eq!(recorded.at, T);
        assert_eq!(recorded.exp, 1_000);
        assert_eq!(recorded.axp, 44);
        assert_eq!(recorded.exp_gained, 500);
        assert_eq!(recorded.axp_gained, 0);
        assert_eq!(recorded.source, PulseSource::Manual);
    }

    #[test]
    fn answer_asks_before_resetting() {
        let mut src = Fake::default();
        let who = Who::new("Sindar");
        assert_eq!(
            answer(&mut src, &who, &Parsed::Reset, T),
            Reply::Lines(vec![
                "This drops every experience row of this character's, for good.".to_string(),
                "To go on, say: exp confirm_reset".to_string(),
            ])
        );
        assert_eq!(
            answer(&mut src, &who, &Parsed::Clean, T),
            Reply::Say("Dropped 0 rows with no experience gained.".to_string())
        );
        assert_eq!(
            answer(&mut src, &who, &Parsed::ConfirmReset, T),
            Reply::Say("Dropped 0 rows; the next bar starts over.".to_string())
        );
        assert_eq!(parse("confirm_reset", T), Ok(Parsed::ConfirmReset));
    }

    #[test]
    fn read_dispatches_to_the_reports() {
        let src = Fake {
            pulses: vec![
                Pulse {
                    at: T - 60,
                    exp: 1_000,
                    exp_gained: 100,
                    source: PulseSource::Normal,
                    ..Pulse::default()
                },
                Pulse {
                    at: T,
                    exp: 1_100,
                    exp_gained: 100,
                    source: PulseSource::Bounty,
                    ..Pulse::default()
                },
            ],
            ..Fake::default()
        };
        let who = Who::new("Sindar");
        match read(&src, &who, &Parsed::Last { instant: true }, T) {
            Reply::Lines(lines) => {
                assert_eq!(lines[0], "Last 1 bounty/brooch/instant pulses:");
                assert!(lines[2].contains("bounty"));
            }
            other => panic!("{other:?}"),
        }
        match read(&src, &who, &Parsed::Hour, T) {
            Reply::Lines(lines) => assert_eq!(lines[0], "Last hour for Sindar:"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            read(&src, &who, &Parsed::Help, T).into_lines().len(),
            12
        );
    }
}
