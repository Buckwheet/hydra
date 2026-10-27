//! `AttackLine` and its prefixes.
//!
//! Recovered:
//! * `AttackLine::classify` (4448 B): def family `"attack"`, named groups
//!   `group`, `weapon`, `aimed`, `environment`, `ambush_prefix`,
//!   `reaction_prefix`, `redirect…`, `target` (the group names come from the
//!   concatenated literal `attackgroupweaponaimedenvironmentambush_prefix
//!   reaction_prefixredir…`), then `target::link_in` / `bolded_link`.
//! * `AmbushPrefix::classify` (1332 B): family `"ambush_prefix"` /
//!   `"reaction_prefix"`, group `"attacker"`.
//! * `RedirectPrefix::classify` (948 B): family `"redirect_prefix"`, groups
//!   `"interceptor"`, `"intended"`.
//! * `swing_weapon` (648 B): `Defs::swing_weapon` + group `"attacker"`.
//! * `attackerless_line` (692 B): the def-family list literal
//!   `attack/group/weapon/aimed/environment/ambush_prefix/reaction_prefix/
//!   redirect…` with `Runs::plain`.
//! * `coup_kill_location` (364 B): def family `"coup_kill"`.
//!
//! `// UNKNOWN:` named captures — the std-only engine has no capture support,
//! so the group values are reported as `None`; the shape of the record is
//! recovered from the group names.

/// The attack prefixes that can wrap an attack line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttackPrefix {
    pub ambush: Option<String>,
    pub reaction: Option<String>,
    pub redirect_from: Option<String>,
    pub redirect_to: Option<String>,
}

/// One attack line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttackLine {
    pub group: Option<String>,
    pub weapon: Option<String>,
    pub aimed: bool,
    pub environment: Option<String>,
    pub prefix: AttackPrefix,
    pub target: Option<String>,
}

impl AttackLine {
    /// @0x0000000101b8fd08 (4448 B)
    pub fn classify(text: &str) -> Option<AttackLine> {
        let def = super::defs::defs().first_match(text)?;
        if def.family != "attack" {
            return None;
        }
        Some(AttackLine {
            group: None,
            weapon: None,
            aimed: false,
            environment: None,
            prefix: AttackPrefix::default(),
            target: None,
        })
    }
}

impl AttackPrefix {
    /// @0x0000000101b90e68 (1332 B)
    pub fn classify(text: &str) -> Option<AttackPrefix> {
        let def = super::defs::defs().first_match(text)?;
        if !matches!(def.family.as_str(), "ambush_prefix" | "reaction_prefix") {
            return None;
        }
        Some(AttackPrefix {
            ambush: (def.family == "ambush_prefix").then(|| def.name.clone()),
            reaction: (def.family == "reaction_prefix").then(|| def.name.clone()),
            ..AttackPrefix::default()
        })
    }
}

/// @0x0000000101b9139c (648 B) — the weapon captured from a swing line.
pub fn swing_weapon(text: &str) -> Option<String> {
    super::defs::defs()
        .swing_weapon(text)
        .map(|def| def.name.clone())
}

/// @0x0000000101b919d8 (692 B) — is this an attack line with no attacker
/// (e.g. an environmental attack)?
pub fn attackerless_line(text: &str) -> bool {
    super::defs::defs()
        .family("attack")
        .map(|def| def.matcher.is_match(text))
        .unwrap_or(false)
}

/// @0x0000000101b91c8c (364 B) — the location named by a coup-de-grace line.
pub fn coup_kill_location(text: &str) -> Option<String> {
    let def = super::defs::defs().family("coup_kill")?;
    def.matcher.is_match(text).then(|| def.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_needs_definitions() {
        assert_eq!(AttackLine::classify("The kobold swings at you!"), None);
        assert_eq!(AttackPrefix::classify("Caught unaware"), None);
        assert_eq!(swing_weapon("You swing your sword"), None);
        assert_eq!(coup_kill_location("You coup de grace the kobold"), None);
        assert!(!attackerless_line("The wall slams into you"));
    }

    #[test]
    fn prefixes_record_their_family() {
        let defs = crate::state::combat::defs::Defs {
            defs: vec![crate::state::combat::defs::Def::new(
                "ambush_prefix",
                "leaps_from_hiding",
                "damaging",
                1,
                "",
                "",
                "^It leaps from hiding",
            )],
        };
        let text = "It leaps from hiding and attacks!";
        let def = defs.first_match(text).unwrap();
        assert_eq!(def.family, "ambush_prefix");
        assert_eq!(def.name, "leaps_from_hiding");
    }
}
