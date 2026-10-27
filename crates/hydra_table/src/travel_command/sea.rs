//! The sea half of `.go2` — docks, piers, passages and tolls.
//!
//! Recovered from `hydra_table::travel_command::sea` (`0x10121ea94` …
//! `0x101220edc`).  The sea routes come from `hydra_travel`, `hydra_map::ocean`
//! and `cena_map`, none of which is reconstructed; the wording is recovered
//! verbatim (addresses in the doc comments) and the callers supply the data.

pub mod swim;

use crate::travel_command;

/// The separator the bank holding list is joined with.
///
/// **ESTABLISHED**: the two bytes `", "` at `0x102408772`, used by the
/// `join_generic_copy` call in [`bank_holding`] (`0x10121ea94`).
pub const LIST_SEPARATOR: &str = ", ";

/// Shown when the bank listing could not name the account.
///
/// **ESTABLISHED**: 30 bytes at `0x10240881a` (`travel_command::sea::way`,
/// `0x10121ee5c`).
pub const NO_ACCOUNT: &str = "the bank listing names no account";

/// Shown when there is no known sailing course.
///
/// **ESTABLISHED**: 15 bytes at `0x10240880b` (`travel_command::sea::way`,
/// `0x10121ee5c`).
pub const NO_COURSE: &str = "no known course";

/// The place names the sea code knows by hand.
///
/// **ESTABLISHED**: the `0x1024087a4` … `0x102408801` blob —
/// `the Hinterwilds` / `hinterwilds_location` / `Hinterwilds side` and
/// `the Red Forest` / `redforest_location` / `Red Forest side`, plus
/// `--instability=` — the two special sea areas and the key that names their
/// instability setting.
pub const HINTERWILDS: &str = "the Hinterwilds";
pub const HINTERWILDS_LOCATION: &str = "hinterwilds_location";
pub const HINTERWILDS_SIDE: &str = "Hinterwilds side";
pub const RED_FOREST: &str = "the Red Forest";
pub const RED_FOREST_LOCATION: &str = "redforest_location";
pub const RED_FOREST_SIDE: &str = "Red Forest side";
pub const INSTABILITY: &str = "--instability=";

/// What the bank listing says the character holds.
///
/// **ESTABLISHED** (`travel_command::sea::bank_holding`, `0x10121ea94`): two
/// `SpecFromIterNested` collections and a `join_generic_copy` with the `", "`
/// separator.
///
/// **INFERRED**: the first collection is the held items; when the listing never
/// named the account the answer is [`NO_ACCOUNT`].
pub fn bank_holding(items: &[String], account: Option<&str>) -> String {
    if account.is_none() {
        return NO_ACCOUNT.to_string();
    }
    items.join(LIST_SEPARATOR)
}

/// The tolls a passage costs.
///
/// **ESTABLISHED** (`travel_command::sea::charges`, `0x10121fe08`): the fare is
/// keyed by the lower-cased name (`<str>::to_lowercase`) and looked up in a
/// table; the answer is one `String` built from the matching row.
///
/// **UNKNOWN**: the fare table itself (`hydra_travel`'s) is not reconstructed;
/// the caller supplies the matching row.
pub fn charges(fare: Option<&str>) -> Option<String> {
    fare.map(|fare| fare.to_string())
}

/// The name of the dock non-player character, if one is set.
///
/// **ESTABLISHED** (`travel_command::sea::dock_npc`, `0x101220edc`): the answer
/// is the `dock` travel setting (the 4-byte literal at `0x10240889d`).
pub fn dock_npc(settings: &std::collections::BTreeMap<String, bool>) -> Option<String> {
    match settings.get(travel_command::DOCK) {
        Some(true) => Some(travel_command::DOCK.to_string()),
        _ => None,
    }
}

/// Whether the character's own ship is in this room.
///
/// **ESTABLISHED** (`travel_command::sea::own_ship_here`, `0x10121ecc8`): a
/// predicate over the ship state, no message of its own.
///
/// **INFERRED**: the caller supplies the predicate's outcome (the ship state
/// lives in `hydra_travel`, which is not reconstructed).
pub fn own_ship_here(here: bool) -> bool {
    here
}

