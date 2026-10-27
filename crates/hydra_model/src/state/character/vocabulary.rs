//! Small enumerations shared by the character readers.
//!
//! Recovered from the string pool: `ResourceType` (`Bounty Points`,
//! `Covert Arts Charges`, …), `Society` (`Order of Voln`, `Council of Light`,
//! `Guardians of Sunfist`), `Warcry`, `PsmCategory` (parsed by
//! `PsmCategory::parse_heading`, 572 B), `AccountType`, `DeathsSting`, `Che`.
//!
//! `Society` and `ResourceType` have recovered `Display` impls (40 B / 10628 B).
//!
//! `// UNKNOWN:` the exact variant sets of `Warcry`, `AccountType`,
//! `DeathsSting` and `Che`; only the names that appear as literals are listed.

/// A premium/society affiliation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Society {
    OrderOfVoln,
    CouncilOfLight,
    GuardiansOfSunfist,
}

impl Society {
    pub const ALL: [Society; 3] = [
        Society::OrderOfVoln,
        Society::CouncilOfLight,
        Society::GuardiansOfSunfist,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Society::OrderOfVoln => "Order of Voln",
            Society::CouncilOfLight => "Council of Light",
            Society::GuardiansOfSunfist => "Guardians of Sunfist",
        }
    }

    pub fn parse(text: &str) -> Option<Society> {
        let text = text.trim();
        Society::ALL
            .iter()
            .copied()
            .find(|s| s.name().eq_ignore_ascii_case(text))
    }
}

impl std::fmt::Display for Society {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A standing/society resource counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceType {
    BountyPoints,
    CovertArtsCharges,
    VolnFavor,
    SunfistRecognition,
    LightLore,
}

impl ResourceType {
    pub const ALL: [ResourceType; 5] = [
        ResourceType::BountyPoints,
        ResourceType::CovertArtsCharges,
        ResourceType::VolnFavor,
        ResourceType::SunfistRecognition,
        ResourceType::LightLore,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ResourceType::BountyPoints => "Bounty Points",
            ResourceType::CovertArtsCharges => "Covert Arts Charges",
            ResourceType::VolnFavor => "Voln Favor",
            ResourceType::SunfistRecognition => "Sunfist Recognition",
            ResourceType::LightLore => "Light Lore",
        }
    }

    /// @0x0000000101bb1? (880 B) — case-insensitive name lookup.
    pub fn parse(text: &str) -> Option<ResourceType> {
        let text = text.trim();
        ResourceType::ALL
            .iter()
            .copied()
            .find(|r| r.name().eq_ignore_ascii_case(text))
    }
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A `PSM` category heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PsmCategory {
    CombatManeuvers,
    ShieldSpecializations,
    ArmorSpecializations,
    Feats,
    WeaponTechniques,
    AscensionAbilities,
}

impl PsmCategory {
    pub const ALL: [PsmCategory; 6] = [
        PsmCategory::CombatManeuvers,
        PsmCategory::ShieldSpecializations,
        PsmCategory::ArmorSpecializations,
        PsmCategory::Feats,
        PsmCategory::WeaponTechniques,
        PsmCategory::AscensionAbilities,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PsmCategory::CombatManeuvers => "Combat Maneuvers",
            PsmCategory::ShieldSpecializations => "Shield Specializations",
            PsmCategory::ArmorSpecializations => "Armor Specializations",
            PsmCategory::Feats => "Feats",
            PsmCategory::WeaponTechniques => "Weapon Techniques",
            PsmCategory::AscensionAbilities => "Ascension Abilities",
        }
    }

    /// @0x0000000101b5? (572 B) — the heading text is `"<category>, <page>"`
    /// (`classify_header` splits on `", "`).
    pub fn parse_heading(text: &str) -> Option<PsmCategory> {
        let head = text.split(", ").next().unwrap_or(text).trim();
        PsmCategory::ALL
            .iter()
            .copied()
            .find(|c| c.key().eq_ignore_ascii_case(head))
    }
}

/// A warcry name.
///
/// `// UNKNOWN:` the variant set (`Warcry::parse` is 3588 B and its literals
/// are not reachable as a table); only free text is kept.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Warcry(pub String);

impl Warcry {
    /// @0x0000000101bb0244 (3588 B) — accepts any non-empty cry.
    pub fn parse(text: &str) -> Option<Warcry> {
        let text = text.trim();
        if text.is_empty() {
            None
        } else {
            Some(Warcry(text.to_string()))
        }
    }
}

/// `// UNKNOWN:` variant set (`AccountType` appears only as a serde field
/// visitor string).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountType {
    Free,
    Premium,
    Platinum,
    Shattered,
}

impl AccountType {
    pub fn key(self) -> &'static str {
        match self {
            AccountType::Free => "FREETRUE",
            AccountType::Premium => "premium",
            AccountType::Platinum => "platinum",
            AccountType::Shattered => "shattered",
        }
    }
}

/// `// UNKNOWN:` variant set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathsSting {
    Inactive,
    Active,
}

/// `// UNKNOWN:` variant set (`Che` appears only as a serde field visitor).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Che(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn society_names_round_trip_and_display() {
        for society in Society::ALL {
            assert_eq!(Society::parse(society.name()), Some(society));
            assert_eq!(society.to_string(), society.name());
        }
        assert_eq!(Society::parse("order of voln"), Some(Society::OrderOfVoln));
        assert_eq!(Society::parse("Order of Voln "), Some(Society::OrderOfVoln));
        assert_eq!(Society::parse("Order of the Sphere"), None);
    }

    #[test]
    fn resource_type_parse_and_display() {
        assert_eq!(
            ResourceType::parse("Bounty Points"),
            Some(ResourceType::BountyPoints)
        );
        assert_eq!(
            ResourceType::parse("covert arts charges"),
            Some(ResourceType::CovertArtsCharges)
        );
        assert_eq!(
            ResourceType::VolnFavor.to_string(),
            "Voln Favor"
        );
        assert_eq!(ResourceType::parse("mana"), None);
    }

    #[test]
    fn psm_heading_splits_on_comma() {
        assert_eq!(
            PsmCategory::parse_heading("Combat Maneuvers, page 2"),
            Some(PsmCategory::CombatManeuvers)
        );
        assert_eq!(
            PsmCategory::parse_heading("Feats, page 1"),
            Some(PsmCategory::Feats)
        );
        assert_eq!(PsmCategory::parse_heading("Spells, page 1"), None);
    }

    #[test]
    fn warcry_needs_text() {
        assert_eq!(Warcry::parse("  "), None);
        assert_eq!(Warcry::parse(" For the Glory "), Some(Warcry("For the Glory".into())));
    }
}
