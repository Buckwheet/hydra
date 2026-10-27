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
//! The group *values* come from `regex`'s capture set
//! ([`crate::pattern::Groups`], the recovered `get_group_by_name`); the `bool`
//! fields (`aimed`) are group *presence*.
//!
//! `// UNKNOWN:` the exact spelling of the `redirect…` group names — the
//! literal run is truncated in the string pool. Both halves are read as
//! `redirect_from` / `redirect_to`.

use super::defs::{defs, Defs};

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
        AttackLine::classify_with_defs(defs(), text)
    }

    /// [`AttackLine::classify`] against a caller-supplied table.
    pub fn classify_with_defs(table: &Defs, text: &str) -> Option<AttackLine> {
        let (def, groups) = table.first_match_groups(text)?;
        if def.family != "attack" {
            return None;
        }
        Some(AttackLine {
            group: groups.text("group"),
            weapon: groups.text("weapon"),
            aimed: groups.has("aimed"),
            environment: groups.text("environment"),
            prefix: AttackPrefix {
                ambush: groups.text("ambush_prefix"),
                reaction: groups.text("reaction_prefix"),
                redirect_from: groups.text("redirect_from"),
                redirect_to: groups.text("redirect_to"),
            },
            target: groups.text("target"),
        })
    }
}

impl AttackPrefix {
    /// @0x0000000101b90e68 (1332 B)
    pub fn classify(text: &str) -> Option<AttackPrefix> {
        AttackPrefix::classify_with_defs(defs(), text)
    }

    /// [`AttackPrefix::classify`] against a caller-supplied table.
    pub fn classify_with_defs(table: &Defs, text: &str) -> Option<AttackPrefix> {
        let (def, groups) = table.first_match_groups(text)?;
        if !matches!(def.family.as_str(), "ambush_prefix" | "reaction_prefix") {
            return None;
        }
        let attacker = groups.text("attacker").unwrap_or_else(|| def.name.clone());
        Some(AttackPrefix {
            ambush: (def.family == "ambush_prefix").then(|| attacker.clone()),
            reaction: (def.family == "reaction_prefix").then_some(attacker),
            ..AttackPrefix::default()
        })
    }
}

/// @0x0000000101b9139c (648 B) — the weapon captured from a swing line.
pub fn swing_weapon(text: &str) -> Option<String> {
    swing_weapon_with_defs(defs(), text)
}

/// [`swing_weapon`] against a caller-supplied table.
pub fn swing_weapon_with_defs(table: &Defs, text: &str) -> Option<String> {
    let def = table.swing_weapon(text)?;
    Some(
        def.groups(text)
            .and_then(|groups| groups.text("weapon"))
            .unwrap_or_else(|| def.name.clone()),
    )
}

/// @0x0000000101b919d8 (692 B) — is this an attack line with no attacker
/// (e.g. an environmental attack)?
pub fn attackerless_line(text: &str) -> bool {
    attackerless_line_with_defs(defs(), text)
}

/// The def families `attackerless_line` tests (the recovered concatenated
/// literal `attack` / `group` / `weapon` / `aimed` / `environment` /
/// `ambush_prefix` / `reaction_prefix` / `redirect…`).
pub const ATTACKERLESS_FAMILIES: [&str; 7] = [
    "attack",
    "group",
    "weapon",
    "aimed",
    "environment",
    "ambush_prefix",
    "reaction_prefix",
];

/// [`attackerless_line`] against a caller-supplied table.
pub fn attackerless_line_with_defs(table: &Defs, text: &str) -> bool {
    table.defs.iter().any(|def| {
        ATTACKERLESS_FAMILIES.contains(&def.family.as_str()) && def.matcher.is_match(text)
    })
}

/// @0x0000000101b91c8c (364 B) — the location named by a coup-de-grace line.
pub fn coup_kill_location(text: &str) -> Option<String> {
    coup_kill_location_with_defs(defs(), text)
}

