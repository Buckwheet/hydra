//! What the store's own pages said, kept for the ledger.
//!
//! Recovered from `hydra_rewards::store::kept` (`0x1014d1204`).

use hydra_net::eastern;
use hydra_net::store::page;

/// The game a store answer belongs to.
///
/// **ESTABLISHED**: the two-byte literals `GS` (`mov w8, #0x5347`) and `DR`
/// (`mov w9, #0x5244`) at `0x1014d1350`, chosen by `csel w8, w9, w8, ne` on
/// the second argument — so "is this DragonRealms?" decides the code.
pub fn game_code(dragonrealms: bool) -> &'static str {
    if dragonrealms {
        "DR"
    } else {
        "GS"
    }
}

/// A store page, as the claim path hands it over.
///
/// **ESTABLISHED**: the third argument of `kept` holds a byte flag at
/// `+0x48` (`0x1014d123c`) and, depending on it, a `&str` at `+0x08`/`+0x10`
/// (`0x1014d1258`, `0x1014d126c`) or at `+0x38`/`+0x40` (`0x1014d1248`) — two
/// variants of one enum.
///
/// **UNKNOWN**: which variant means what.  The recovered keying of
/// `hydra_net::store::page::next_in`/`next_available` only ever uses the text
/// and the flag, so this reconstruction keeps those two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Page<'a> {
    /// The page text the answer was read from.
    pub text: &'a str,
    /// The flag the answer carried (the variant selector at `+0x48`).
    pub claimed: bool,
}

/// The row `kept` answers: what the store said, and when the next is.
///
/// **ESTABLISHED**: 0x80 bytes, written field by field at `0x1014d1380` …
/// `0x1014d13b4`:
///
/// | offset | field | evidence |
/// |---|---|---|
/// | `+0x00` | [`Noted::page`]'s pointer | `stp x24, x27, [x21]` — the page text and the time |
/// | `+0x08` | [`Noted::next`] | `x27`, the instant the page's own dates work out to |
/// | `+0x10` | [`Noted::what`] | a fresh `String` copied from the first argument |
/// | `+0x30` | [`Noted::game`] | `GS`/`DR`, two bytes, length `2` at `+0x38` |
/// | `+0x40` | [`Noted::account`] | cloned from the page's first `String` |
/// | `+0x58` | [`Noted::character`] | cloned from the page's second `String` |
/// | `+0x70` | [`Noted::now`] | the fourth argument |
/// | `+0x78` | [`Noted::claimed`] | the page's flag byte |
///
/// **UNKNOWN**: the `2` written at `+0x28`; this reconstruction leaves it out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Noted {
    /// The page's text.
    pub page: String,
    /// When the free SimuCoins come round again, unix seconds (`0` = unknown).
    pub next: i64,
    /// What was asked for.
    pub what: String,
    /// `GS` or `DR`.
    pub game: String,
    /// The account the page answered for.
    pub account: String,
    /// The character the page answered for.
    pub character: String,
    /// The clock the answer was read at.
    pub now: i64,
    /// Whether the page carried the claimed flag.
    pub claimed: bool,
}

/// Work out when the store next hands out the free SimuCoins, from the page.
///
/// `store::kept(what: &str, dragonrealms: bool, page: &Page, now: i64) ->
/// Noted` — `@0x1014d1204` (**partly**: the date arithmetic and the row are
/// recovered, the page enum is not).
///
/// The emitted body asks `hydra_net::store::page::next_in` first
/// (`0x1014d1280`) and, when the page gives a relative time, adds `now` to
/// take the instant (`0x1014d1290`: `add x8, x8, x20; add x27, x8, x9`).
/// Otherwise it takes `eastern::date(now)` (`0x1014d12ac`) and asks
/// `page::next_available` for that date (`0x1014d12c0`); a date it does
/// answer is turned into that day's midnight (`0x1014d12e4`,
/// `eastern::midnight`), and no answer at all leaves the instant at zero
/// (`0x1014d13d8`: `mov x24, #0`).
///
/// **Departure**: `hydra_net::store::page::next_available` in this workspace
/// takes only the page text, so the date is not passed through.
pub fn kept(what: &str, dragonrealms: bool, page: &Page<'_>, now: i64) -> Noted {
    let next = match page::next_in(page.text, now) {
        // `next_in` answers the wait, not the instant (`page.rs`:
        // `next_available(page).map(|at| (at - now).max(0))`), and the emitted
        // body adds `now` back on (`add x8, x8, x20` at `0x1014d1290`).
        Some(delta) => now + delta,
        None => match page::next_available(page.text) {
            Some(at) => at,
            None => 0,
        },
    };
    Noted {
        page: page.text.to_string(),
        next,
        what: what.to_string(),
        game: game_code(dragonrealms).to_string(),
        account: String::new(),
        character: String::new(),
        now,
        claimed: page.claimed,
    }
}

/// The day the store's dates fall on, for a row's tooltip.
///
/// **ESTABLISHED**: the reports print the store's instants through
/// `eastern::date`/`eastern::format` (`0x1014cf274`, `0x1014cf29c`).
pub fn on(at: i64) -> String {
    eastern::format(&eastern::date(at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_code_is_two_bytes() {
        assert_eq!(game_code(false), "GS");
        assert_eq!(game_code(true), "DR");
        assert_eq!(game_code(false).len(), 2);
    }

    #[test]
    fn a_page_without_dates_leaves_the_instant_at_zero() {
        let noted = kept("main", false, &Page::default(), 1_705_348_200);
        assert_eq!(noted.next, 0);
        assert_eq!(noted.what, "main");
        assert_eq!(noted.game, "GS");
        assert_eq!(noted.now, 1_705_348_200);
        assert!(!noted.claimed);
    }

    #[test]
    fn a_page_with_a_date_gives_the_instant() {
        // the shape hydra_net::store::page reads: a "next" date on the page
        let text = "Your next free SimuCoins are available on 2/1/2024.";
        let noted = kept("main", true, &Page { text, claimed: true }, 1_705_348_200);
        assert_eq!(noted.next, eastern::midnight(&eastern::Date::new(2024, 2, 1)));
        assert_eq!(noted.game, "DR");
        assert!(noted.claimed);
        assert_eq!(on(noted.next), "2024-02-01");
    }
}
