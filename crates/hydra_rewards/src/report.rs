//! The `rewards …` chat reports.
//!
//! Recovered from `hydra_rewards::report::{own, named, streak, run, answer,
//! store, everyone}` (`0x1014ce818`, `0x1014cf05c`, `0x1014d0110`,
//! `0x1014cecec`, `0x1014cf570`, `0x1014cf140`, `0x1014d0260`).  Every message
//! below was decoded from its length-prefixed format blob; the blobs live at
//! `0x103e6b790`, `0x103e6b7bd`, `0x103e6b7e6`, `0x103e6b81f`, `0x103e6b83b`,
//! `0x103e6b871`, `0x103e6b87f`, `0x103e6b887`, `0x103e6b8a7`, `0x10241b06f`,
//! `0x10241b08f`, `0x10241b0ab` and `0x10241e2xx`.
//!
//! The reports read the ledger, which is not part of this reconstruction, so
//! the four reads the recovered bodies make are behind [`Source`] — the same
//! seam `hydra_exp` uses for its `Reader`/`Writer`.

use crate::row::Who;
use crate::watcher::Heard;
use hydra_net::eastern;

/// The plain report reads this many of the character's latest rewards
/// (`mov w2, #0xa` before `Reader::rewards`, `0x1014cf9a0`).
pub const LAST: u32 = 10;

/// What the reports need from `hydra_ledger`.
///
/// **ESTABLISHED**: the recovered `answer` calls
/// `hydra_ledger::reader::Reader::{rewards, latest_store_visits,
/// latest_store_claims}` (`0x10168c874`, `0x10168c4b0`, `0x10168c3b0`) and
/// `hydra_ledger::reader::<impl Ledger>::reader` (`0x10168e934`); the failure
/// arm formats the `rusqlite::Error` through its `Display`
/// (`0x10169b550`) into the `Err` of the `Result<Vec<String>, String>` these
/// functions answer.
///
/// **Departure**: `hydra_ledger` is not reconstructed, so the four reads are
/// declared here and the error is `String` instead of `rusqlite::Error`.
pub trait Source {
    /// The character's latest rewards, newest first.
    fn rewards(&self, who: &Who, limit: u32) -> Result<Vec<Heard>, String>;
    /// Every character's latest reward.
    fn latest(&self) -> Result<Vec<Heard>, String>;
    /// Each account's latest store standing.
    fn store_claims(&self) -> Result<Vec<Store>, String>;
    /// Every account's last visit to the store.
    fn store_visits(&self) -> Result<Vec<Visit>, String>;
}

/// A `&Source` is a `Source`, so callers can pass either.
impl<T: Source + ?Sized> Source for &T {
    fn rewards(&self, who: &Who, limit: u32) -> Result<Vec<Heard>, String> {
        (**self).rewards(who, limit)
    }
    fn latest(&self) -> Result<Vec<Heard>, String> {
        (**self).latest()
    }
    fn store_claims(&self) -> Result<Vec<Store>, String> {
        (**self).store_claims()
    }
    fn store_visits(&self) -> Result<Vec<Visit>, String> {
        (**self).store_visits()
    }
}

/// The "given before Hydra saw it" text (`0x104437120`, 25 bytes).
pub const GIVEN_BEFORE: &str = "given before Hydra saw it";
/// What the store report prints for a missing balance
/// (`0x10241b0aa`, the one-byte literal `"-"`).
pub const NO_BALANCE: &str = "-";
/// What the store report prints when a visit has no next date
/// (`0x1014cf30c`: the eight bytes `not told`).
pub const NOT_TOLD: &str = "not told";
/// What the store report prints when no visit is on file
/// (`0x1014cf2b8`, the ten bytes `never seen`).
pub const NEVER_SEEN: &str = "never seen";

/// The state of one account's store standing, as `rewards store` reads it.
///
/// **ESTABLISHED**: 0x80-byte rows.  `report::store` reads
/// `+0x00`/`+0x08` (`cmp x8, #1` then `[+0x08]` as the "next" unix second,
/// `0x1014cf284`), `+0x18`/`+0x20` (the account, compared with the visit's
/// `+0x00`/`+0x08` at `0x1014cf22c`), `+0x30`/`+0x38` (the character,
/// `0x1014cf24c`) and `+0x60`/`+0x68` (the balance text, `0x1014cf2a8`).
///
/// **UNKNOWN**: the rest of the row; `hydra_ledger::row::StoreClaim` is not in
/// the symbol table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Store {
    /// When the store next hands out the free SimuCoins, unix seconds.
    pub next: Option<i64>,
    /// The account.
    pub account: String,
    /// The character.
    pub character: String,
    /// The balance as the store said it (already text).
    pub balance: Option<String>,
}

