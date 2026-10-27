//! The combat tracker.
//!
//! Recovered field names come straight out of
//! `<CombatTracker as core::fmt::Debug>::fmt` (2992 B at `0x101bdb344`), which
//! calls `debug_struct("CombatTracker")` and then names each field:
//! `held_cast`, `held_pre_flares`, `active_assault`, `volley_round`, `tremors`,
//! `crit_tables`, `pending`, `dropped`.
//!
//! Recovered methods: `invalidate_for_reconnect` (256 B), `publish` (424 B),
//! `set_crit_tables` (80 B) and `take_facts` (56 B; returns the pending vector).

use crate::crit::CritTables;

/// Facts queued for the persistence layer.
///
/// `// UNKNOWN:` the variant set; the recovered `Debug` names `pending` and
/// `dropped`, and `take_facts` hands the whole vector out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CombatFact {
    Attack(String),
    Damage(String),
    Flare(String),
    SpellLoss(String),
    Outcome(String),
    Resolution(String),
}

/// The in-progress combat state.
#[derive(Default, Clone, Debug)]
pub struct CombatTracker {
    /// A cast happened but its resolution line has not arrived yet.
    pub held_cast: Option<String>,
    /// Flares held before the attack line.
    pub held_pre_flares: Vec<String>,
    /// The open assault bracket.
    pub active_assault: Option<String>,
    /// The current volley round (area attacks).
    pub volley_round: Option<u32>,
    /// Tremor/volley markers.
    pub tremors: Vec<String>,
    /// The parsed crit table (set once).
    pub crit_tables: Option<CritTables>,
    /// Facts waiting to be taken.
    pub pending: Vec<CombatFact>,
    /// Facts dropped because no bracket was open.
    pub dropped: Vec<CombatFact>,
}

impl CombatTracker {
    /// @0x0000000101bdb? (80 B)
    pub fn set_crit_tables(&mut self, tables: CritTables) -> Option<CritTables> {
        self.crit_tables.replace(tables)
    }

    /// @0x0000000101bdb? (424 B) — queue a fact, or drop it when nothing is
    /// open.
    pub fn publish(&mut self, fact: CombatFact) -> bool {
        let open = self.active_assault.is_some() || !self.pending.is_empty();
        if open {
            self.pending.push(fact);
            true
        } else {
            self.dropped.push(fact);
            false
        }
    }

    /// @0x0000000101bdb? (56 B)
    pub fn take_facts(&mut self) -> Vec<CombatFact> {
        std::mem::take(&mut self.pending)
    }

    /// @0x0000000101b? (256 B) — clear everything that cannot survive a
    /// reconnect.
    pub fn invalidate_for_reconnect(&mut self) {
        self.held_cast = None;
        self.held_pre_flares.clear();
        self.active_assault = None;
        self.volley_round = None;
        self.tremors.clear();
        self.dropped.clear();
        self.pending.clear();
    }

    /// Does the tracker hold anything the UI should show?
    pub fn is_active(&self) -> bool {
        self.active_assault.is_some()
            || self.held_cast.is_some()
            || !self.held_pre_flares.is_empty()
            || !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_match_the_recovered_debug_names() {
        let tracker = CombatTracker::default();
        let debug = format!("{tracker:?}");
        for field in [
            "held_cast",
            "held_pre_flares",
            "active_assault",
            "volley_round",
            "tremors",
            "crit_tables",
            "pending",
            "dropped",
        ] {
            assert!(debug.contains(field), "{field} missing from {debug}");
        }
    }

    #[test]
    fn publish_drops_when_nothing_is_open() {
        let mut tracker = CombatTracker::default();
        assert!(!tracker.publish(CombatFact::Attack("x".into())));
        assert_eq!(tracker.dropped.len(), 1);
        tracker.active_assault = Some("fury".into());
        assert!(tracker.publish(CombatFact::Damage("y".into())));
        assert_eq!(tracker.pending.len(), 1);
    }

    #[test]
    fn take_facts_empties_pending() {
        let mut tracker = CombatTracker::default();
        tracker.active_assault = Some("fury".into());
        tracker.publish(CombatFact::Attack("x".into()));
        assert_eq!(tracker.take_facts().len(), 1);
        assert!(tracker.take_facts().is_empty());
    }

    #[test]
    fn reconnect_clears_everything() {
        let mut tracker = CombatTracker::default();
        tracker.active_assault = Some("fury".into());
        tracker.held_cast = Some("fireball".into());
        tracker.held_pre_flares.push("flare".into());
        tracker.volley_round = Some(3);
        tracker.publish(CombatFact::Attack("x".into()));
        tracker.invalidate_for_reconnect();
        assert!(!tracker.is_active());
        assert!(tracker.pending.is_empty() && tracker.dropped.is_empty());
    }

    #[test]
    fn crit_tables_are_installed_once() {
        let mut tracker = CombatTracker::default();
        let tables = CritTables::default();
        assert!(tracker.set_crit_tables(tables).is_none());
        assert!(tracker.crit_tables.is_some());
        assert!(tracker.set_crit_tables(CritTables::default()).is_some());
    }
}
