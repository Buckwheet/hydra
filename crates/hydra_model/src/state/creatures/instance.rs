//! One live creature.
//!
//! Recovered field names from the string pool (the SQL/serde field runs):
//! `first_hostile`, `amputated`, `damage_taken`, `fatal_crit`, `ucs_position`,
//! `ucs_tier`, `ucs_smote_at`, `ucs_updated_at`, `stun_rounds`, `stun_until`,
//! `template_max_hp`, `stated_health`, `stated_max_health`, `ending_phase`,
//! `marks`, `first_seen_at`, `last_seen_at`, plus the crit-injury words
//! (`wound_rank`, `secondary_wound`, `location`, `children`, `relation`,
//! `Abnormal`, `Reserved`, `is_final`).
//!
//! Recovered methods and their shapes: `new` (952 B, calls `creature::by_name`
//! + `ally::classify`), `template` (324 B), `is_boss` (300 B), `max_hp` (40 B),
//! `current_hp` (80 B), `hp_percent` (104 B), `dead` (152 B), `corpse` (304 B),
//! `hostile` (156 B), `valid_target` (868 B), `flag` (132 B), `has_status`
//! (152 B), `statuses` (84 B), `add_status` (500 B), `add_injury` (452 B),
//! `appendage` (2408 B) and `sync_crtr_status_at` (4088 B).

use std::collections::BTreeMap;

use crate::state::creatures::body::BodyPart;

/// A wound on one body part.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Injury {
    pub rank: u32,
    pub amputated: bool,
    pub fatal: bool,
    pub secondary: Option<u32>,
}

/// A creature the client has seen.
#[derive(Clone, Debug)]
pub struct CreatureInstance {
    pub id: u32,
    pub name: String,
    pub noun: String,
    /// Level from the bestiary template, when known.
    pub level: Option<u32>,
    /// `template_max_hp`.
    pub max_hp: Option<u32>,
    /// `stated_health` / `stated_max_health` from the health phrase.
    pub current_hp: Option<u32>,
    pub dead: bool,
    pub corpse: bool,
    pub hostile: bool,
    pub boss: bool,
    /// Status words -> `true` while they hold.
    pub statuses: BTreeMap<String, bool>,
    /// `location -> injury`.
    pub injuries: BTreeMap<BodyPart, Injury>,
    pub amputated: Vec<BodyPart>,
    pub stun_until: Option<u32>,
    pub stun_rounds: Option<u32>,
    pub first_seen: Option<u32>,
    pub last_seen: Option<u32>,
    pub ending_phase: bool,
    pub marks: u32,
}

impl CreatureInstance {
    /// @0x0000000101bf3348 (952 B)
    pub fn new(id: u32, name: &str) -> CreatureInstance {
        let template = crate::state::creature::by_name(name);
        let ally = crate::state::creatures::ally::classify(name);
        CreatureInstance {
            id,
            name: name.to_string(),
            noun: template
                .map(|t| t.noun.clone())
                .unwrap_or_else(|| last_word(name)),
            level: template.and_then(|t| t.level),
            max_hp: template.and_then(|t| t.max_hp),
            current_hp: None,
            dead: false,
            corpse: false,
            hostile: !ally,
            boss: false,
            statuses: BTreeMap::new(),
            injuries: BTreeMap::new(),
            amputated: Vec::new(),
            stun_until: None,
            stun_rounds: None,
            first_seen: None,
            last_seen: None,
            ending_phase: false,
            marks: 0,
        }
    }

