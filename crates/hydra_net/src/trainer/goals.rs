//! Skill goals for the play.net trainer.
//!
//! Recovered from `hydra_net::trainer::goals` (`0x101c66d0c` .. `0x101c6b9d0`).
//! A goal is "train this skill up to that rank"; the trainer page wants them as
//! `skill{n}` / `cm_spell` form fields, and the arithmetic that decides what to
//! submit next lives here.

use std::collections::BTreeMap;

/// The prefix every skill field on the trainer form carries.
pub const SKILL_FIELD: &str = "skill";
/// The field the spell-training checkbox uses.
pub const SPELL_FIELD: &str = "cm_spell";

/// A set of skill goals, keyed by the skill's page name.
///
/// The original's field type is a sorted map of name to rank; the trainer page
/// reads a `BTreeMap` out of [`SkillGoals::goal_fields`] (see
/// `trainer::page::form`, which builds one directly).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillGoals {
    goals: BTreeMap<String, i64>,
    /// The goals as they were when the page was read, so `changed` can tell.
    submitted: BTreeMap<String, i64>,
}

impl SkillGoals {
    /// An empty set of goals.
    pub fn new() -> SkillGoals {
        SkillGoals::default()
    }

    /// The goal for one skill, if it has one.
    ///
    /// Recovered from `0x101c67dc4`.
    pub fn goal(&self, skill: &str) -> Option<i64> {
        self.goals.get(skill).copied()
    }

    /// True when `skill` has a goal at all.
    ///
    /// Recovered from `0x101c67a68`.
    pub fn has(&self, skill: &str) -> bool {
        self.goals.contains_key(skill)
    }

    /// Set the goal for a skill.
    ///
    /// Recovered from `0x101c67860`: a goal of zero still records the skill
    /// (the page lists every skill), so the entry is kept.
    pub fn aim(&mut self, skill: &str, rank: i64) {
        self.goals.insert(skill.to_string(), rank.max(0));
    }

    /// Raise every goal by one.
    ///
    /// Recovered from `0x101c6747c`.
    pub fn up(&mut self) {
        for rank in self.goals.values_mut() {
            *rank += 1;
        }
    }

    /// Lower every goal by one, never below zero.
    ///
    /// Recovered from `0x101c67a80`.
    pub fn down(&mut self) {
        for rank in self.goals.values_mut() {
            *rank = (*rank - 1).max(0);
        }
    }

    /// Move one skill's goal by `step`, never below zero.
    ///
    /// Recovered from `0x101c67ddc`.
    pub fn step(&mut self, skill: &str, step: i64) {
        if let Some(rank) = self.goals.get_mut(skill) {
            *rank = (*rank + step).max(0);
        }
    }

    /// Forget every goal, and treat the page as freshly read.
    ///
    /// Recovered from `0x101c67e70`.
    pub fn reset(&mut self) {
        self.goals.clear();
        self.submitted.clear();
    }

    /// True when the goals differ from what was read off the page.
    ///
    /// Recovered from `0x101c67f40`: the two maps are compared.
    pub fn changed(&self) -> bool {
        self.goals != self.submitted
    }

    /// The skills that have a non-zero goal, sorted by name.
    ///
    /// Recovered from `0x101c67fc4`: the entries are collected in map order.
    pub fn targets(&self) -> Vec<(String, i64)> {
        self.goals
            .iter()
            .filter(|(_, rank)| **rank > 0)
            .map(|(skill, rank)| (skill.clone(), *rank))
            .collect()
    }

    /// The trainer cost of the next rank of a skill.
    ///
    /// Recovered from `0x101c67370`: the original keeps a per-skill cost table
    /// and scales it by the rank already trained.  UNKNOWN: the table is a
    /// static array that is not reachable from the symbol table, so this
    /// reconstruction reports the flat cost of one rank.
    pub fn cost_of_next(&self, skill: &str) -> i64 {
        // UNKNOWN: the original's per-skill cost table is a static array that the
        // symbol table does not name; one rank is reported as costing one.
        if self.has(skill) {
            1
        } else {
            0
        }
    }

