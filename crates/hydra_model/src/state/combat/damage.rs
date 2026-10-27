//! `DamageLine` — the plain "N points of damage" line.
//!
//! Recovered literals: `"damage"` (the def family), `" hits!"`, `"target"`.
//! The classifier runs `Runs::plain`, `Defs::first_match` and reads the named
//! groups (`target`).
//!
//! `// UNKNOWN:` the def table is not embedded, so classification depends on
//! the caller installing definitions via [`crate::state::combat::defs`].

use super::defs::defs;

/// A damage line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DamageLine {
    pub target: Option<String>,
    pub amount: u32,
}

impl DamageLine {
    /// @0x0000000101ba031c (1108 B)
    pub fn classify(text: &str) -> Option<DamageLine> {
        let def = defs().first_match(text)?;
        if def.family != "damage" {
            return None;
        }
        Some(DamageLine {
            target: None,
            amount: first_number(text).unwrap_or(0),
        })
    }
}

fn first_number(text: &str) -> Option<u32> {
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_definitions_nothing_matches() {
        assert_eq!(DamageLine::classify("The kobold takes 12 points of damage!"), None);
    }
}