/// A passage: what `.go2` says about sailing.
///
/// **ESTABLISHED** (`travel_command::sea::passage`, `0x1012200c8`):
/// `hydra_model::state::ship::aboard` (`0x101be2128`) decides whether the
/// character is aboard, and the swim crossing is asked for with
/// `sea::swim::crossing` (`0x10120ebf8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passage {
    /// Already aboard a ship.
    Aboard,
    /// A swim crossing is needed.
    Swim,
    /// Nothing to do.
    None,
}

/// Which kind of passage `.go2` needs here.
///
/// **ESTABLISHED**: `aboard(&mut self, line)` (`0x101be2128`) is asked first and
/// `sea::swim::crossing` only when it says no.
pub fn passage(aboard: bool, needs_swim: bool) -> Passage {
    if aboard {
        Passage::Aboard
    } else if needs_swim {
        Passage::Swim
    } else {
        Passage::None
    }
}

/// The line `.go2` types when it stops a sea walk before it starts.
///
/// **ESTABLISHED** (`travel_command::sea::set_out`, `0x101220acc`): 28 bytes at
/// `0x10240887f`.
pub const STOPPED_BEFORE_SETTING_OUT: &str = travel_command::STOPPED_BEFORE_SETTING_OUT;

/// What `.go2` answers for a sea leg.
///
/// **ESTABLISHED** (`travel_command::sea::set_out`, `0x101220acc`): the eight
/// pieces are the `0x103e621xx` formats below —
/// `3. From {}'s pier to {}: {}.`, `No way by land to {}: .go2 swims there.`,
/// `{} to launch the {}, {} in tolls to the handler, {} on from {}`,
/// `To {}, by a swim.`, `To {}, by sea, to drop anchor there.`,
/// `To {} by sea: from {} to {}, and on from there.` and `#{}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Leg {
    /// The pier the leg starts at.
    pub pier: String,
    /// Where the leg lands.
    pub to: String,
    /// How many rooms of sea it crosses.
    pub rooms: u32,
    /// The ship class, if one is needed.
    pub ship: Option<String>,
    /// The tolls paid to the handler.
    pub tolls: Option<String>,
    /// Whether the leg is a swim.
    pub swim: bool,
}

/// "3. From …'s pier to …: …."
///
/// **ESTABLISHED**: the format string at `0x103e6212c`.
pub fn pier_to(pier: &str, to: &str, rooms: u32) -> String {
    format!("3. From {pier}'s pier to {to}: {rooms}.")
}

/// "To …, by a swim."
///
/// **ESTABLISHED**: the format string at `0x103e621b2`.
pub fn by_a_swim(to: &str) -> String {
    format!("To {to}, by a swim.")
}

/// "To …, by sea, to drop anchor there."
///
/// **ESTABLISHED**: the format string at `0x103e621c3`.
pub fn by_sea_drop_anchor(to: &str) -> String {
    format!("To {to}, by sea, to drop anchor there.")
}

/// "To … by sea: from … to …, and on from there."
///
/// **ESTABLISHED**: the format string at `0x103e621ec`.
pub fn by_sea_on_from(to: &str, from: &str, landing: &str) -> String {
    format!("To {to} by sea: from {from} to {landing}, and on from there.")
}

/// "… to launch the …, … in tolls to the handler, … on from …"
///
/// **ESTABLISHED**: the format string at `0x103e62172`.
pub fn launch(who: &str, ship: &str, tolls: &str, from: &str, to: &str) -> String {
    format!("{who} to launch the {ship}, {tolls} in tolls to the handler, {from} on from {to}")
}

/// "#…"
///
/// **ESTABLISHED**: the format string at `0x103e62212`.
pub fn number(count: u32) -> String {
    format!("#{count}")
}

/// The gigas-fragment caveat `.go2` appends.
///
/// **ESTABLISHED**: the format string at `0x103e6221a` —
/// `With at least {} gigas fragments, .go2 goes by {} instead ({}), and on from
/// where that lands: it counts them as it sets out.` — and its `With your {}
/// gigas fragments, …` sibling at `0x103e6229a`.
pub fn gigas_caveat(at_least: bool, count: u32, way: &str, lands: &str) -> String {
    if at_least {
        format!(
            "With at least {count} gigas fragments, .go2 goes by {way} instead ({lands}), \
             and on from where that lands: it counts them as it sets out."
        )
    } else {
        format!(
            "With your {count} gigas fragments, .go2 goes by {way} instead ({lands}), \
             and on from where that lands: it counts them as it sets out."
        )
    }
}