    /// The form fields that carry the goals to the trainer.
    ///
    /// Recovered from `0x101c66d0c`: every skill becomes `skill{n}` in map
    /// order, the spell field is `cm_spell`, and the derived `phy_*`/`mnt_*`
    /// fields (`phy_tp`, `mnt_tp`, `phy_left`, `mnt_left`, `phy_conv`,
    /// `mnt_conv`, `phy_spent`, `mnt_spent`) are all zero unless a rank was
    /// trained.  The field names are the recovered literals at `0x103e7ff79`
    /// .. `0x103bfb238`.
    pub fn goal_fields(&self) -> BTreeMap<String, i64> {
        let mut fields = BTreeMap::new();
        for (i, (_, rank)) in self.goals.iter().enumerate() {
            fields.insert(format!("{}{}", SKILL_FIELD, i), *rank);
        }
        fields.insert(SPELL_FIELD.to_string(), 0);
        for name in [
            "phy_tp", "mnt_tp", "phy_left", "mnt_left", "phy_conv", "mnt_conv", "phy_spent",
            "mnt_spent",
        ] {
            fields.insert(name.to_string(), 0);
        }
        fields
    }

    /// Mark the goals as submitted, so [`SkillGoals::changed`] goes quiet.
    ///
    /// INFERRED: the original keeps the submitted copy alongside the goals; the
    /// symbol table only exposes the readers.
    pub fn submitted(&mut self) {
        self.submitted = self.goals.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goals_are_recorded_and_read_back() {
        let mut goals = SkillGoals::new();
        assert!(!goals.has("Climbing"));
        goals.aim("Climbing", 5);
        assert!(goals.has("Climbing"));
        assert_eq!(goals.goal("Climbing"), Some(5));
        assert_eq!(goals.goal("Swimming"), None);
    }

    #[test]
    fn goals_never_go_negative() {
        let mut goals = SkillGoals::new();
        goals.aim("Climbing", 0);
        goals.down();
        assert_eq!(goals.goal("Climbing"), Some(0));
        goals.aim("Climbing", 2);
        goals.step("Climbing", -5);
        assert_eq!(goals.goal("Climbing"), Some(0));
    }

    #[test]
    fn up_and_down_move_every_goal() {
        let mut goals = SkillGoals::new();
        goals.aim("A", 1);
        goals.aim("B", 2);
        goals.up();
        assert_eq!(goals.goal("A"), Some(2));
        assert_eq!(goals.goal("B"), Some(3));
        goals.down();
        assert_eq!(goals.goal("A"), Some(1));
        assert_eq!(goals.targets(), vec![("A".to_string(), 1), ("B".to_string(), 2)]);
    }

    #[test]
    fn targets_skip_the_untrained() {
        let mut goals = SkillGoals::new();
        goals.aim("A", 0);
        goals.aim("B", 3);
        assert_eq!(goals.targets(), vec![("B".to_string(), 3)]);
    }

    #[test]
    fn the_fields_are_numbered_in_map_order() {
        let mut goals = SkillGoals::new();
        goals.aim("B", 7);
        goals.aim("A", 2);
        let fields = goals.goal_fields();
        assert_eq!(fields.get("skill0"), Some(&2));
        assert_eq!(fields.get("skill1"), Some(&7));
        assert_eq!(fields.get(SPELL_FIELD), Some(&0));
        assert_eq!(fields.get("phy_tp"), Some(&0));
        assert_eq!(fields.get("mnt_spent"), Some(&0));
    }

    #[test]
    fn changed_tracks_the_submitted_copy() {
        let mut goals = SkillGoals::new();
        goals.aim("A", 1);
        assert!(goals.changed());
        goals.submitted();
        assert!(!goals.changed());
        goals.aim("A", 2);
        assert!(goals.changed());
        goals.reset();
        assert!(!goals.changed());
        assert_eq!(goals.goal("A"), None);
    }

    #[test]
    fn the_next_rank_has_a_cost() {
        let mut goals = SkillGoals::new();
        assert_eq!(goals.cost_of_next("A"), 0);
        goals.aim("A", 3);
        assert_eq!(goals.cost_of_next("A"), 1);
    }
}