    /// @0x0000000101bf3b90 (324 B)
    pub fn template(&self) -> Option<&'static crate::state::creature::Creature> {
        crate::state::creature::by_name(&self.name)
    }

    /// @0x0000000101bf3a10 (300 B)
    pub fn is_boss(&self) -> bool {
        self.boss
    }

    /// @0x0000000101b? (40 B)
    pub fn max_hp(&self) -> Option<u32> {
        self.max_hp
    }

    /// @0x0000000101b? (80 B)
    pub fn current_hp(&self) -> Option<u32> {
        self.current_hp
    }

    /// @0x0000000101b? (104 B) — `current / max` as a percentage.
    pub fn hp_percent(&self) -> Option<u32> {
        let current = self.current_hp?;
        let max = self.max_hp.or(self.template().and_then(|t| t.max_hp))?;
        if max == 0 {
            return None;
        }
        Some(((current.min(max)) as u64 * 100 / max as u64) as u32)
    }

    /// @0x0000000101b? (152 B)
    pub fn dead(&self) -> bool {
        self.dead
    }

    /// @0x0000000101b? (304 B)
    pub fn corpse(&self) -> bool {
        self.corpse || self.dead
    }

    /// @0x0000000101b? (156 B)
    pub fn hostile(&self) -> bool {
        self.hostile
    }

    /// @0x0000000101bf1fec (868 B) — can this instance be targeted?
    pub fn valid_target(&self) -> bool {
        !self.dead && !self.name.is_empty()
    }

    /// @0x0000000101b? (132 B) — set/clear a boolean flag by name.
    pub fn flag(&mut self, name: &str, value: bool) -> bool {
        match name {
            "dead" => std::mem::replace(&mut self.dead, value) != value,
            "corpse" => std::mem::replace(&mut self.corpse, value) != value,
            "hostile" => std::mem::replace(&mut self.hostile, value) != value,
            "boss" => std::mem::replace(&mut self.boss, value) != value,
            "ending_phase" => std::mem::replace(&mut self.ending_phase, value) != value,
            _ => self.add_status(name, value),
        }
    }

    /// @0x0000000101b? (152 B)
    pub fn has_status(&self, status: &str) -> bool {
        self.statuses.get(status).copied().unwrap_or(false)
    }

    /// @0x0000000101b? (84 B)
    pub fn statuses(&self) -> Vec<&str> {
        self.statuses
            .iter()
            .filter(|(_, on)| **on)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// @0x0000000101b? (500 B)
    pub fn add_status(&mut self, status: &str, on: bool) -> bool {
        let status = status.trim().to_ascii_lowercase();
        if status.is_empty() {
            return false;
        }
        if self.statuses.get(&status) == Some(&on) {
            return false;
        }
        self.statuses.insert(status, on);
        true
    }

    /// @0x0000000101b? (452 B)
    pub fn add_injury(&mut self, part: BodyPart, injury: Injury) -> bool {
        if self.injuries.get(&part) == Some(&injury) {
            return false;
        }
        if injury.amputated && !self.amputated.contains(&part) {
            self.amputated.push(part);
        }
        self.injuries.insert(part, injury);
        true
    }

    /// @0x0000000101bf3cd4 (2408 B) — injuries on one part.
    pub fn appendage(&self, part: BodyPart) -> Option<&Injury> {
        self.injuries.get(&part)
    }

    /// @0x0000000101bf2350 (4088 B) — fold a `CRTR`/status block in.
    pub fn sync_crtr_status_at(&mut self, at: u32, statuses: &[&str]) -> bool {
        let mut changed = false;
        for status in statuses {
            changed |= self.add_status(status, true);
        }
        if self.last_seen != Some(at) {
            self.last_seen = Some(at);
            changed = true;
        }
        changed
    }
}

fn last_word(name: &str) -> String {
    name.split_whitespace().last().unwrap_or(name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_derives_a_noun() {
        let instance = CreatureInstance::new(1, "a scruffy kobold");
        assert_eq!(instance.name, "a scruffy kobold");
        assert_eq!(instance.noun, "kobold");
        assert_eq!(instance.id, 1);
    }

    #[test]
    fn hp_percent_uses_the_reference_maximum() {
        let mut instance = CreatureInstance::new(1, "a kobold");
        instance.current_hp = Some(25);
        instance.max_hp = Some(100);
        assert_eq!(instance.hp_percent(), Some(25));
        instance.current_hp = Some(200);
        assert_eq!(instance.hp_percent(), Some(100));
        instance.max_hp = None;
        assert_eq!(instance.hp_percent(), None);
    }

    #[test]
    fn statuses_are_deduplicated() {
        let mut instance = CreatureInstance::new(1, "a kobold");
        assert!(instance.add_status("Stunned", true));
        assert!(!instance.add_status("stunned", true));
        assert!(instance.has_status("stunned"));
        assert_eq!(instance.statuses(), vec!["stunned"]);
        assert!(instance.add_status("stunned", false));
        assert!(!instance.has_status("stunned"));
    }

    #[test]
    fn injuries_track_amputation() {
        let mut instance = CreatureInstance::new(1, "a kobold");
        let injury = Injury {
            rank: 9,
            amputated: true,
            ..Injury::default()
        };
        assert!(instance.add_injury(BodyPart::LeftArm, injury));
        assert!(!instance.add_injury(BodyPart::LeftArm, injury));
        assert_eq!(instance.amputated, vec![BodyPart::LeftArm]);
        assert_eq!(instance.appendage(BodyPart::LeftArm).unwrap().rank, 9);
    }

    #[test]
    fn flags_route_to_fields_then_statuses() {
        let mut instance = CreatureInstance::new(1, "a kobold");
        assert!(instance.flag("dead", true));
        assert!(instance.dead());
        assert!(instance.corpse());
        assert!(!instance.valid_target());
        assert!(instance.flag("webbed", true));
        assert!(instance.has_status("webbed"));
    }

    #[test]
    fn sync_records_last_seen() {
        let mut instance = CreatureInstance::new(1, "a kobold");
        assert!(instance.sync_crtr_status_at(10, &["prone"]));
        assert_eq!(instance.last_seen, Some(10));
        assert!(!instance.sync_crtr_status_at(10, &["prone"]));
        assert!(instance.has_status("prone"));
    }
}