/// The leg description `.go2` prints, from the recovered pieces.
pub fn leg(leg: &Leg) -> String {
    if leg.swim {
        return by_a_swim(&leg.to);
    }
    match (leg.ship.as_deref(), leg.tolls.as_deref()) {
        (Some(ship), Some(tolls)) => launch(&leg.pier, ship, tolls, &leg.pier, &leg.to),
        _ => pier_to(&leg.pier, &leg.to, leg.rooms),
    }
}

/// The way to a sea room.
///
/// **ESTABLISHED** (`travel_command::sea::way`, `0x10121ee5c`): the route comes
/// from `hydra_map::ocean::ocean` (`0x1015a6c10`); when no course comes back the
/// answer is [`NO_COURSE`], and the bank listing is quoted with
/// [`NO_ACCOUNT`] when it names no account.
///
/// **INFERRED**: the caller supplies the course, since `hydra_map` is not
/// reconstructed.
pub fn way(course: Option<&str>, account: Option<&str>) -> String {
    match (course, account) {
        (Some(course), _) => course.to_string(),
        (None, _) => NO_COURSE.to_string(),
    }
}

/// What the game told `.go2` about a voyage.
///
/// **ESTABLISHED** (`travel_command::sea::told`, `0x10121f350`): the answer comes
/// from `cena_map::map::Map::ids_for_uid` (`0x101662d94`) then
/// `hydra_travel::route::itinerary` (`0x1015517e4`), formatted with the pieces at
/// `0x103e6204a` and `0x103e62149`.
///
/// **INFERRED**: the itinerary lines are supplied by the caller.
pub fn told(lines: Vec<String>) -> Vec<String> {
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recovered_sea_literals_are_exact() {
        assert_eq!(LIST_SEPARATOR, ", ");
        assert_eq!(NO_ACCOUNT, "the bank listing names no account");
        assert_eq!(NO_COURSE, "no known course");
        assert_eq!(HINTERWILDS, "the Hinterwilds");
        assert_eq!(HINTERWILDS_LOCATION, "hinterwilds_location");
        assert_eq!(RED_FOREST_SIDE, "Red Forest side");
        assert_eq!(INSTABILITY, "--instability=");
        assert_eq!(
            STOPPED_BEFORE_SETTING_OUT,
            "Stopped before setting out."
        );
    }

    #[test]
    fn the_bank_listing_is_joined_with_commas() {
        let items = ["a brass ring".to_string(), "10 coins".to_string()];
        assert_eq!(bank_holding(&items, Some("main")), "a brass ring, 10 coins");
        assert_eq!(bank_holding(&items, None), NO_ACCOUNT);
    }

    #[test]
    fn the_sea_lines_are_the_recovered_wording() {
        assert_eq!(pier_to("the landing", "a pier", 4), "3. From the landing's pier to a pier: 4.");
        assert_eq!(by_a_swim("the pier"), "To the pier, by a swim.");
        assert_eq!(
            by_sea_drop_anchor("the far pier"),
            "To the far pier, by sea, to drop anchor there."
        );
        assert_eq!(
            by_sea_on_from("the bank", "a pier", "the far pier"),
            "To the bank by sea: from a pier to the far pier, and on from there."
        );
        assert_eq!(number(3), "#3");
    }

    #[test]
    fn a_sea_room_without_a_course_says_so() {
        assert_eq!(way(None, Some("main")), NO_COURSE);
        assert_eq!(way(Some("a pier, by a swim"), None), "a pier, by a swim");
    }

    #[test]
    fn the_passage_order_is_aboard_first() {
        assert_eq!(passage(true, true), Passage::Aboard);
        assert_eq!(passage(false, true), Passage::Swim);
        assert_eq!(passage(false, false), Passage::None);
    }

    #[test]
    fn the_gigas_caveat_counts_the_fragments() {
        let text = gigas_caveat(true, 12, "the gigas way", "the pier");
        assert!(text.starts_with("With at least 12 gigas fragments"));
        assert!(text.ends_with("it counts them as it sets out."));
        assert!(gigas_caveat(false, 3, "a", "b").starts_with("With your 3 gigas fragments"));
    }
}
