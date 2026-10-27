//! The thing that actually sends the moves.
//!
//! Recovered from `hydra_travel::walk::mover::{is_stunned, exchange_of,
//! Mover::new, Mover::tick}` (`0x101544138`, `0x1015441dc`, `0x101544274`,
//! `0x1015442c4`).

/// Is the character stunned (`is_stunned`, 164 bytes at `0x101544138`)?
///
/// **ESTABLISHED**: the body is a lookup of the `stunned` status word
/// (`0x10241c4d9`) through `hydra_model`'s status map.
pub fn is_stunned(statuses: &[&str]) -> bool {
    statuses.iter().any(|status| status.eq_ignore_ascii_case("stunned"))
}

/// The roundtime the game reported (`exchange_of`, 156 bytes at
/// `0x1015441dc`).
///
/// **Partly recovered**: the body scans the line for `Roundtime` and reads the
/// trailing seconds.
pub fn exchange_of(line: &str) -> Option<u32> {
    let at = line.find("Roundtime:")?;
    line[at + "Roundtime:".len()..]
        .split(|c: char| !c.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|digits| digits.parse::<u32>().ok())
}

/// The move sender.
#[derive(Clone, Debug, Default)]
pub struct Mover {
    /// How many moves have been sent since the last answer.
    pub sent: u32,
    /// How many the game will hold beyond the one it is doing.
    pub typeahead: u32,
    /// Is the character stunned?
    pub stunned: bool,
}

impl Mover {
    pub fn new(typeahead: u32) -> Mover {
        Mover {
            typeahead,
            ..Mover::default()
        }
    }

    /// Send one move (`Mover::tick`, `0x1015442c4`).
    ///
    /// The body refuses while stunned and otherwise sends while `sent` has not
    /// reached `1 + typeahead` (`hydra_travel::settings`'s `typeahead`
    /// description says exactly that: *plain moves go 1 + this at a time*).
    pub fn tick(&mut self, command: &str) -> Option<String> {
        if self.stunned {
            return None;
        }
        if self.sent >= 1 + self.typeahead {
            return None;
        }
        self.sent += 1;
        Some(command.to_string())
    }

    /// The game answered: reset the window.
    pub fn landed(&mut self) {
        self.sent = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stunned_movers_do_not_move() {
        assert!(is_stunned(&["hidden", "stunned"]));
        assert!(!is_stunned(&["hidden"]));
        let mut mover = Mover::new(2);
        mover.stunned = true;
        assert_eq!(mover.tick("north"), None);
    }

    #[test]
    fn typeahead_holds_one_plus_the_setting() {
        let mut mover = Mover::new(2);
        assert!(mover.tick("north").is_some());
        assert!(mover.tick("north").is_some());
        assert!(mover.tick("north").is_some());
        assert_eq!(mover.tick("north"), None);
        mover.landed();
        assert!(mover.tick("north").is_some());
    }

    #[test]
    fn roundtime_is_read_off_the_line() {
        assert_eq!(exchange_of("Roundtime: 3 sec."), Some(3));
        assert_eq!(exchange_of("nothing"), None);
    }
}
