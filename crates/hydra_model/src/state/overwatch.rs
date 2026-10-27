//! Overwatch (sentry) state.
//!
//! `Overwatch::clear` (12284 B) is a large classifier whose `Debug` output
//! names the fields `SecondaryWound`, `wound_rank`, `location`, `children`,
//! `relation`, `in_encum`, `Abnormal`, `Reserved`, `is_final`; the literals
//! `"The air clears as the deadly volley of arrows abates"` /
//! `"hail of arrows arcs down from above"` are the volley messages.

/// The overwatch state.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Overwatch {
    /// `in_encum` — the encumbrance marker the block reports.
    pub in_encum: bool,
    /// Volley rounds seen.
    pub volley: u32,
    /// `// UNKNOWN:` the abnormal/reserved flags.
    pub abnormal: bool,
    pub reserved: bool,
    pub is_final: bool,
}

impl Overwatch {
    /// @0x0000000101b80450 (12284 B)
    pub fn clear(&mut self, line: &str) -> bool {
        let mut changed = false;
        if line.contains("The air clears as the deadly volley of arrows abates") {
            if self.volley != 0 {
                self.volley = 0;
                changed = true;
            }
            if self.is_final {
                self.is_final = false;
                changed = true;
            }
        } else if line.contains("hail of arrows arcs down from above")
            || line.contains("volley of arrows")
        {
            self.volley = self.volley.saturating_add(1);
            changed = true;
        }
        changed
    }

    /// The two volley messages recovered verbatim.
    pub const VOLLEY_START: &'static str = "hail of arrows arcs down from above";
    pub const VOLLEY_END: &'static str = "The air clears as the deadly volley of arrows abates";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volley_messages_are_recovered_verbatim() {
        let mut overwatch = Overwatch::default();
        assert!(overwatch.clear(Overwatch::VOLLEY_START));
        assert_eq!(overwatch.volley, 1);
        assert!(overwatch.clear(Overwatch::VOLLEY_START));
        assert_eq!(overwatch.volley, 2);
        assert!(overwatch.clear(Overwatch::VOLLEY_END));
        assert_eq!(overwatch.volley, 0);
        assert!(!overwatch.clear(Overwatch::VOLLEY_END));
    }

    #[test]
    fn unrelated_lines_change_nothing() {
        let mut overwatch = Overwatch::default();
        assert!(!overwatch.clear("You stand still."));
    }
}
