//! Flare lines.
//!
//! `FlareLine::classify` (8520 B) is the largest combat classifier. Recovered
//! data: def family `"flare"` / `"flare_mirror"`, named groups `"attacker"`,
//! `"flare_weapon_link"`, `"spawns"`, `"target"`, `"reactive"`, `"barriers"`,
//! `"via_cast"`, `"released"`, `"aura"`; helper calls `Defs::first_match`,
//! `Defs::any_match`, `Defs::nock_weapon`, `AttackLine::classify`,
//! `DamageLine::classify`, `RedirectPrefix::classify`, `AmbushPrefix::classify`
//! and `target::link_in` / `bolded_link`.

/// A flare/flare-mirror line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlareLine {
    pub mirror: bool,
    pub attacker: Option<String>,
    pub target: Option<String>,
    pub weapon: Option<String>,
    pub via_cast: bool,
    pub released: bool,
    pub aura: bool,
    pub barriers: bool,
    pub reactive: bool,
    pub spawns: Option<String>,
}

impl FlareLine {
    /// @0x0000000101b8dbc0 (8520 B)
    pub fn classify(text: &str) -> Option<FlareLine> {
        let def = super::defs::defs().first_match(text)?;
        if !matches!(def.family.as_str(), "flare" | "flare_mirror") {
            return None;
        }
        Some(FlareLine {
            mirror: def.family == "flare_mirror",
            ..FlareLine::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_needs_the_def_table() {
        assert_eq!(FlareLine::classify("A burst of flame engulfs the kobold!"), None);
    }
}
