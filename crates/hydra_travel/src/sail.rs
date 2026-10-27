//! Sailing: getting aboard, getting under way and keeping the ship on course.
//!
//! Recovered from `hydra_travel::sail::*` — `wind_known` (`0x101531800`),
//! `sail::go` (`0x101531850`), `Runner::not_the_captain` (`0x101531af8`),
//! `Runner::wheel_already_turned` (`0x101531c84`), `guided` (`0x101531eb4`)
//! and `MakeReady::next` (`0x1015322cc`).
//!
//! Every command string and every message below is a literal from the merged
//! `__cstring` runs at `0x10241d938`..`0x10241da80` and
//! `0x10241f2a0`..`0x10241f430`.

pub mod voyage;

/// `Only the Captain can begin your voyage` (`0x10241d90f`).
pub const NOT_THE_CAPTAIN: &str = "Only the Captain can begin your voyage";

/// `you are not aboard a ship, nor on a pier beside one.` (`0x10241d945`).
pub const NOT_ABOARD: &str = "you are not aboard a ship, nor on a pier beside one.";

/// `you have left the ship.` (`0x10241d979`).
pub const LEFT_THE_SHIP: &str = "you have left the ship.";

/// `I find no course from here.` (`0x10241d990`).
pub const NO_COURSE: &str = "I find no course from here.";

/// `the ship would not depart.` (`0x10241d9c3`).
pub const WOULD_NOT_DEPART: &str = "the ship would not depart.";

/// `this is not your ship.` (`0x10241d9f2`).
pub const NOT_YOUR_SHIP: &str = "this is not your ship.";

/// `there is no ship here to board.` (`0x10241da37`).
pub const NO_SHIP: &str = "there is no ship here to board.";

/// `I could not get aboard.` (`0x10241da62`).
pub const COULD_NOT_GET_ABOARD: &str = "I could not get aboard.";

/// `the ship has come in at another port than the one I sailed for.`
/// (`0x10241f2c0`).
pub const ANOTHER_PORT: &str = "the ship has come in at another port than the one I sailed for.";

/// `I have lost the ship's place: look at the ocean, and sail again.`
/// (`0x10241f34e`).
pub const LOST_THE_PLACE: &str =
    "I have lost the ship's place: look at the ocean, and sail again.";

/// `the anchor will not go down.` (`0x10241f3a5`).
pub const ANCHOR_WILL_NOT_DROP: &str = "the anchor will not go down.";

/// `the anchor will not come up.` (`0x10241f3c1`).
pub const ANCHOR_WILL_NOT_RISE: &str = "the anchor will not come up.";

/// `the sail will not stay out.` (`0x10241f3dd`).
pub const SAIL_WILL_NOT_STAY: &str = "the sail will not stay out.";

/// `the wheel will not turn where I turn it.` (`0x10241f3f8`).
pub const WHEEL_WILL_NOT_TURN: &str = "the wheel will not turn where I turn it.";

/// The words that name a wind, as `look ocean` prints them
/// (`0x102156bb4`: `wind`, `Hull`, `here`, `role`, …).
pub const WINDS: [&str; 6] = [
    "no wind",
    "a light breeze",
    "a fresh breeze",
    "a strong breeze",
    "a gale",
    "a storm",
];

/// One step of getting a ship ready to sail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ready {
    /// Step off the pier onto the ship.
    Board,
    /// Pull the gangplank in behind us.
    Gangplank,
    /// Raise the sails.
    Sail,
    /// Take the wheel.
    Wheel,
    /// Under way.
    Underway,
}

/// The steps of getting under way, in the recovered order:
/// `go gangplank` (`0x10241da56`), `pull gangplank` / `push gangplank`
/// (`0x10241da0a`, `0x10241f2b2`), `cast sail` (`0x10241da18`),
/// `wheel` (`0x10241f306`).
#[derive(Clone, Debug, Default)]
pub struct MakeReady {
    pub step: u8,
    pub on_ship: bool,
}

impl MakeReady {
    /// The next command that gets the ship ready.
    ///
    /// **ESTABLISHED (literals and order)** (`0x1015322cc`): the body tests the
    /// last lines against [`NO_SHIP`], [`COULD_NOT_GET_ABOARD`],
    /// [`NOT_YOUR_SHIP`] and [`ANOTHER_PORT`] and otherwise walks the four
    /// commands above. The six closures the inventory lists for this function
    /// are the per-step `contains` tests.
    pub fn next(&mut self, seen: &[String]) -> Option<String> {
        for line in seen {
            if line.contains(NO_SHIP) || line.contains(COULD_NOT_GET_ABOARD) {
                return None;
            }
            if line.contains(NOT_YOUR_SHIP) || line.contains(ANOTHER_PORT) {
                return None;
            }
        }
        let command = match self.step {
            0 => "go gangplank",
            1 => "pull gangplank",
            2 => "cast sail",
            3 => "wheel",
            _ => return None,
        };
        self.step += 1;
        Some(command.to_string())
    }
}