/// [`coup_kill_location`] against a caller-supplied table.
pub fn coup_kill_location_with_defs(table: &Defs, text: &str) -> Option<String> {
    let def = table.family("coup_kill")?;
    def.matcher.is_match(text).then(|| def.name.clone())
}

#[cfg(test)]
mod tests {
    use super::super::defs::Def;
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
        let defs = Defs {
            defs: vec![Def::new(
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
        let prefix = AttackPrefix::classify_with_defs(&defs, text).unwrap();
        assert_eq!(prefix.ambush.as_deref(), Some("leaps_from_hiding"));
        assert_eq!(prefix.reaction, None);
    }

    fn table() -> Defs {
        Defs {
            defs: vec![
                Def::new(
                    "attack",
                    "claw",
                    "damaging",
                    1,
                    "",
                    "",
                    "^(?P<group>the .+?) (?P<ambush_prefix>leaps from hiding) and swings its \
                     (?P<weapon>\\w+) at (?P<target>.+?)!$",
                ),
                Def::new("attack", "plain", "damaging", 2, "", "", "^The wall slams into you"),
                Def::new(
                    "attack",
                    "aimed_shot",
                    "damaging",
                    3,
                    "",
                    "",
                    "^An arrow (?P<aimed>slams) into (?P<target>\\w+)",
                ),
                Def::new(
                    "attack",
                    "redirected",
                    "damaging",
                    4,
                    "",
                    "",
                    "^Redirected from (?P<redirect_from>\\w+) to (?P<redirect_to>\\w+)",
                ),
                Def::new(
                    "swing",
                    "swing",
                    "damaging",
                    5,
                    "",
                    "",
                    "^You swing your (?P<weapon>\\w+)",
                ),
                Def::new("coup_kill", "neck", "damaging", 6, "", "", "^You coup de grace"),
            ],
        }
    }

    #[test]
    fn classify_populates_every_recovered_group() {
        let table = table();
        let line = AttackLine::classify_with_defs(
            &table,
            "the kobold leaps from hiding and swings its sword at you!",
        )
        .expect("an attack line");
        assert_eq!(line.group.as_deref(), Some("the kobold"));
        assert_eq!(line.weapon.as_deref(), Some("sword"));
        assert_eq!(line.target.as_deref(), Some("you"));
        assert!(!line.aimed);
        assert_eq!(line.prefix.ambush.as_deref(), Some("leaps from hiding"));
        assert_eq!(line.prefix.reaction, None);
        assert_eq!(line.prefix.redirect_from, None);
        assert_eq!(line.prefix.redirect_to, None);
    }

    #[test]
    fn a_hit_without_groups_leaves_the_fields_empty() {
        let table = table();
        let line = AttackLine::classify_with_defs(&table, "The wall slams into you").unwrap();
        assert_eq!(line, AttackLine::default());
        assert!(attackerless_line_with_defs(&table, "The wall slams into you"));
    }

    #[test]
    fn aimed_and_redirect_groups_are_read() {
        let table = table();
        let line = AttackLine::classify_with_defs(&table, "An arrow slams into kobold").unwrap();
        assert!(line.aimed, "the `aimed` group took part in the match");
        assert_eq!(line.target.as_deref(), Some("kobold"));
        let line =
            AttackLine::classify_with_defs(&table, "Redirected from kobold to goblin").unwrap();
        assert_eq!(line.prefix.redirect_from.as_deref(), Some("kobold"));
        assert_eq!(line.prefix.redirect_to.as_deref(), Some("goblin"));
    }

    #[test]
    fn swing_weapon_reads_the_weapon_group() {
        let table = table();
        assert_eq!(
            swing_weapon_with_defs(&table, "You swing your broadsword").as_deref(),
            Some("broadsword")
        );
        assert_eq!(
            coup_kill_location_with_defs(&table, "You coup de grace the kobold").as_deref(),
            Some("neck")
        );
    }
}
