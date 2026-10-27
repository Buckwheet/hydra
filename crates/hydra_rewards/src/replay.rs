//! Replaying an old log for the login rewards it holds.
//!
//! Recovered from `hydra_rewards::replay::rewards` (`0x1014ca344`).

use crate::parse;
use crate::watcher::{Chunk, Said, Watcher};

/// The 27 bytes every replayed log is searched for.
///
/// **ESTABLISHED**: the five immediates at `0x1014ca380` … `0x1014ca3bc`
/// (`0x6165…`, `0x6e72…`, …) build `earned t` / `he follo` / `wing rew` /
/// `g reward`, and the search walks the whole text one byte at a time
/// (`add x13, x13, #1` at `0x1014ca3e4`, bounded by `len - 0x1b`).  It is the
/// tail of [`parse::PREFIX`] — a log may have the speaker's name in front of
/// it, so the replay looks for the announcement itself.
pub const ANNOUNCED: &str = "earned the following reward";

/// Every reward an old log holds.
///
/// `replay::rewards(text: &str) -> Vec<Heard>` — `@0x1014ca344` (**partly**:
/// the search, the frame replay and the row push are recovered; the row is
/// assembled from [`crate::watcher::heard`] here).
///
/// ```text
/// rewards("You say, \"You have earned the following reward: a brass ring\"")
///     == [ one Heard whose reward is "a brass ring" ]
/// ```
///
/// The emitted body answers an empty `Vec` straight away when the text is
/// shorter than 27 bytes or does not contain [`ANNOUNCED`]
/// (`0x1014ca3f4`: `(cap, ptr, len) = (0, 8, 0)`);
/// otherwise it builds a `hydra_protocol::parser::Parser` (the `push_bytes`
/// at `0x1014ca5xx` and `finish`), feeds every frame through
/// `hydra_rewards::watcher::Watcher::hear` (`0x1014ca5e8`) and keeps the
/// frames that produced a reward (the push at `0x1014ca64c`, element stride
/// `0x68` = [`Said`]).
///
/// The replay has no character and no clock of its own — the caller supplies
/// them, as the ledger needs both:
///
/// ```text
/// let rows = rewards(text).into_iter()
///     .filter_map(|said| heard(&said, &who, stamp))
///     .collect();
/// ```
///
/// **KEPT (Phase 1 seam)**: the original replayed the text through Hydra's own
/// protocol parser; this feeds it line by line — the same text reaches the
/// watcher for every log that has one line per `\n`, and the local
/// `hydra_protocol`'s `Frame` cannot carry the "line ends here" fact the
/// watcher needs (`NOTES.md` §5.2).
pub fn rewards(text: &str) -> Vec<Said> {
    if text.len() < parse::THANKS.len().max(ANNOUNCED.len()) || !text.contains(ANNOUNCED) {
        return Vec::new();
    }
    let mut watcher = Watcher::new();
    let mut out = Vec::new();
    for line in text.lines() {
        match watcher.hear(&Chunk::line(line)) {
            Said::Nothing => {}
            said => out.push(said),
        }
    }
    out
}

/// The rows of a replay, for a character and a clock.
pub fn rows<'a>(
    said: impl IntoIterator<Item = &'a Said>,
    who: &crate::row::Who,
    stamp: crate::row::Stamp,
) -> Vec<crate::watcher::Heard> {
    said.into_iter()
        .filter_map(|said| crate::watcher::heard(said, who, stamp))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::{Stamp, Who};

    #[test]
    fn a_log_with_a_reward_yields_it() {
        let log = "Siel says, \"Hello.\"\n\
                   You have earned the following reward: a brass ring\n\
                   You wave.\n";
        let said = rewards(log);
        assert_eq!(said.len(), 1);
        match &said[0] {
            Said::Reward { reward, line, .. } => {
                assert_eq!(reward, "a brass ring");
                assert_eq!(line, "You have earned the following reward: a brass ring");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_streak_line_is_replayed_too() {
        let log = "Thank you for logging into GemStone IV!  You have logged in \
                   4 consecutive days and crossed 9 login bridges.\n\
                   You have earned the following reward: a brass ring\n";
        let said = rewards(log);
        assert_eq!(said.len(), 1);
        // the streak updates the watcher's memory but is not a reward itself
        assert!(matches!(&said[0], Said::Reward { day: Some(4), bridges: Some(9), .. }));
    }

    #[test]
    fn a_log_without_the_announcement_is_not_parsed() {
        assert!(rewards("").is_empty());
        assert!(rewards("You wave.").is_empty());
        // the prefix alone is not enough either
        assert!(rewards("You have earned the following reward: ").is_empty());
    }

    #[test]
    fn the_rows_carry_the_character_and_the_clock() {
        let log = "You have earned the following reward: a brass ring";
        let said = rewards(log);
        let rows = rows(&said, &Who::new("GS4", "Aelith"), Stamp::new(1_705_348_200, 1_705_348_200));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].character, "Aelith");
        assert_eq!(rows[0].day_text, "2024-01-15");
        assert_eq!(rows[0].reward, "a brass ring");
    }
}
