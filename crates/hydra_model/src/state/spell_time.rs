//! Spell cost / spellsong timing.
//!
//! Recovered:
//! * `spell_cost` (408 B) = `f64::from_str` + `spells::expr::evaluate`,
//! * `spellsong_timeleft` (492 B) uses `Instant::elapsed`,
//! * `evaluate` (2568 B) calls `spells::expr::evaluate`, `SkillSet::ranks`,
//!   `SkillKind::key`, `Effects::active` and `Effects::remaining`, and formats
//!   with `u16` display,
//! * `value_names` (1192 B) returns the variable names the expression language
//!   exposes.

use std::time::Instant;

/// The spell casting state.
#[derive(Default, Clone, Debug)]
pub struct SpellTiming {
    /// When the current spellsong started.
    pub spellsong_started: Option<Instant>,
    /// Duration of the current spellsong.
    pub spellsong_duration: u32,
}

impl crate::state::GameState {
    /// @0x0000000101bcc4b8 (2568 B) — evaluate an expression against the
    /// character's skills and active effects.
    pub fn evaluate(&self, expression: &str) -> Option<f64> {
        let skills = &self.character.skills;
        let effects = &self.effects;
        crate::spells::expr::evaluate(expression, &|name: &str| {
            let lower = name.to_lowercase();
            if let Some(kind) = crate::state::character::skills::SkillKind::parse(&lower) {
                return Some(skills.ranks(kind) as f64);
            }
            if let Some(remaining) = effects.remaining(&lower) {
                return Some(remaining as f64);
            }
            // Recovered helper: `SkillKind::key` is consulted with the raw name.
            if let Some(kind) = crate::state::character::skills::SkillKind::parse(name) {
                return Some(skills.ranks(kind) as f64);
            }
            None
        })
    }

    /// @0x0000000101bcc134 (408 B) — the mana cost of a spell expression.
    pub fn spell_cost(&self, expression: &str) -> Option<f64> {
        self.evaluate(expression)
    }

    /// @0x0000000101bcc2cc (492 B) — seconds left in the current spellsong.
    pub fn spellsong_timeleft(&self) -> Option<u32> {
        let started = self.timing.spellsong_started?;
        let elapsed = started.elapsed().as_secs().min(u32::MAX as u64) as u32;
        Some(self.timing.spellsong_duration.saturating_sub(elapsed))
    }

    /// @0x0000000101f6d1a8 (1192 B) — the names usable in an expression.
    ///
    /// `// UNKNOWN:` the full list; the recovered function builds it from the
    /// skill keys plus the effect names.
    pub fn value_names(&self) -> Vec<String> {
        let mut names: Vec<String> = crate::state::character::skills::SkillKind::ALL
            .iter()
            .map(|kind| kind.key().to_lowercase())
            .collect();
        for effect in self.effects.effects.keys() {
            let effect = effect.to_lowercase();
            if !names.contains(&effect) {
                names.push(effect);
            }
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::character::skills::{Skill, SkillKind};

    #[test]
    fn spell_cost_evaluates_arithmetic() {
        let state = crate::state::GameState::default();
        assert_eq!(state.spell_cost("3 * 4"), Some(12.0));
        assert_eq!(state.spell_cost("(2 + 3) / 2"), Some(2.5));
        assert_eq!(state.spell_cost("nonsense("), None);
    }

    #[test]
    fn skill_names_resolve_to_ranks() {
        let mut state = crate::state::GameState::default();
        state.character.skills.replace(
            SkillKind::HarnessPower,
            Skill {
                rank: 40,
                bonus: 80,
            },
        );
        assert_eq!(state.evaluate("harness power"), Some(40.0));
        assert_eq!(state.evaluate("harness power * 2"), Some(80.0));
    }

    #[test]
    fn effect_names_resolve_to_remaining() {
        let mut state = crate::state::GameState::default();
        state.effects.effects.insert(
            "haste".into(),
            crate::state::effects::Effect {
                name: "haste".into(),
                remaining: 12,
                category: "Buffs".into(),
                lasting: false,
            },
        );
        assert_eq!(state.evaluate("haste"), Some(12.0));
    }

    #[test]
    fn spellsong_timeleft_counts_down() {
        let mut state = crate::state::GameState::default();
        assert_eq!(state.spellsong_timeleft(), None);
        state.timing.spellsong_started = Some(Instant::now());
        state.timing.spellsong_duration = 30;
        assert_eq!(state.spellsong_timeleft(), Some(30));
    }

    #[test]
    fn value_names_include_every_skill() {
        let state = crate::state::GameState::default();
        let names = state.value_names();
        assert!(names.contains(&"harness power".to_string()));
        assert_eq!(
            names.len(),
            crate::state::character::skills::SkillKind::ALL.len()
        );
    }
}
