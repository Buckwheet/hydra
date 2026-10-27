//! Working out where the ship is.
//!
//! `Navigator::fix` (`0x1015b3fe0`) is what turns "you are standing on a pier"
//! into a port: it reads the room the character is in, checks whether they are
//! aboard (`hydra_model::state::ship::aboard` is called from `0x1015b41b4`), and
//! asks the ocean for the port whose pier carries the room's title
//! (`Ocean::port_of_pier_titled`, called from `0x1015b452c` and `0x1015b4548`).

use crate::cena::title_from_subtitle;
use crate::follow::Standing;
use crate::ocean::{Ocean, Port};

/// The `+0x28` state byte's "no fix yet" value: `fix` compares it against `4` at
/// `0x1015b4028`.
pub const UNFIXED: u8 = 4;

/// The `+0x28` state byte once a fix has been made.
///
/// **Partly recovered.** The original writes `1`, `2` and `3` into the same byte
/// (`mov w8, #0x1`/`#0x2`/`#0x3` before `strb w8, [x21]` at `0x1015b4c98`,
/// `0x1015b4cf8`, with the byte copied into the navigator by the `ldur q0,
/// [x21, #0x28]` at `0x1015b4cb0`), so the three numerals stand for three
/// distinct outcomes of a successful fix. Which rule picks which numeral is not
/// in the instruction stream: all three arms share the same tail. This build
/// therefore uses one value — it does not pretend to tell the three apart — and
/// records `aboard` in [`Navigator::aboard`] instead, which *is* evidenced (the
/// `and w8, w8, #0x1` / `strb w8, [x21, #0x1]` pair at `0x1015b4cf0`).
pub const FIXED: u8 = 1;

/// Where the ship is: the port and the pier.
///
/// The state byte is at `+0x28`; the port and pier it settles on are what the
/// rest of the sailing code reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Navigator {
    /// `+0x28`. [`UNFIXED`] until a room has been placed, then [`FIXED`].
    pub state: u8,
    /// The port the ship is at, once known.
    pub port: Option<u64>,
    /// The pier the character is standing on, once known.
    pub pier: Option<u64>,
    /// Whether the character was aboard at the fix — the one-bit field the
    /// original keeps beside the state byte (`+0x1` of the returned record).
    pub aboard: bool,
}

impl Navigator {
    /// @0x1015b3fe0 — settle the port and pier from the room the character is
    /// in, and report whether a fix was made.
    ///
    /// **Partly recovered.** Recovered:
    ///
    /// * it needs a room name (`state.room + 0xa70` must not hold the
    ///   `isize::MIN` niche — `0x1015b4010`);
    /// * it parses the room number out of the room block (`+0xa58`/`+0xa60`/
    ///   `+0xa68`) with the same decimal reader [`crate::room::found`] uses;
    /// * it consults `hydra_model::state::ship::aboard` and resolves the port
    ///   through `Ocean::port_of_pier_titled`, then `Ocean::port_of_pier`.
    ///
    /// Not recovered: which of `1`/`2`/`3` the three successful arms write into
    /// the state byte (see [`FIXED`]). This build resolves by the pier title
    /// first, then by room number, then by port name, leaves the state byte at
    /// [`UNFIXED`] when nothing matches, and records the aboard flag separately.
    pub fn fix(&mut self, ocean: &Ocean, standing: &Standing, aboard: bool) -> bool {
        let (title, subtitle) = title_from_subtitle(&standing.name);
        if title.is_empty() {
            self.state = UNFIXED;
            return false;
        }
        let pier_title = if subtitle.is_empty() {
            title.clone()
        } else {
            format!("{title}, {subtitle}")
        };
        let number = standing.number.and_then(|n| u64::try_from(n).ok());
        let by_title = ocean.port_of_pier_titled(&pier_title);
        let by_number = number.and_then(|n| ocean.port_of_pier(n));
        let port: Option<&Port> = by_title.or(by_number).or_else(|| ocean.port_named(&title));
        match port {
            Some(port) => {
                self.port = Some(port.number);
                self.pier = number.or_else(|| ocean.piers_of(port.number).next());
                self.state = FIXED;
                self.aboard = aboard;
                true
            }
            None => {
                self.state = UNFIXED;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = concat!(
        "uid\tocean\tsea\tcol\trow\tn\tne\te\tse\ts\tsw\tw\tnw\n",
        "7116001\tSleeping Drake Harbor\tNorth Blue\t10\t53\t\t\t\t\t\t\t\t\n",
        "7142002\tSleeping Drake Harbor\tSleeping Drake Harbor, Pier One\n",
        "7142003\tSleeping Drake Harbor\tSleeping Drake Harbor, Pier Two\n",
    );

    #[test]
    fn fix_finds_the_port_by_pier_title() {
        let ocean = Ocean::read(TABLE);
        let mut nav = Navigator::default();
        assert_eq!(nav.state, 0);
        assert!(nav.fix(
            &ocean,
            &Standing::new("Sleeping Drake Harbor, Pier Two", None, Some(7142003), 1),
            false,
        ));
        assert_eq!(nav.port, Some(7142003));
        assert_eq!(nav.pier, Some(7142003));
        assert_eq!(nav.state, FIXED);
        assert!(!nav.aboard);
    }

    #[test]
    fn fix_reports_failure_and_resets_the_state() {
        let ocean = Ocean::read(TABLE);
        let mut nav = Navigator {
            state: FIXED,
            port: Some(1),
            pier: Some(2),
            aboard: true,
        };
        assert!(!nav.fix(&ocean, &Standing::new("", None, None, 1), true));
        assert_eq!(nav.state, UNFIXED);
        assert!(!nav.fix(&ocean, &Standing::new("Somewhere Else", None, None, 1), true));
        assert_eq!(nav.state, UNFIXED);
    }

    /// The one-bit aboard field is what the original keeps beside the state byte
    /// (`strb w8, [x21, #0x1]` at `0x1015b4cf4`); the state byte itself is only
    /// known to be "fixed" (`1`/`2`/`3` are three outcomes this build does not
    /// tell apart — see [`FIXED`]).
    #[test]
    fn aboard_is_recorded_beside_the_state_byte() {
        let ocean = Ocean::read(TABLE);
        let mut nav = Navigator::default();
        assert!(nav.fix(
            &ocean,
            &Standing::new("Sleeping Drake Harbor, Pier One", None, Some(7142002), 1),
            true,
        ));
        assert_eq!(nav.state, FIXED);
        assert_ne!(nav.state, UNFIXED);
        assert!(nav.aboard);
        // A failed fix does not claim a state, and does not claim the flag.
        assert!(!nav.fix(&ocean, &Standing::new("Nowhere", None, None, 1), false));
        assert_eq!(nav.state, UNFIXED);
        assert!(nav.aboard, "a failed fix leaves the last good flag alone");
    }
}