/// One visit to the store's website.
///
/// **ESTABLISHED**: 0x80-byte rows; `report::store` reads `+0x00`/`+0x08` as
/// the account (`&str`, compared at `0x1014cf22c`), `+0x18`/`+0x20` as the
/// character (`0x1014cf24c`) and `+0x38` as the unix second of the visit
/// (`0x1014cf26c`, `eastern::date` on it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Visit {
    pub account: String,
    pub character: String,
    /// When the visit happened, unix seconds.
    pub at: i64,
}

/// This character's report: is today's reward in, and the last ten.
///
/// `report::own(who: &Who, today: &str, history: &[Heard]) -> Vec<String>` —
/// `@0x1014ce818` (**full**).
///
/// ```text
/// own(who, "2024-01-15", &[row]) == [
///   "Aelith: today's login reward (2024-01-15) is in, at 14:50 Eastern: \
///      a brass ring, day 4, 9 bridges.",
///   "a brass ring, day 4, 9 bridges  2024-01-15",
/// ]
/// ```
///
/// * with no history at all the first line is
///   `{}: today's login reward ({}) is not in yet.` (`0x103e6b790`);
/// * with a history whose newest `day` is not today, the same line
///   (`0x1014ce858` compares the newest row's `day_text` with `today`);
/// * with today's reward in: when its `reward` is empty
///   `{}: today's login reward ({}) is in: {}.` with
///   [`GIVEN_BEFORE`] (`0x103e6b7bd`, taken when `[row+0x78]` — the reward
///   `String`'s length — is zero, `0x1014cea3c`); otherwise
///   `{}: today's login reward ({}) is in, at {} Eastern: {}{}.`
///   (`0x103e6b7e6`) with the clock from `row.stamp.game_time`
///   (`0x1014cea44`: `ldr x22, [x20, #0xb8]`, `eastern::offset`, then
///   `{:02}:{:02}`), the reward and [`streak`];
/// * then `No login reward is on file.` (`0x10241b046`) when the history is
///   empty, else one `{}  {}` line per row (`0x103e2b3fb`): [`named`] and the
///   row's day text (`0x1014ce950` advances by `0x50` into the row, `0x1014ce938`
///   by the remaining `0x70` — the 0xc0 stride).
pub fn own(who: &Who, today: &str, history: &[Heard]) -> Vec<String> {
    let mut out = Vec::new();
    match history.first() {
        Some(row) if row.day_text == *today => {
            if row.reward.is_empty() {
                out.push(format!(
                    "{}: today's login reward ({}) is in: {}.",
                    who.character, today, GIVEN_BEFORE
                ));
            } else {
                out.push(format!(
                    "{}: today's login reward ({}) is in, at {} Eastern: {}{}.",
                    who.character,
                    today,
                    clock(row.stamp.game_time),
                    row.reward,
                    streak(row)
                ));
            }
        }
        _ => out.push(format!(
            "{}: today's login reward ({}) is not in yet.",
            who.character, today
        )),
    }
    if history.is_empty() {
        out.push("No login reward is on file.".to_string());
    } else {
        for row in history {
            out.push(format!("{}  {}", named(row), row.day_text));
        }
    }
    out
}

/// `hh:mm` on the game's clock.
///
/// **ESTABLISHED** (`0x1014cea44` … `0x1014ceadc`): `eastern::offset` is added
/// to the unix second, the remainder of `86400` (`msub` against
/// `0x1_5180`) is taken (rounded towards negative infinity by the
/// `csel …, mi` at `0x1014cea80`), then `/3600` and `%3600 / 60`; the two
/// numbers are formatted with the `{:02}:{:02}` blob at `0x10241af01`.
pub fn clock(at: i64) -> String {
    let secs = at + eastern::offset(at);
    let mut day = secs % 86_400;
    if day < 0 {
        day += 86_400;
    }
    format!("{:02}:{:02}", day / 3600, (day % 3600) / 60)
}

