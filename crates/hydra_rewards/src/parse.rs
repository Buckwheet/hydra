//! The two line grammars of the daily login reward.
//!
//! Recovered from `hydra_rewards::parse::{reward, number_before}`
//! (`0x1014ee138`, `0x1014edc00`).

/// The prefix every reward line starts with, compared as five 8-byte chunks
/// in the emitted code (`0x1014ee168` … `0x1014ee1c8`); it is 38 bytes long,
/// which is also the `subs x20, x1, #0x26` length gate.
pub const PREFIX: &str = "You have earned the following reward: ";

/// What the game announced, with the announcement stripped off.
///
/// `parse::reward(line: &str) -> Option<String>` — `@0x1014ee138` (**full**).
///
/// ```text
/// reward("You have earned the following reward: a silvery potion!")
///     == Some("a silvery potion!")
/// reward("You have earned the following reward:  ")  == None
/// reward("You wave.")                                == None
/// ```
///
/// The line is trimmed (`core::str::trim_matches` at `0x1014ee150`), the
/// 38-byte prefix is compared, the remainder is trimmed again
/// (`0x1014ee2b8`) and an empty remainder answers `None`.  The answer is
/// copied into a fresh `String` (the `__rust_alloc` at `0x1014ee2ec`).
///
/// **Conflict** — the emitted body also contains a scan for `'!'`
/// (`0x1014ee200` … `0x1014ee2a4`) that would cut the text at the last bang
/// (`&rest[..idx]` when one is found, the whole remainder otherwise), but
/// every entry into the cut is guarded by `b.hs`/`b.ls` on `idx < len`, so the
/// loop can only leave through its "not found" edge and the cut branch is
/// dead: the *executed* behaviour keeps the whole remainder.  This
/// reconstruction keeps the whole remainder and does not cut.  See
/// `NOTES.md`.
pub fn reward(line: &str) -> Option<String> {
    let line = line.trim();
    let rest = line.strip_prefix(PREFIX)?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    Some(rest.to_string())
}

/// The number written immediately before `phrase`.
///
/// `parse::number_before(haystack: &str, phrase: &str) -> Option<u32>` —
/// `@0x1014edc00` (**partly**: the search and the digit fold are recovered,
/// the exact choice between the first and the last occurrence of `phrase` is
/// not).  The emitted body runs `core::str::pattern::StrSearcher` over the
/// haystack (the `StrSearcher::new` at `0x1014edc1c`), takes the text in
/// front of the match (`&haystack[..idx]`), skips trailing whitespace and
/// folds the digits into an integer with an overflow check
/// (`smulh`/`adds … b.vs`, `0x1014ee04c`).
///
/// ```text
/// number_before("You have logged in 12 consecutive days", " consecutive day")
///     == Some(12)
/// ```
pub fn number_before(haystack: &str, phrase: &str) -> Option<u32> {
    let at = haystack.rfind(phrase)?;
    let front = haystack[..at].trim_end();
    let digits: String = front
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

/// The phrase that follows the day count of the streak line
/// (`0x10241b73d`, 16 bytes, passed with `w3 = 0x10`).
pub const CONSECUTIVE_DAY: &str = " consecutive day";
/// The phrase that follows the bridge count of the streak line
/// (`0x10241b74d`, 13 bytes, passed with `w3 = 0xd`).
pub const LOGIN_BRIDGE: &str = " login bridge";
/// The head of the streak line (`0x10241b722`, 27 bytes, `x25 = len - 0x1b`).
pub const THANKS: &str = "Thank you for logging into ";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reward_line_yields_the_reward() {
        assert_eq!(
            reward("You have earned the following reward: a silvery potion!").as_deref(),
            Some("a silvery potion!")
        );
    }

    #[test]
    fn the_line_is_trimmed_and_the_reward_is_trimmed() {
        assert_eq!(
            reward("  You have earned the following reward:   a brass ring  \n").as_deref(),
            Some("a brass ring")
        );
    }

    #[test]
    fn anything_else_is_none() {
        assert_eq!(reward(""), None);
        assert_eq!(reward("You wave."), None);
        assert_eq!(reward("You have earned the following reward: "), None);
        assert_eq!(reward("You have earned the following reward:    "), None);
        // one byte short of the prefix
        assert_eq!(reward("You have earned the following reward:"), None);
        // the prefix is matched case-sensitively
        assert_eq!(reward("you have earned the following reward: a ring"), None);
    }

    #[test]
    fn a_streak_line_yields_its_two_numbers() {
        let line = "Thank you for logging into GemStone IV!  You have logged in \
                    12 consecutive days in a row and crossed 3 login bridges.";
        let rest = line.strip_prefix(THANKS).unwrap();
        assert_eq!(number_before(rest, CONSECUTIVE_DAY), Some(12));
        assert_eq!(number_before(rest, LOGIN_BRIDGE), Some(3));
    }

    #[test]
    fn number_before_needs_a_number_and_a_phrase() {
        assert_eq!(
            number_before("no digits here consecutive day", CONSECUTIVE_DAY),
            None
        );
        assert_eq!(number_before("7 consecutive day", CONSECUTIVE_DAY), Some(7));
        assert_eq!(number_before("7 login bridge", " days gone"), None);
        // the day count is a plain ASCII run; a longer one still parses
        assert_eq!(
            number_before("logged in 1234 consecutive days", CONSECUTIVE_DAY),
            Some(1234)
        );
    }

    #[test]
    fn the_prefixes_are_the_recovered_lengths() {
        assert_eq!(PREFIX.len(), 38);
        assert_eq!(THANKS.len(), 27);
        assert_eq!(CONSECUTIVE_DAY.len(), 16);
        assert_eq!(LOGIN_BRIDGE.len(), 13);
    }
}
