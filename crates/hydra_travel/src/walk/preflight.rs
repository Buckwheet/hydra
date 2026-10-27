//! What a walk needs before it starts: silver, a shop to leave, gigas
//! fragments.
//!
//! Recovered from `hydra_travel::walk::preflight::{silver_for, gigas_least,
//! out_of_a_shop, withdraw_command, gigas_way}` (`0x101528238`, `0x1015285cc`,
//! `0x101528788`, `0x101528bcc`, `0x101528c70`).

/// The silver a route charges, with the bank trip when the walker is short.
///
/// **ESTABLISHED (literals and shape)** (`0x101528238`): the body formats the
/// piece table at `0x103e6c0f8`..`0x103e6c110` — `" banker for "` (12) and
/// `" silvers"` (8) — so the message names the amount from the banker, and it
/// reads the `get_silvers` and `get_return_trip_silvers` settings (both are in
/// the schema at `0x10241c6f5`..`0x10241c765`).
pub fn silver_for(charged: u32, held: u32, get_silvers: bool, return_trip: bool) -> Sought {
    let wanted = if return_trip { charged.saturating_mul(2) } else { charged };
    if held >= wanted || !get_silvers {
        return Sought {
            wanted,
            withdraw: None,
        };
    }
    Sought {
        wanted,
        withdraw: Some(withdraw_command(wanted - held)),
    }
}

/// What [`silver_for`] decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sought {
    /// How much silver the route needs.
    pub wanted: u32,
    /// The bank command to get it, when the bank has to be visited.
    pub withdraw: Option<String>,
}

/// The fewest gigas fragments a crossing may be made with
/// (`gigas_least`, `0x1015285cc`).
///
/// **ESTABLISHED (literals)**: the setting key `gigas_min_number` and its
/// description (`0x102409b60`) give the default of **4** — `Take the fragment
/// crossing only with at least this many.` The function reads the setting and
/// clamps at zero.
pub fn gigas_least(set: Option<u32>) -> u32 {
    set.unwrap_or(4)
}

/// How the walker crosses to the Hinterwilds (`gigas_way`, `0x101528c70`).
///
/// **Partly recovered**: the body checks the fragment count against
/// [`gigas_least`] and picks the caravan or the fragments; the recovered words
/// around it are `gigas_min_number`, `prestidigitorium`, `elaienaran_haven` and
/// `Hinterwilds side` (`0x102409b60`).
pub fn gigas_way(fragments: u32, least: u32, side: Option<&str>) -> Option<&'static str> {
    if fragments < least {
        return None;
    }
    match side {
        Some("Icemule Trace") => Some("go caravan"),
        Some("Elven Nations") => Some("go caravan"),
        _ => Some("go gate"),
    }
}

/// The bank command that gets `amount` silver (`withdraw_command`, 164 bytes at
/// `0x101528bcc`).
///
/// The recovered fragment is the `withdraw ` verb of the day-pass clerk's
/// `withdraw 5000` (`0x10241becb`).
pub fn withdraw_command(amount: u32) -> String {
    format!("withdraw {amount}")
}

/// Is the walker stuck in a shop (`out_of_a_shop`, 1092 bytes at `0x101528788`)?
///
/// **Partly recovered**: the body matches the shop names at `0x10241c59e`
/// (`Outfitting`, `Magic Shoppe`, `General Store`, `Armory`, `Combat Gear`,
/// `Lockpicks`, `Pinefar, Depository`) against the room title and answers with
/// `out` when one matches.
pub fn out_of_a_shop(room: &str) -> Option<&'static str> {
    const SHOPS: [&str; 7] = [
        "Outfitting",
        "Magic Shoppe",
        "General Store",
        "Armory",
        "Combat Gear",
        "Lockpicks",
        "Pinefar, Depository",
    ];
    if SHOPS.iter().any(|shop| room.contains(shop)) {
        return Some("out");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_walkers_visit_the_bank() {
        let sought = silver_for(500, 100, true, false);
        assert_eq!(sought.wanted, 500);
        assert_eq!(sought.withdraw.as_deref(), Some("withdraw 400"));
        let sought = silver_for(500, 5000, true, false);
        assert_eq!(sought.withdraw, None);
        let sought = silver_for(500, 0, false, false);
        assert_eq!(sought.withdraw, None);
        assert_eq!(silver_for(500, 0, true, true).wanted, 1000);
    }

    #[test]
    fn gigas_default_is_four() {
        assert_eq!(gigas_least(None), 4);
        assert_eq!(gigas_least(Some(9)), 9);
        assert_eq!(gigas_way(3, 4, None), None);
        assert_eq!(gigas_way(4, 4, Some("Icemule Trace")), Some("go caravan"));
        assert_eq!(gigas_way(4, 4, None), Some("go gate"));
    }

    #[test]
    fn shops_are_left() {
        assert_eq!(out_of_a_shop("Pinefar, Depository"), Some("out"));
        assert_eq!(out_of_a_shop("Wehnimer's Landing, Town Square"), None);
    }
}