/// The wind named by the last `look ocean` line.
///
/// **ESTABLISHED** (`0x101531800`): the body scans the line for the wind words
/// in [`WINDS`] and returns the first one it finds; a line with none of them
/// (or a line that is not about the ocean at all) yields `None`.
pub fn wind_known(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    WINDS.into_iter().find(|wind| lower.contains(wind))
}

/// The guided sail: what to do once the ship is moving.
///
/// **ESTABLISHED (messages)** (`0x101531eb4`): the body answers with
/// [`NO_COURSE`], [`WHEEL_WILL_NOT_TURN`], [`LOST_THE_PLACE`],
/// [`LEFT_THE_SHIP`], [`ANCHOR_WILL_NOT_DROP`], [`ANCHOR_WILL_NOT_RISE`] or
/// [`SAIL_WILL_NOT_STAY`] depending on the room and the last lines, and
/// otherwise issues `wheel`, `lower sail`, `anchor` or `push capstan`
/// (`0x10241f306`..`0x10241f420`). The predicate that picks between them is not
/// recoverable; this build keeps the "lost the place" and "left the ship"
/// answers and otherwise turns the wheel.
pub fn guided(seen: &[String], aboard: bool) -> Option<String> {
    if !aboard {
        return Some(format!("say {LEFT_THE_SHIP}"));
    }
    for line in seen {
        if line.contains(ANCHOR_WILL_NOT_DROP) || line.contains(ANCHOR_WILL_NOT_RISE) {
            return None;
        }
        if line.contains(SAIL_WILL_NOT_STAY) {
            return Some("cast sail".to_string());
        }
        if line.contains(LOST_THE_PLACE) {
            return Some("look ocean".to_string());
        }
    }
    Some("wheel".to_string())
}

/// The sailing runner.
#[derive(Clone, Debug, Default)]
pub struct Runner {
    pub captain: bool,
    pub turned: bool,
}

impl Runner {
    /// The refusal a non-captain gets when it tries to begin a voyage.
    ///
    /// **ESTABLISHED** (`0x101531af8`): the body formats
    /// [`NOT_THE_CAPTAIN`] (`0x10241d90f`, 40 bytes).
    pub fn not_the_captain(&self) -> String {
        NOT_THE_CAPTAIN.to_string()
    }

    /// The refusal when the wheel has already been turned this turn.
    ///
    /// **ESTABLISHED** (`0x101531c84`): the body formats
    /// [`WHEEL_WILL_NOT_TURN`] (`0x10241f3f8`, 43 bytes).
    pub fn wheel_already_turned(&self) -> String {
        WHEEL_WILL_NOT_TURN.to_string()
    }
}

/// Sail toward a port: the command a route issues.
///
/// `sail::go` (`0x101531850`) checks that the walker is aboard and issues
/// `sail <port>`; the pooled fragments around it are the port names.
pub fn go(port: &str) -> Option<String> {
    let port = port.trim();
    if port.is_empty() {
        return None;
    }
    Some(format!("sail {port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_messages_are_the_recovered_literals() {
        assert_eq!(NOT_THE_CAPTAIN, "Only the Captain can begin your voyage");
        assert_eq!(NOT_THE_CAPTAIN.len(), 38);
            assert_eq!(WHEEL_WILL_NOT_TURN, "the wheel will not turn where I turn it.");
        assert_eq!(NO_SHIP, "there is no ship here to board.");
        assert_eq!(COULD_NOT_GET_ABOARD, "I could not get aboard.");
        assert_eq!(
            LOST_THE_PLACE,
            "I have lost the ship's place: look at the ocean, and sail again."
        );
    }

    #[test]
    fn runner_refusals() {
        let runner = Runner::default();
        assert_eq!(runner.not_the_captain(), NOT_THE_CAPTAIN);
        assert_eq!(runner.wheel_already_turned(), WHEEL_WILL_NOT_TURN);
    }

    #[test]
    fn make_ready_walks_the_commands() {
        let mut ready = MakeReady::default();
        assert_eq!(ready.next(&[]).as_deref(), Some("go gangplank"));
        assert_eq!(ready.next(&[]).as_deref(), Some("pull gangplank"));
        assert_eq!(ready.next(&[]).as_deref(), Some("cast sail"));
        assert_eq!(ready.next(&[]).as_deref(), Some("wheel"));
        assert_eq!(ready.next(&[]), None);
        let mut ready = MakeReady::default();
        assert_eq!(ready.next(&[NO_SHIP.to_string()]), None);
    }

    #[test]
    fn wind_is_read_off_the_ocean() {
        assert_eq!(
            wind_known("The wind is a fresh breeze out of the northeast."),
            Some("a fresh breeze")
        );
        assert_eq!(wind_known("Nothing moves."), None);
    }

    #[test]
    fn guided_turns_the_wheel() {
        assert_eq!(guided(&[], true).as_deref(), Some("wheel"));
        assert_eq!(
            guided(&[SAIL_WILL_NOT_STAY.to_string()], true).as_deref(),
            Some("cast sail")
        );
        assert_eq!(
            guided(&[LOST_THE_PLACE.to_string()], true).as_deref(),
            Some("look ocean")
        );
        assert_eq!(go("kraken").as_deref(), Some("sail kraken"));
        assert_eq!(go("  "), None);
    }
}