/// The streak suffix of a row: `, day 4, 9 bridges`.
///
/// `report::streak(&row) -> String` — `@0x1014d0110` (**full**).
///
/// The two `Option`s of the row are printed independently: `", day {}"`
/// (`0x103e6b87f`) when `row.day` is `Some`, then `", {} bridge{}"`
/// (`0x103e6b871`) when `row.bridges` is `Some`, the last `{}` being `"s"`
/// unless the count is exactly 1 (the one-byte literal `s` at `0x10241b26e`,
/// selected by `cset w9, ne` / `csinc x8, x8, xzr, ne` at `0x1014d01a0` —
/// the empty `&str` is the dangling `ptr = 1, len = 0` pair).  The two parts
/// are collected into one `String` (`String::from_iter` at `0x1014efa80`).
pub fn streak(row: &Heard) -> String {
    let mut out = String::new();
    if let Some(day) = row.day {
        out.push_str(&format!(", day {}", day));
    }
    if let Some(bridges) = row.bridges {
        out.push_str(&format!(
            ", {} bridge{}",
            bridges,
            if bridges == 1 { "" } else { "s" }
        ));
    }
    out
}

/// How a row is named in a list: its reward and streak, or [`GIVEN_BEFORE`].
///
/// `report::named(&row) -> String` — `@0x1014cf05c` (**full**).
///
/// ```text
/// named(&row_with_reward) == "a brass ring, day 4, 9 bridges"
/// named(&empty_row)       == "given before Hydra saw it"
/// ```
///
/// The reward `String`'s length at `[row+0x78]` decides (`0x1014cf074`);
/// otherwise `format!("{}{}", reward, streak)` with the blob `{}{}` at
/// `0x103e2252a` (`0x1014cf0ac`).
pub fn named(row: &Heard) -> String {
    if row.reward.is_empty() {
        GIVEN_BEFORE.to_string()
    } else {
        format!("{}{}", row.reward, streak(row))
    }
}

/// Each account's store SimuCoins.
///
/// `report::store(claims: &[Store], visits: &[Visit]) -> Vec<String>` —
/// `@0x1014cf140` (**full** for the wording and the matching; the row layouts
/// are the ones quoted on [`Store`]/[`Visit`]).
///
/// The first line is `No store visit is on file.` (`0x10241b075`) when there
/// are no rows (`0x1014cf168`), else the header `Store SimuCoins by
/// account:` (`0x10241b08f`, 27 bytes) and one line per [`Store`]:
///
/// ```text
/// format!("{:<14} {}  balance {}  last claimed {}  next {}", …)
/// ```
///
/// (the blob at `0x10241b0ab`: `{:<14}`, `" "`, `{}`, `"  balance "`, `{}`,
/// `"  last claimed "`, `{}`, `"  next "`, `{}`).  The account is matched
/// against the visits' account *and* character, case-sensitively, by
/// `memcmp` (`0x1014cf244`, `0x1014cf264`); a matching visit lends its date
/// (`0x1014cf26c`), otherwise the column is [`NEVER_SEEN`].  A missing
/// balance prints [`NO_BALANCE`] (`0x1014cf334`); a `next` of `None` prints
/// [`NOT_TOLD`] (`0x1014cf30c`), otherwise its date.
pub fn store(claims: &[Store], visits: &[Visit]) -> Vec<String> {
    if claims.is_empty() {
        return vec!["No store visit is on file.".to_string()];
    }
    let mut out = vec!["Store SimuCoins by account:".to_string()];
    for claim in claims {
        let visit = visits
            .iter()
            .find(|v| v.account == claim.account && v.character == claim.character);
        let last_claimed = match visit {
            Some(v) => eastern::format(&eastern::date(v.at)),
            None => NEVER_SEEN.to_string(),
        };
        let next = match claim.next {
            Some(at) => eastern::format(&eastern::date(at)),
            None => NOT_TOLD.to_string(),
        };
        let balance = claim.balance.as_deref().unwrap_or(NO_BALANCE);
        out.push(format!(
            "{:<14} {}  balance {}  last claimed {}  next {}",
            claim.account, claim.character, balance, last_claimed, next
        ));
    }
    out
}

