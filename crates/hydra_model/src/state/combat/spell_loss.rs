//! Spell-loss lines (`SpellLoss`).
//!
//! Recovered: def family `"spell_loss"`, named groups `"target"` and
//! `"spell_name"` (both literals), plus `target::link_in`. The values come from
//! the hit's capture set ([`crate::pattern::Groups`]); when the row's pattern
//! carries no `spell_name` group the row's `name` column is used, which is what
//! the recovered code falls back to.

use super::defs::{defs, Defs};

/// A "you lose your spell" line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpellLoss {
    pub target: Option<String>,
    pub spell_name: Option<String>,
}

impl SpellLoss {
    /// @0x0000000101bfe79c (1708 B)
    pub fn classify(text: &str) -> Option<SpellLoss> {
        SpellLoss::classify_with_defs(defs(), text)
    }

    /// [`SpellLoss::classify`] against a caller-supplied table.
    pub fn classify_with_defs(table: &Defs, text: &str) -> Option<SpellLoss> {
        let (def, groups) = table.first_match_groups(text)?;
        if def.family != "spell_loss" {
            return None;
        }
        Some(SpellLoss {
            target: groups.text("target"),
            spell_name: Some(
                groups
                    .text("spell_name")
                    .unwrap_or_else(|| def.name.clone()),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::defs::Def;
    use super::*;

    #[test]
    fn classify_needs_definitions() {
        assert_eq!(SpellLoss::classify("Your spell is disrupted!"), None);
    }

    fn table() -> Defs {
        Defs {
            defs: vec![
                Def::new(
                    "spell_loss",
                    "focus",
                    "damaging",
                    1,
                    "",
                    "",
                    "^(?P<target>The kobold) disrupts your (?P<spell_name>\\w+)",
                ),
                Def::new(
                    "spell_loss",
                    "disrupted",
                    "damaging",
                    2,
                    "",
                    "",
                    "^Your spell is disrupted",
                ),
            ],
        }
    }

    #[test]
    fn the_target_and_spell_name_are_captured() {
        let table = table();
        let loss =
            SpellLoss::classify_with_defs(&table, "The kobold disrupts your Fireball").unwrap();
        assert_eq!(loss.target.as_deref(), Some("The kobold"));
        assert_eq!(loss.spell_name.as_deref(), Some("Fireball"));
    }

    #[test]
    fn a_row_without_the_group_keeps_its_name() {
        let table = table();
        let loss = SpellLoss::classify_with_defs(&table, "Your spell is disrupted").unwrap();
        assert_eq!(loss.target, None);
        assert_eq!(loss.spell_name.as_deref(), Some("disrupted"));
    }
}
