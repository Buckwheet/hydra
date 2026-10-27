//! The facts a route's conditions are answered from.
//!
//! Recovered from `hydra_travel::facts::{walker_from, listed_names}`
//! (`0x1015240f8`, `0x101525b9c`).
//!
//! `walker_from` is the crate's implementation of the map's condition walker:
//! it gathers the flags, the profession, the citizenship, the society and the
//! skills a `cena_map::cond::Cond` can ask about. Every name below is a literal
//! from the pool at `0x10241c0b0`..`0x10241c520`.

use crate::GameState;

/// The condition names the walker answers, in pool order.
///
/// `stunned`, `hidden`, `invisible`, `mounted`, `urchin_access`,
/// `premium_account` are the six flags at `0x10241c4d9`; `kneeling`,
/// `sitting`, `prone` and `stunned` appear again in the posture list at
/// `0x10241c3c0`.
pub const FLAGS: [&str; 6] = [
    "stunned",
    "hidden",
    "invisible",
    "mounted",
    "urchin_access",
    "premium_account",
];

/// The postures a room's exit can require (`0x10241c044`).
pub const POSTURES: [&str; 3] = ["kneeling", "sitting", "prone"];

/// The professions that gate a guild door, from `0x10241c44c`.
pub const PROFESSIONS: [&str; 10] = [
    "Cleric",
    "Minor Elemental",
    "Major Elemental",
    "Ranger",
    "Wizard",
    "Empath",
    "Minor Mental",
    "Paladin",
    "Sorcerer",
    "Rogue",
];

/// The societies, from `0x10241c4ad`.
pub const SOCIETIES: [&str; 2] = ["Order of Voln", "Guardians of Sunfist"];

/// What a walker is, as far as the map's conditions care.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Walker {
    /// The flags that are set.
    pub flags: Vec<String>,
    /// The character's profession.
    pub profession: String,
    /// The character's level.
    pub level: u32,
    /// The character's citizenship.
    pub citizenship: Option<String>,
    /// The character's society.
    pub society: Option<String>,
    /// The skills the character has, with their ranks.
    pub skills: Vec<(String, u32)>,
    /// The spell ranks the character knows.
    pub spells: Vec<(u16, u32)>,
    /// What the character carries, by noun.
    pub carries: Vec<String>,
}

impl Walker {
    /// Is a flag set?
    pub fn is_set(&self, name: &str) -> bool {
        self.flags.iter().any(|flag| flag.eq_ignore_ascii_case(name))
    }

    /// Is the character at least `level`?
    pub fn level_at_least(&self, level: u32) -> bool {
        self.level >= level
    }

    /// Does the character have this profession?
    pub fn profession_is(&self, profession: &str) -> bool {
        self.profession.eq_ignore_ascii_case(profession)
    }

    /// The rank of one skill.
    pub fn rank(&self, skill: &str) -> u32 {
        self.skills
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(skill))
            .map(|(_, rank)| *rank)
            .unwrap_or(0)
    }

    /// Does the character know this spell?
    pub fn knows_spell(&self, id: u16) -> bool {
        self.spells.iter().any(|(spell, _)| *spell == id)
    }
}

/// Build the walker from the game state (`walker_from`, 11608 bytes at
/// `0x1015240f8`).
///
/// **Partly recovered**: the body reads the recovered status words for
/// [`FLAGS`] (`hydra_model::model::StatusInfo`-style lookups), the posture list,
/// the profession, the society (`Order of Voln` / `Guardians of Sunfist`), the
/// `Sigil of Resolve` / `Council of Light` titles, `Skill`/`Carries`/`Load` and
/// the `platinum` currency word. The exact per-flag plumbing is not recoverable
/// from the instruction stream; this build maps the state's own collections onto
/// the same names.
pub fn walker_from(state: &GameState) -> Walker {
    let mut walker = Walker::default();
    walker.profession = state.character.profession.clone();
    walker.level = state.character.level;
    walker.citizenship = state.character.info.get("citizenship").cloned();
    for (kind, skill) in &state.character.skills.skills {
        walker.skills.push((format!("{kind:?}"), skill.rank));
    }
    for name in &state.spells.names {
        if let Some(spell) = hydra_model::spells::spell_named(name) {
            walker.spells.push((spell.id, 1));
        }
    }
    walker
}

/// The names a list line offers (`listed_names`, 4780 bytes at
/// `0x101525b9c`).
///
/// **Partly recovered**: the body splits the line on `,` and ` and ` and trims
/// each part, which is the shape of the game's ` names you chose:` list
/// (`0x10241ef80`).
pub fn listed_names(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in line.split(',') {
        for piece in part.split(" and ") {
            let piece = piece.trim().trim_end_matches('.');
            if !piece.is_empty() {
                out.push(piece.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_and_posture_names_are_the_pool_literals() {
        assert_eq!(
            FLAGS,
            [
                "stunned",
                "hidden",
                "invisible",
                "mounted",
                "urchin_access",
                "premium_account"
            ]
        );
        assert_eq!(POSTURES, ["kneeling", "sitting", "prone"]);
        assert_eq!(SOCIETIES, ["Order of Voln", "Guardians of Sunfist"]);
        assert!(PROFESSIONS.contains(&"Rogue"));
        assert!(PROFESSIONS.contains(&"Sorcerer"));
    }

    #[test]
    fn a_walker_answers_its_own_questions() {
        let walker = Walker {
            flags: vec!["hidden".into()],
            profession: "Rogue".into(),
            level: 42,
            skills: vec![("Survival".into(), 30)],
            spells: vec![(612, 1)],
            ..Walker::default()
        };
        assert!(walker.is_set("HIDDEN"));
        assert!(!walker.is_set("stunned"));
        assert!(walker.level_at_least(42));
        assert!(!walker.level_at_least(43));
        assert!(walker.profession_is("rogue"));
        assert_eq!(walker.rank("survival"), 30);
        assert_eq!(walker.rank("Climbing"), 0);
        assert!(walker.knows_spell(612));
        assert!(!walker.knows_spell(704));
    }

    #[test]
    fn names_are_split_on_commas_and_ands() {
        assert_eq!(
            listed_names("Wehnimer's Landing, Icemule Trace and Ta'Illistim."),
            vec!["Wehnimer's Landing", "Icemule Trace", "Ta'Illistim"]
        );
    }
}
