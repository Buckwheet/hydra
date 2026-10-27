//! Spell-loss lines (`SpellLoss`).
//!
//! Recovered: def family `"spell_loss"`, named groups `"target"` and
//! `"spell_name"` (both literals), plus `target::link_in`.

/// A "you lose your spell" line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpellLoss {
    pub target: Option<String>,
    pub spell_name: Option<String>,
}

impl SpellLoss {
    /// @0x0000000101bfe79c (1708 B)
    pub fn classify(text: &str) -> Option<SpellLoss> {
        let def = super::defs::defs().first_match(text)?;
        if def.family != "spell_loss" {
            return None;
        }
        Some(SpellLoss {
            target: None,
            spell_name: Some(def.name.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_needs_definitions() {
        assert_eq!(SpellLoss::classify("Your spell is disrupted!"), None);
    }
}
