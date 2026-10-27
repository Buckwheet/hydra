//! The two small row types the rest of the crate passes around: *who* a
//! hearing belongs to, and *when* it happened.
//!
//! The original's rows live in `hydra_ledger::row` (`Who`, `Stamp`,
//! `Reward`), which is not part of this reconstruction, so the shapes are
//! declared here at the offsets the recovered code reads them at.  The
//! heap-owning row itself is [`crate::watcher::Heard`].

/// A character, as the login-reward paths identify one.
///
/// **ESTABLISHED**: two `String`s, 0x30 bytes, `instance` at `+0x00` and
/// `character` at `+0x18`:
///
/// * `watcher::Heard::at` (`0x1014efe50`) clones `[who+0x00]` into the row's
///   `+0x20` and `[who+0x18]` into the row's `+0x38`
///   (`<String as Clone>::clone` at `0x1014efe58` and `0x1014efe64`);
/// * `report::own` (`0x1014ce880`) formats `who+0x18` as the character name
///   in `{}: today's login reward (…`;
/// * `schedule::Character::skipped` (`0x1014c9ccc`) formats
///   `format!("{}:{}", who+0x00, who+0x18)` (the `"{}:{}"` blob at
///   `0x103e21670`) and compares it with the skip entries.
///
/// The instance is the game code the session is on (`hydra_net::gemstone::
/// endpoint::instance` answers the same string), which is why the settings
/// help says "by name or as GAME:Name".
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Who {
    /// The game's instance code, e.g. `GS4`.
    pub instance: String,
    /// The character's name.
    pub character: String,
}

impl Who {
    pub fn new(instance: impl Into<String>, character: impl Into<String>) -> Who {
        Who {
            instance: instance.into(),
            character: character.into(),
        }
    }

    /// `GAME:Name`, as the settings' `skip` list spells it.
    pub fn tag(&self) -> String {
        format!("{}:{}", self.instance, self.character)
    }
}

/// When something was seen: the wall clock and the game's own clock.
///
/// **ESTABLISHED**: 16 bytes, copied verbatim into the row
/// (`watcher::Heard::at`, `0x1014efed8`: `ldr q1, [x21]` / `str q1, [x20,
/// #0xb0]`), and field `+0x08` is the unix second that
/// `hydra_net::eastern::date` is called on (`0x1014efe88`) — the game clock
/// the day and the `hh:mm` in the reports come from.
///
/// **UNKNOWN**: what `+0x00` holds; the original's `hydra_ledger::row::Stamp`
/// is not in the binary's symbol table beyond `Stamp::now` (`0x1016995a4`).
/// This reconstruction keeps `at` (when Hydra saw it, unix seconds) at
/// `+0x00` and `game_time` (the game clock, unix seconds) at `+0x08`, the
/// only reading that fits the two field reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stamp {
    /// When Hydra saw the line, unix seconds.
    pub at: i64,
    /// The game's clock at that moment, unix seconds.
    pub game_time: i64,
}

impl Stamp {
    pub const fn new(at: i64, game_time: i64) -> Stamp {
        Stamp { at, game_time }
    }

    /// Now, for both clocks (the original asked the wall clock once and used
    /// it for both — `hydra_ledger::row::Stamp::now`).
    pub fn now() -> Stamp {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Stamp { at, game_time: at }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_is_game_colon_name() {
        assert_eq!(Who::new("GS4", "Aelith").tag(), "GS4:Aelith");
    }

    #[test]
    fn stamp_now_is_a_unix_second() {
        let s = Stamp::now();
        assert_eq!(s.at, s.game_time);
        // 2020-01-01 .. 2100-01-01
        assert!(s.at > 1_577_836_800 && s.at < 4_102_444_800, "{s:?}");
    }
}
