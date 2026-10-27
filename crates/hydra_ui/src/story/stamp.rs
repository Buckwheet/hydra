//! Timestamps in front of scrollback lines.
//!
//! `Stamp::now` @0x101842704 is 1.2 KB: it resolves the US-Eastern civil time
//! and formats it.  The two recovered literals are `" PM AM"` (0x103991d88, a
//! four-entry suffix table) and serde's `always valid Unix epoch day` /
//! `always valid civil subsecond`, which is how `jiff` describes the stored
//! values.  `Hours` is the serde type the stamp stores.

use gs_base::elanthian_time;

/// A clock time in the game's time zone, as a stamp shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hours {
    /// 0..=23.
    pub hour: u8,
    /// 0..=59.
    pub minute: u8,
}

impl Hours {
    /// `@0x101842704` — the Eastern civil time at a Unix timestamp.
    pub fn at(unix_secs: i64) -> Hours {
        let off = if elanthian_time::eastern_is_dst(unix_secs) {
            elanthian_time::EDT_OFFSET
        } else {
            elanthian_time::EST_OFFSET
        };
        let local = unix_secs.saturating_add(off);
        let secs = local.rem_euclid(elanthian_time::SECS_PER_DAY);
        Hours {
            hour: (secs / 3600) as u8,
            minute: ((secs % 3600) / 60) as u8,
        }
    }

    /// The 12-hour form and its suffix, from the recovered `" PM AM"` table.
    pub fn twelve(&self) -> (u8, &'static str) {
        let suffix = if self.hour < 12 { " AM" } else { " PM" };
        let mut h = self.hour % 12;
        if h == 0 {
            h = 12;
        }
        (h, suffix)
    }
}

/// One stamped line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    pub hours: Hours,
}

impl Stamp {
    /// `@0x101842704` — a stamp for a Unix timestamp.
    pub fn now(unix_secs: i64) -> Stamp {
        Stamp {
            hours: Hours::at(unix_secs),
        }
    }

    /// `@0x101842bd8` — the text put in front of a line: `"10:05 PM"`, with the
    /// hour unpadded and the minute always two digits.
    pub fn said(&self) -> String {
        let (h, suffix) = self.hours.twelve();
        format!("{h}:{:02}{suffix}", self.hours.minute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eastern_hours() {
        // 2024-01-01T12:00:00Z is 07:00 EST.
        let h = Hours::at(1_704_110_400);
        assert_eq!((h.hour, h.minute), (7, 0));
        // 2024-07-01T12:00:00Z is 08:00 EDT.
        let h = Hours::at(1_719_835_200);
        assert_eq!((h.hour, h.minute), (8, 0));
    }

    #[test]
    fn twelve_hour_form() {
        assert_eq!(Hours { hour: 0, minute: 5 }.twelve(), (12, " AM"));
        assert_eq!(Hours { hour: 11, minute: 59 }.twelve(), (11, " AM"));
        assert_eq!(Hours { hour: 12, minute: 0 }.twelve(), (12, " PM"));
        assert_eq!(Hours { hour: 23, minute: 30 }.twelve(), (11, " PM"));
    }

    #[test]
    fn stamped_text() {
        let s = Stamp {
            hours: Hours {
                hour: 22,
                minute: 5,
            },
        };
        assert_eq!(s.said(), "10:05 PM");
        let s = Stamp {
            hours: Hours { hour: 9, minute: 0 },
        };
        assert_eq!(s.said(), "9:00 AM");
    }
}
