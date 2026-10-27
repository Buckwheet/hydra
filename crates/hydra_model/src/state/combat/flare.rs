//! Flare lines.
//!
//! `FlareLine::classify` (8520 B) is the largest combat classifier. Recovered
//! data: def family `"flare"` / `"flare_mirror"`, named groups `"attacker"`,
//! `"flare_weapon_link"`, `"spawns"`, `"target"`, `"reactive"`, `"barriers"`,
//! `"via_cast"`, `"released"`, `"aura"`; helper calls `Defs::first_match`,
//! `Defs::any_match`, `Defs::nock_weapon`, `AttackLine::classify`,
//! `DamageLine::classify`, `RedirectPrefix::classify`, `AmbushPrefix::classify`
//! and `target::link_in` / `bolded_link`.
//!
//! The group values come from the hit's capture set
//! ([`crate::pattern::Groups`]); the `bool` fields are group *presence*.

use super::defs::{defs, Defs};

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
        FlareLine::classify_with_defs(defs(), text)
    }

    /// [`FlareLine::classify`] against a caller-supplied table.
    pub fn classify_with_defs(table: &Defs, text: &str) -> Option<FlareLine> {
        let (def, groups) = table.first_match_groups(text)?;
        if !matches!(def.family.as_str(), "flare" | "flare_mirror") {
            return None;
        }
        Some(FlareLine {
            mirror: def.family == "flare_mirror",
            attacker: groups.text("attacker"),
            target: groups.text("target"),
            weapon: groups.text("flare_weapon_link"),
            via_cast: groups.has("via_cast"),
            released: groups.has("released"),
            aura: groups.has("aura"),
            barriers: groups.has("barriers"),
            reactive: groups.has("reactive"),
            spawns: groups.text("spawns"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::defs::Def;
    use super::*;

    #[test]
    fn classify_needs_the_def_table() {
        assert_eq!(
            FlareLine::classify("A burst of flame engulfs the kobold!"),
            None
        );
    }

    fn table() -> Defs {
        Defs {
            defs: vec![
                Def::new(
                    "flare",
                    "fire",
                    "damaging",
                    1,
                    "",
                    "",
                    "^A burst of flame engulfs (?P<target>.+?), loosed by (?P<attacker>\\w+) with \
                     (?P<flare_weapon_link>\\w+)",
                ),
                Def::new(
                    "flare_mirror",
                    "mirror",
                    "damaging",
                    2,
                    "",
                    "",
                    "^(?P<reactive>Reactive) (?P<barriers>barriers) (?P<via_cast>cast) \
                     (?P<released>released) (?P<aura>aura) (?P<spawns>three)",
                ),
                Def::new("flare", "plain", "damaging", 3, "", "", "^A quiet glimmer"),
            ],
        }
    }

    #[test]
    fn the_recovered_groups_populate_the_line() {
        let table = table();
        let flare = FlareLine::classify_with_defs(
            &table,
            "A burst of flame engulfs the kobold, loosed by Kithar with runestaff",
        )
        .expect("a flare");
        assert!(!flare.mirror);
        assert_eq!(flare.target.as_deref(), Some("the kobold"));
        assert_eq!(flare.attacker.as_deref(), Some("Kithar"));
        assert_eq!(flare.weapon.as_deref(), Some("runestaff"));
        assert!(!flare.via_cast && !flare.released && !flare.aura);
        assert_eq!(flare.spawns, None);
    }

    #[test]
    fn mirror_flares_and_the_bool_groups() {
        let table = table();
        let flare = FlareLine::classify_with_defs(&table, "Reactive barriers cast released aura three")
            .expect("a mirror flare");
        assert!(flare.mirror);
        assert!(flare.reactive && flare.barriers && flare.via_cast);
        assert!(flare.released && flare.aura);
        assert_eq!(flare.spawns.as_deref(), Some("three"));
        // a hit without any groups is still a flare line
        let plain = FlareLine::classify_with_defs(&table, "A quiet glimmer").unwrap();
        assert_eq!(plain, FlareLine::default());
    }
}