/// Every character's latest login reward, and whether today's is in.
///
/// `report::everyone(today: &str, rows: &[Heard]) -> Vec<String>` —
/// `@0x1014d0260` (**partly**: the header and the per-row pieces are
/// recovered, the row layout is inferred).
///
/// The header is `Login rewards, today is {}:` (`0x103e6b887`, 24 bytes of
/// text); each row prints [`named`] next to either `today: in` or
/// `today: not yet` (`0x10241b26f` / `0x10241b27d`, both in the literal pool
/// next to the header).
///
/// **UNKNOWN**: the column order of a row.  The recovered
/// `0x1014d0260`-range blobs are `{}` and `{}{}` only, so the row is
/// `"{}  {}"` of the character and the named reward, then the marker.
pub fn everyone(today: &str, rows: &[Heard]) -> Vec<String> {
    let mut out = vec![format!("Login rewards, today is {}:", today)];
    if rows.is_empty() {
        out.push("No login reward is on file.".to_string());
        return out;
    }
    for row in rows {
        out.push(format!(
            "{}  {}  {}",
            row.character,
            named(row),
            if row.day_text == *today {
                "today: in"
            } else {
                "today: not yet"
            }
        ));
    }
    out
}

/// The word the chat command starts with, and the seven bytes `run` compares
/// (`0x1014cef14`: `r`,`e`,`w`,`a`,`r`,`d`,`s`, folded to lower case).
pub const WORD: &str = "rewards";

/// Answer one `rewards …` request.
///
/// `report::answer(source: &Source, who: &Who, query: &str, now: i64) ->
/// Result<Vec<String>, String>` — `@0x1014cf570` (**full** for the dispatch
/// and the messages; the ledger reads are the [`Source`] seam).
///
/// `now` is the unix second the report is made at: the body calls
/// `eastern::date`/`eastern::format` on its fifth argument
/// (`0x1014cf874`, `0x1014cf884`) to build the `today` every message quotes.
///
/// * no word at all → [`own`] over `source.rewards(who, LAST)` (`0x1014cf9a4`);
/// * `all` (and, as an alias, the catalogue's name `everyone`) → [`everyone`]
///   (`0x1014cfe7c`);
/// * `store` (`0x1014cfa3c` compares the word with the four bytes `stor` plus
///   `e`) → [`store`] over `store_claims()` and `store_visits()`
///   (`0x1014cfa5c`, `0x1014cfa8c`);
/// * anything else → `Err("`{}` is not a rewards word; rewards help lists
///   them.")` (`0x103e6b83b`).
///
/// The `Err` arm is the `Result`'s `String`; a ledger failure is that error's
/// own text (`0x1014cf8e0` formats the `rusqlite::Error`).
///
/// **Partly inferred**: the emitted dispatch is a numbered `match` (`x19` is
/// compared with `3`, `4`, `5` and with `isize::MIN` for "no word"); the
/// mapping of `3`/`4` to `all`/`everyone` is a guess — only `5` is pinned
/// (`store`).
pub fn answer(
    source: &dyn Source,
    who: &Who,
    query: &str,
    now: i64,
) -> Result<Vec<String>, String> {
    let today = eastern::format(&eastern::date(now));
    let word = query.split_whitespace().next().unwrap_or("");
    match word.to_lowercase().as_str() {
        "" => Ok(own(who, &today, &source.rewards(who, LAST)?)),
        "all" | "everyone" => Ok(everyone(&today, &source.latest()?)),
        "store" => Ok(store(&source.store_claims()?, &source.store_visits()?)),
        other => Err(format!(
            "`{}` is not a rewards word; rewards help lists them.",
            other
        )),
    }
}

/// The chat command's entry point: `rewards …`.
///
/// `report::run(source: &Source, who: &Who, what: &str, now: i64) ->
/// Result<Vec<String>, String>` — `@0x1014cecec` (**full**).
///
/// The line is trimmed, leading whitespace, `;` and `.` are skipped
/// (`0x1014ced90`, `0x1014cedb0`: `cmp w12, #0x3b` / `cmp w12, #0x2e`), and
/// its first seven characters must be [`WORD`], folded to lower case
/// (`0x1014cef14` … `0x1014cefe4`); then the rest is handed to [`answer`]
/// (`0x1014cf008` is a tail call).  A line that does not name the command
/// answers `Err(format!("{} is not a rewards report.", what))`
/// (`0x103e6b81f`).
pub fn run(source: &dyn Source, who: &Who, what: &str, now: i64) -> Result<Vec<String>, String> {
    let trimmed = what.trim();
    let start = trimmed
        .trim_start_matches(|c: char| c.is_whitespace() || c == ';' || c == '.');
    let mut words = start.split_whitespace();
    let first = words.next().unwrap_or("");
    if !first.eq_ignore_ascii_case(WORD) {
        return Err(format!("{} is not a rewards report.", trimmed));
    }
    let rest = start
        .char_indices()
        .find(|&(i, _)| i >= first.len())
        .map(|(i, _)| start[i..].trim_start())
        .unwrap_or("");
    answer(source, who, rest, now)
}

