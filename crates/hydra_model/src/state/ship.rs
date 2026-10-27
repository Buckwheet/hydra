//! Sailing state.
//!
//! `aboard` (10888 B) parses the ship status block; recovered literals:
//! `" - You're pretty sure you're not on the ocean"`, `"Docked at: "`, `", "`.
//! `Ship::lapse` (1624 B) ages the recorded state.
//! `reports::ShipInfo::look` (660 B) / `is_mine` (2644 B) are the look-at
//! helpers (`" is "`, `": "`).

pub mod reports;

/// The player's ship.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Ship {
    pub name: Option<String>,
    /// Where the ship is docked, when docked.
    pub docked_at: Option<String>,
    /// `false` once the game says the character is not on the ocean.
    pub aboard: bool,
    /// `// UNKNOWN:` the lapse tick (seconds); only the accessor is recovered.
    pub lapse: Option<u32>,
}

impl Ship {
    /// @0x0000000101be2128 (10888 B)
    pub fn aboard(&mut self, line: &str) -> bool {
        let mut changed = false;
        if line.contains("You're pretty sure you're not on the ocean") {
            if self.aboard {
                self.aboard = false;
                changed = true;
            }
            if self.docked_at.take().is_some() {
                changed = true;
            }
            return changed;
        }
        if let Some((_, rest)) = line.split_once("Docked at: ") {
            let port = rest
                .split(',')
                .next()
                .unwrap_or(rest)
                .trim()
                .trim_end_matches('.')
                .to_string();
            changed |= !self.aboard;
            self.aboard = true;
            if self.docked_at.as_deref() != Some(&port) {
                self.docked_at = Some(port);
                changed = true;
            }
        }
        changed
    }

    /// @0x0000000101be1ad0 (1624 B) — age the recorded state.
    pub fn lapse(&mut self, seconds: u32) -> bool {
        match self.lapse {
            Some(current) if current == seconds => false,
            _ => {
                self.lapse = Some(seconds);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docking_is_recorded() {
        let mut ship = Ship::default();
        assert!(ship.aboard("Docked at: Wehnimer's Landing, Pier 3"));
        assert!(ship.aboard);
        assert_eq!(ship.docked_at.as_deref(), Some("Wehnimer's Landing"));
        assert!(!ship.aboard("Docked at: Wehnimer's Landing, Pier 3"));
    }

    #[test]
    fn not_on_the_ocean_clears_the_state() {
        let mut ship = Ship::default();
        ship.aboard("Docked at: Solhaven");
        assert!(ship.aboard(" - You're pretty sure you're not on the ocean"));
        assert!(!ship.aboard);
        assert_eq!(ship.docked_at, None);
    }

    #[test]
    fn lapse_reports_change() {
        let mut ship = Ship::default();
        assert!(ship.lapse(5));
        assert!(!ship.lapse(5));
    }
}
