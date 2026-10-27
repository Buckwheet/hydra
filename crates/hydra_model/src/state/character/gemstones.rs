//! The `GEMSTONES`/`LORE` inventory report.
//!
//! Recovered literals from `Gemstones::read_chunk` (4548 B): `" (Rank "`,
//! `" of "`, `"(greater binding)"`, `"(lesser binding)"`, `"(equipped)"`.
//! Recovered type names (`Property`, `Gemstone`, `Gemstones`) come from the
//! serde `expecting` texts; `Binding` likewise.

use serde::Deserialize;

/// How a gemstone is bound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum Binding {
    #[default]
    None,
    Lesser,
    Greater,
}

/// A gemstone property (`+2 Strength`, …).
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Property {
    pub name: String,
    pub amount: i32,
}

/// One gemstone.
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Gemstone {
    pub name: String,
    pub rank: u32,
    pub binding: Binding,
    pub equipped: bool,
    pub properties: Vec<Property>,
    pub charges: Option<u32>,
}

/// All gemstones in inventory.
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Gemstones {
    pub items: Vec<Gemstone>,
}

impl Gemstones {
    /// @0x0000000101ba27b8 (4548 B)
    pub fn read_chunk(&mut self, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        let mut gem = Gemstone::default();
        let name = match line.split_once(" (Rank ") {
            Some((name, rest)) => {
                gem.rank = rest
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                name.trim().to_string()
            }
            None => line.to_string(),
        };
        gem.name = name;
        gem.binding = if line.contains("(greater binding)") {
            Binding::Greater
        } else if line.contains("(lesser binding)") {
            Binding::Lesser
        } else {
            Binding::None
        };
        gem.equipped = line.contains("(equipped)");
        if let Some((_, rest)) = line.split_once(" of ") {
            if let Ok(charges) = rest
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap_or("")
                .parse()
            {
                gem.charges = Some(charges);
            }
        }
        if self.items.iter().any(|g| g.name == gem.name && g.rank == gem.rank) {
            return false;
        }
        self.items.push(gem);
        true
    }

    /// @0x0000000101ba397c (2504 B) — properties of the equipped gemstones.
    pub fn equipped_properties(&self) -> Vec<&Property> {
        self.items
            .iter()
            .filter(|g| g.equipped)
            .flat_map(|g| g.properties.iter())
            .collect()
    }

    pub fn equipped(&self) -> impl Iterator<Item = &Gemstone> {
        self.items.iter().filter(|g| g.equipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_parses_rank_and_binding() {
        let mut gems = Gemstones::default();
        assert!(gems.read_chunk("a fiery red ruby (Rank 4) (greater binding) (equipped)"));
        let gem = &gems.items[0];
        assert_eq!(gem.name, "a fiery red ruby");
        assert_eq!(gem.rank, 4);
        assert_eq!(gem.binding, Binding::Greater);
        assert!(gem.equipped);
    }

    #[test]
    fn duplicate_chunks_are_ignored() {
        let mut gems = Gemstones::default();
        assert!(gems.read_chunk("a blue sapphire (Rank 1)"));
        assert!(!gems.read_chunk("a blue sapphire (Rank 1)"));
        assert_eq!(gems.items.len(), 1);
    }

    #[test]
    fn lesser_binding_is_recognised() {
        let mut gems = Gemstones::default();
        gems.read_chunk("a plain stone (Rank 2) (lesser binding)");
        assert_eq!(gems.items[0].binding, Binding::Lesser);
    }

    #[test]
    fn equipped_properties_only_from_equipped_items() {
        let mut gems = Gemstones::default();
        gems.items.push(Gemstone {
            name: "a".into(),
            equipped: true,
            properties: vec![Property {
                name: "Strength".into(),
                amount: 2,
            }],
            ..Default::default()
        });
        gems.items.push(Gemstone {
            name: "b".into(),
            equipped: false,
            properties: vec![Property {
                name: "Logic".into(),
                amount: 5,
            }],
            ..Default::default()
        });
        let props = gems.equipped_properties();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "Strength");
        assert_eq!(gems.equipped().count(), 1);
    }
}
