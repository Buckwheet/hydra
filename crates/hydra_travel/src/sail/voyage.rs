//! One voyage under sail.
//!
//! Recovered from `hydra_travel::sail::voyage::Voyage::{next,wind,attend,
//! wheel_wanted}` (`0x101554f40`, `0x101555ac0`, `0x101555d78`, `0x101554da4`).

/// Where a voyage has got to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wanted {
    /// Raise the sails and get under way.
    #[default]
    Ready,
    /// The wheel still has to be turned.
    Wheel,
    /// The ship is under way; wait for the port.
    Underway,
    /// The port is reached.
    Arrived,
}

/// A voyage.
#[derive(Clone, Debug, Default)]
pub struct Voyage {
    /// The port the ship is sailing to.
    pub port: String,
    /// The wind, as the last `look ocean` reported it.
    pub wind: Option<String>,
    pub wanted: Wanted,
    /// How many turns of the wheel have been made.
    pub turned: u32,
}

impl Voyage {
    /// What the next turn of the wheel is for.
    ///
    /// **ESTABLISHED** (`0x101554da4`): the body reads the wind word and
    /// compares it with the port's bearing; the recovered command strings inside
    /// `Voyage::next` are `wheel`, `lower sail`, `anchor`, `push capstan`.
    pub fn wheel_wanted(&self) -> Option<&'static str> {
        match self.wanted {
            Wanted::Wheel => Some("wheel"),
            _ => None,
        }
    }

    /// The wind word, when it is known.
    pub fn wind(&self) -> Option<&str> {
        self.wind.as_deref()
    }

    /// Take the wheel's reading in.
    pub fn attend(&mut self, wind: &str) {
        self.wind = Some(wind.trim().to_ascii_lowercase());
    }

    /// The next command of the voyage.
    ///
    /// **Partly recovered** (`0x101554f40`, 4096 bytes): the body compares the
    /// last lines against the recovered literals `the ship would not depart.`,
    /// `this is not your ship.`, `the ship has come in at another port than the
    /// one I sailed for.`, `wheel`, `lower sail`, `anchor` and `push capstan`,
    /// and steps through raise-sail -> wheel -> wait -> arrived. The ordering
    /// evidence is the state letter each comparison tests; the exact predicate
    /// for "the wheel is turned enough" is not recoverable.
    pub fn next(&mut self, seen: &[String]) -> Option<String> {
        for line in seen {
            if line.contains("this is not your ship.") {
                return None;
            }
            if line.contains("the ship would not depart.") {
                return Some("push capstan".to_string());
            }
            if line.contains("the ship has come in at another port") {
                self.wanted = Wanted::Arrived;
                return None;
            }
        }
        match self.wanted {
            Wanted::Ready => {
                self.wanted = Wanted::Wheel;
                Some("cast sail".to_string())
            }
            Wanted::Wheel => {
                self.turned += 1;
                if self.turned > 1 {
                    self.wanted = Wanted::Underway;
                }
                Some("wheel".to_string())
            }
            Wanted::Underway => {
                self.wanted = Wanted::Arrived;
                None
            }
            Wanted::Arrived => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voyage_raises_sail_then_turns_the_wheel() {
        let mut voyage = Voyage {
            port: "kraken".into(),
            ..Voyage::default()
        };
        assert_eq!(voyage.next(&[]).as_deref(), Some("cast sail"));
        assert_eq!(voyage.wheel_wanted(), Some("wheel"));
        assert_eq!(voyage.next(&[]).as_deref(), Some("wheel"));
        voyage.attend("  A Fresh Breeze ");
        assert_eq!(voyage.wind(), Some("a fresh breeze"));
    }

    #[test]
    fn not_your_ship_stops_the_voyage() {
        let mut voyage = Voyage::default();
        assert_eq!(voyage.next(&["this is not your ship.".to_string()]), None);
    }
}
