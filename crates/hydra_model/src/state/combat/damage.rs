//! `DamageLine` — the plain "N points of damage" line.
//!
//! Recovered literals: `"damage"` (the def family), `" hits!"`, `"target"`.
//! The classifier runs `Runs::plain`, `Defs::first_match` and reads the named
//! group `target` ([`crate::pattern::Groups`]); the number is the first integer
//! on the line.
//!
//! `// UNKNOWN:` the def table is not embedded, so classification depends on
//! the caller installing definitions via [`crate::state::combat::defs`] or
//! passing one to [`DamageLine::classify_with_defs`].

use super::defs::{defs, Defs};

/// A damage line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DamageLine {
    pub target: Option<String>,
    pub amount: u32,
}

impl DamageLine {
    /// @0x0000000101ba031c (1108 B)
    pub fn classify(text: &str) -> Option<DamageLine> {
        DamageLine::classify_with_defs(defs(), text)
    }

    /// [`DamageLine::classify`] against a caller-supplied table.
    pub fn classify_with_defs(table: &Defs, text: &str) -> Option<DamageLine> {
        let (def, groups) = table.first_match_groups(text)?;
        if def.family != "damage" {
            return None;
        }
        Some(DamageLine {
            target: groups.text("target"),
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
    use super::super::defs::Def;
    use super::*;

    #[test]
    fn without_definitions_nothing_matches() {
        assert_eq!(
            DamageLine::classify("The kobold takes 12 points of damage!"),
            None
        );
    }

    #[test]
    fn the_target_group_and_amount_are_read() {
        let table = Defs {
            defs: vec![Def::new(
                "damage",
                "points",
                "damaging",
                1,
                "",
                "",
                "^(?P<target>The kobold) takes",
            )],
        };
        let line = DamageLine::classify_with_defs(&table, "The kobold takes 12 points of damage!")
            .unwrap();
        assert_eq!(line.target.as_deref(), Some("The kobold"));
        assert_eq!(line.amount, 12);
    }
}
