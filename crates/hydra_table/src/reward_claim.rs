//! Claiming rewards across the accounts of a team.
//!
//! Recovered from `hydra_table::reward_claim` (`0x1012924dc` … `0x1012928f0`).
//! The rewards themselves live in `hydra_rewards` (`schedule::Settings::read`,
//! `hydra_rewards::schedule::next`) which is **not** reconstructed; this module is
//! the part `hydra_table` owns: the account key, the schedule read, and which
//! accounts are already busy.

use crate::roster;
use crate::seat::{Live, Sessions};

/// The key an account is claimed under: `character:account`, lower-cased.
///
/// **ESTABLISHED** (`reward_claim::key`, `0x101292600`): `format!("{}:{}", a, b)`
/// (the two-byte piece `":"` at `0x103e63da2`) with the `String` at `+0x00` and the
/// one at `+0x18` of the argument — a [`roster::Entry`], so `character:account` —
/// then `<str>::to_lowercase`.
pub fn key(entry: &roster::Entry) -> String {
    format!("{}:{}", entry.character, entry.account).to_lowercase()
}

/// The rewards schedule, as far as this crate can see it.
///
/// **ESTABLISHED** (`reward_claim::current`, `0x1012927f0`):
/// `hydra_rewards::schedule::Settings::read` (`0x1014c857c`) answers a struct of
/// 0x28 bytes whose field at `+0x18` is a discriminant: when it is `0` the code
/// drops the settings and answers `None`, otherwise it copies them out.
///
/// **UNKNOWN**: the fields of `hydra_rewards::schedule::Settings`; `hydra_rewards`
/// is not reconstructed, so the settings are reported as "a schedule is set" and
/// nothing more.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schedule {
    /// The schedule's discriminant at `+0x18` — non-zero means a schedule is set.
    pub kind: u32,
}

/// The schedule that is set for this Hydra, if any.
///
/// **ESTABLISHED** (`0x1012927f0`): `Settings::read()` then the `+0x18`
/// discriminant check.
///
/// **Departure**: `hydra_rewards` is not part of this reconstruction, so no
/// schedule is ever reported.
pub fn current() -> Option<Schedule> {
    None
}

/// The sessions that are live right now.
///
/// **ESTABLISHED** (`reward_claim::connected`, `0x1012928f0`): `Instant::now()`
/// then `hydra_board::sessions::Sessions::glance` (`0x1013eacf0`), collecting the
/// glance into a `Vec` (`SpecFromIterNested` at `0x10124c7ac`) and dropping the
/// `Glance`.
pub fn connected(sessions: &Sessions) -> Vec<Live> {
    sessions.glance(std::time::Instant::now())
}

/// The `(account, character)` pairs already playing, in this game.
///
/// **ESTABLISHED** (`reward_claim::busy_accounts`, `0x1012924dc`): the same
/// `glance(now)` and collect as [`connected`], then a second collect that maps
/// each live session to a `(String, String)` pair
/// (`drop_in_place<Vec<(String, String)>>` at `0x101297a4c`) after the game filter
/// the caller passed (`stp x22, x21, [sp, #0x10]`).
pub fn busy_accounts(sessions: &Sessions, game_code: &str) -> Vec<(String, String)> {
    connected(sessions)
        .into_iter()
        .filter(|live| live.game_code.eq_ignore_ascii_case(game_code))
        .map(|live| (live.account, live.character))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(character: &str, account: &str) -> roster::Entry {
        roster::Entry {
            character: character.to_string(),
            account: account.to_string(),
            game_code: "GS4".to_string(),
            favourite: false,
            nickname: None,
        }
    }

    #[test]
    fn the_key_is_character_colon_account() {
        assert_eq!(key(&entry("Aelith", "Main")), "aelith:main");
    }

    #[test]
    fn only_this_games_live_sessions_are_busy() {
        let sessions = Sessions::new();
        sessions.add(Live {
            account: "Main".to_string(),
            character: "Aelith".to_string(),
            game_code: "GS4".to_string(),
        });
        sessions.add(Live {
            account: "Alt".to_string(),
            character: "Wynter".to_string(),
            game_code: "GS3".to_string(),
        });
        assert_eq!(connected(&sessions).len(), 2);
        assert_eq!(
            busy_accounts(&sessions, "gs4"),
            vec![("Main".to_string(), "Aelith".to_string())]
        );
        assert!(busy_accounts(&sessions, "DR").is_empty());
    }

    #[test]
    fn no_schedule_is_reported_without_hydra_rewards() {
        assert_eq!(current(), None);
    }
}