/// The chat help, verbatim from `0x10241b0ec` … `0x10241e2xx`.
///
/// Three lines padded to a 17-character word column, and the footer that
/// explains where the reward comes from.  The shell's own help index lists it
/// as `rewards help` / `the daily login reward` (`0x102408d53`).
pub const HELP: &str = "\
rewards          this character: is today's login reward in, and the last ten
rewards all      every character's latest login reward, and whether today's is in
rewards store    each account's monthly store SimuCoins: the last claimed, the balance, and when the next is
The game's day turns at midnight Eastern. Nothing is ever sent to the game: the reward is read off the lines it says at login.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::Stamp;

    fn who() -> Who {
        Who::new("GS4", "Aelith")
    }

    fn row(day_text: &str, reward: &str) -> Heard {
        Heard {
            day: Some(4),
            bridges: Some(9),
            instance: "GS4".to_string(),
            character: "Aelith".to_string(),
            day_text: day_text.to_string(),
            reward: reward.to_string(),
            line: "You have earned the following reward: a brass ring".to_string(),
            said: String::new(),
            stamp: Stamp::new(1_705_348_200, 1_705_348_200),
        }
    }

    /// 2024-01-15 14:30 UTC
    const NOW: i64 = 1_705_348_200;

    #[derive(Default)]
    struct Fixture {
        history: Vec<Heard>,
        latest: Vec<Heard>,
        claims: Vec<Store>,
        visits: Vec<Visit>,
        fails: bool,
    }

    impl Source for Fixture {
        fn rewards(&self, _who: &Who, _limit: u32) -> Result<Vec<Heard>, String> {
            if self.fails {
                return Err("no such table: login_rewards".to_string());
            }
            Ok(self.history.clone())
        }
        fn latest(&self) -> Result<Vec<Heard>, String> {
            Ok(self.latest.clone())
        }
        fn store_claims(&self) -> Result<Vec<Store>, String> {
            Ok(self.claims.clone())
        }
        fn store_visits(&self) -> Result<Vec<Visit>, String> {
            Ok(self.visits.clone())
        }
    }

    #[test]
    fn the_clock_is_eastern_and_zero_padded() {
        assert_eq!(clock(1_705_348_200), "14:50");
        // 2024-07-01 04:05 UTC = 00:05 EDT
        assert_eq!(clock(1_719_806_700), "00:05");
        // 2023-01-01 05:00 UTC = 00:00 EST
        assert_eq!(clock(1_672_549_200), "00:00");
    }

    #[test]
    fn streak_prints_each_option() {
        let mut r = row("2024-01-15", "a ring");
        assert_eq!(streak(&r), ", day 4, 9 bridges");
        r.bridges = Some(1);
        assert_eq!(streak(&r), ", day 4, 1 bridge");
        r.day = None;
        r.bridges = None;
        assert_eq!(streak(&r), "");
        r.day = Some(1);
        assert_eq!(streak(&r), ", day 1");
    }

    #[test]
    fn named_falls_back_to_given_before() {
        assert_eq!(named(&row("2024-01-15", "a ring")), "a ring, day 4, 9 bridges");
        assert_eq!(named(&row("2024-01-15", "")), GIVEN_BEFORE);
    }

    #[test]
    fn own_says_when_todays_reward_is_in() {
        let lines = own(&who(), "2024-01-15", &[row("2024-01-15", "a brass ring")]);
        assert_eq!(
            lines,
            vec![
                "Aelith: today's login reward (2024-01-15) is in, at 14:50 Eastern: \
                 a brass ring, day 4, 9 bridges."
                    .to_string(),
                "a brass ring, day 4, 9 bridges  2024-01-15".to_string(),
            ]
        );
    }

    #[test]
    fn own_says_when_it_is_not_in() {
        let lines = own(&who(), "2024-01-16", &[row("2024-01-15", "a brass ring")]);
        assert_eq!(
            lines[0],
            "Aelith: today's login reward (2024-01-16) is not in yet."
        );
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn own_reports_an_empty_history() {
        let lines = own(&who(), "2024-01-16", &[]);
        assert_eq!(
            lines,
            vec![
                "Aelith: today's login reward (2024-01-16) is not in yet.".to_string(),
                "No login reward is on file.".to_string(),
            ]
        );
    }

    #[test]
    fn own_says_given_before_for_an_empty_reward() {
        let lines = own(&who(), "2024-01-15", &[row("2024-01-15", "")]);
        assert_eq!(
            lines[0],
            "Aelith: today's login reward (2024-01-15) is in: given before Hydra saw it."
        );
        assert_eq!(lines[1], "given before Hydra saw it  2024-01-15");
    }

    #[test]
    fn store_prints_its_header_and_one_row_an_account() {
        let claims = vec![
            Store {
                next: Some(1_706_000_000),
                account: "main".to_string(),
                character: "Aelith".to_string(),
                balance: Some("1,234".to_string()),
            },
            Store {
                next: None,
                account: "alt".to_string(),
                character: "Wynter".to_string(),
                balance: None,
            },
        ];
        let visits = vec![Visit {
            account: "main".to_string(),
            character: "Aelith".to_string(),
            at: 1_705_348_200,
        }];
        let lines = store(&claims, &visits);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Store SimuCoins by account:");
        assert_eq!(
            lines[1],
            "main           Aelith  balance 1,234  last claimed 2024-01-15  next 2024-01-23"
        );
        assert_eq!(
            lines[2],
            "alt            Wynter  balance -  last claimed never seen  next not told"
        );
    }

    #[test]
    fn store_with_nothing_on_file() {
        assert_eq!(store(&[], &[]), vec!["No store visit is on file.".to_string()]);
    }

    #[test]
    fn everyone_lists_each_character() {
        let lines = everyone("2024-01-15", &[row("2024-01-15", "a ring"), row("2024-01-14", "a cup")]);
        assert_eq!(lines[0], "Login rewards, today is 2024-01-15:");
        assert_eq!(
            lines[1],
            "Aelith  a ring, day 4, 9 bridges  today: in"
        );
        assert_eq!(
            lines[2],
            "Aelith  a cup, day 4, 9 bridges  today: not yet"
        );
    }

    #[test]
    fn answer_dispatches_on_the_first_word() {
        let mut f = Fixture::default();
        f.history = vec![row("2024-01-15", "a brass ring")];
        f.latest = vec![row("2024-01-15", "a brass ring")];
        assert_eq!(answer(&f, &who(), "", NOW).unwrap()[0], own(&who(), "2024-01-15", &f.history)[0]);
        assert_eq!(answer(&f, &who(), "all", NOW).unwrap()[0], "Login rewards, today is 2024-01-15:");
        assert_eq!(
            answer(&f, &who(), "everyone", NOW).unwrap()[0],
            "Login rewards, today is 2024-01-15:"
        );
        assert_eq!(answer(&f, &who(), "store", NOW).unwrap()[0], "No store visit is on file.");
        assert_eq!(
            answer(&f, &who(), "banana", NOW).unwrap_err(),
            "`banana` is not a rewards word; rewards help lists them."
        );
    }

    #[test]
    fn answer_reports_a_ledger_failure() {
        let mut f = Fixture::default();
        f.fails = true;
        assert_eq!(
            answer(&f, &who(), "", NOW).unwrap_err(),
            "no such table: login_rewards"
        );
    }

    #[test]
    fn run_needs_the_word_first() {
        let mut f = Fixture::default();
        f.history = vec![row("2024-01-15", "a brass ring")];
        assert_eq!(run(&f, &who(), "rewards", NOW).unwrap().len(), 2);
        assert_eq!(run(&f, &who(), "REWARDS all", NOW).unwrap()[0], "Login rewards, today is 2024-01-15:");
        assert_eq!(run(&f, &who(), "..; rewards store", NOW).unwrap()[0], "No store visit is on file.");
        assert_eq!(
            run(&f, &who(), "who is on?", NOW).unwrap_err(),
            "who is on? is not a rewards report."
        );
    }

    #[test]
    fn the_help_is_the_recovered_text() {
        let lines: Vec<&str> = HELP.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("rewards          this character:"));
        assert!(lines[1].starts_with("rewards all      every character's latest"));
        assert!(lines[2].starts_with("rewards store    each account's monthly"));
        assert!(lines[3].starts_with("The game's day turns at midnight Eastern."));
    }
}
