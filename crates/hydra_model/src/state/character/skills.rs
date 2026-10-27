//! The skill table (`SKILLS` output).
//!
//! `SkillKind::key` (2656 B at `0x10164d9c`) is a long `match` returning the
//! display name; the ordered name table at `0x102420160` is
//!
//! ```text
//! Edged Weapons / Blunt Weapons / Two-Handed Weapons / Ranged Weapons /
//! Thrown Weapons / Polearm Weapons / Ambush / Multi Opponent Combat /
//! Dodging / Arcane Symbols / Magic Item Use / Spell Aiming / Harness Power /
//! Elemental Mana Control / Mental Mana Control / Spirit Mana Control /
//! Elemental Lore - Air / Elemental Lore - Earth / Elemental Lore - Fire /
//! Elemental Lore - Water / Spiritual Lore - Blessings /
//! Spiritual Lore - Religion / Spiritual Lore - Summoning /
//! Sorcerous Lore - Demonology / Sorcerous Lore - Necromancy /
//! Mental Lore - Divination / Mental Lore - Manipulation /
//! Mental Lore - Telepathy / Mental Lore - Transference /
//! Mental Lore - Transformation / Disarming Traps / Picking Locks /
//! Stalking and Hiding / Perception / First Aid / Trading / Pickpocketing
//! ```
//!
//! `Armor Use`, `Shield Use`, `Combat Maneuvers`, `Physical Fitness`,
//! `Climbing`, `Swimming`, `Brawling` and `Survival` also appear as literals
//! inside `SkillKind::key` and are included here.
//!
//! Also recovered from the string pool: `is_table_header` matches
//! `"), your current skill bonuses and ranks"` and `" (at level "`, and
//! `SkillSet` (a `BTreeMap<SkillKind, Skill>` — the serde `expecting` text
//! `struct SkillSet` survives).

use std::collections::BTreeMap;
use serde::Deserialize;

macro_rules! skill_kinds {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// A trainable skill.
        ///
        /// `// UNKNOWN:` the variant identifiers; the display names, their
        /// order and the `key()` mapping are recovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
        pub enum SkillKind {
            $($variant),*
        }

        impl SkillKind {
            /// Every skill, in the recovered `key()` table order.
            pub const ALL: &'static [SkillKind] = &[$(SkillKind::$variant),*];

            /// The display name (`SkillKind::key`, 2656 B).
            pub fn key(self) -> &'static str {
                match self {
                    $(SkillKind::$variant => $name),*
                }
            }
        }
    };
}

skill_kinds! {
    EdgedWeapons => "Edged Weapons",
    BluntWeapons => "Blunt Weapons",
    TwoHandedWeapons => "Two-Handed Weapons",
    RangedWeapons => "Ranged Weapons",
    ThrownWeapons => "Thrown Weapons",
    PolearmWeapons => "Polearm Weapons",
    Ambush => "Ambush",
    MultiOpponentCombat => "Multi Opponent Combat",
    Dodging => "Dodging",
    ArcaneSymbols => "Arcane Symbols",
    MagicItemUse => "Magic Item Use",
    SpellAiming => "Spell Aiming",
    HarnessPower => "Harness Power",
    ElementalManaControl => "Elemental Mana Control",
    MentalManaControl => "Mental Mana Control",
    SpiritManaControl => "Spirit Mana Control",
    ElementalLoreAir => "Elemental Lore - Air",
    ElementalLoreEarth => "Elemental Lore - Earth",
    ElementalLoreFire => "Elemental Lore - Fire",
    ElementalLoreWater => "Elemental Lore - Water",
    SpiritualLoreBlessings => "Spiritual Lore - Blessings",
    SpiritualLoreReligion => "Spiritual Lore - Religion",
    SpiritualLoreSummoning => "Spiritual Lore - Summoning",
    SorcerousLoreDemonology => "Sorcerous Lore - Demonology",
    SorcerousLoreNecromancy => "Sorcerous Lore - Necromancy",
    MentalLoreDivination => "Mental Lore - Divination",
    MentalLoreManipulation => "Mental Lore - Manipulation",
    MentalLoreTelepathy => "Mental Lore - Telepathy",
    MentalLoreTransference => "Mental Lore - Transference",
    MentalLoreTransformation => "Mental Lore - Transformation",
    DisarmingTraps => "Disarming Traps",
    PickingLocks => "Picking Locks",
    StalkingAndHiding => "Stalking and Hiding",
    Perception => "Perception",
    FirstAid => "First Aid",
    Trading => "Trading",
    Pickpocketing => "Pickpocketing",
    ArmorUse => "Armor Use",
    ShieldUse => "Shield Use",
    CombatManeuvers => "Combat Maneuvers",
    PhysicalFitness => "Physical Fitness",
    Climbing => "Climbing",
    Swimming => "Swimming",
    TwoWeaponCombat => "Two Weapon Combat",
    Brawling => "Brawling",
    Survival => "Survival",
}

impl SkillKind {
    /// @0x0000000101b657fc (180 B)
    ///
    /// `// UNKNOWN:` the 180-byte body compares a small number of literals; it
    /// is modelled as a case-insensitive lookup over [`SkillKind::key`], which
    /// is the only thing a caller can rely on.
    pub fn parse(text: &str) -> Option<SkillKind> {
        let text = text.trim();
        SkillKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.key().eq_ignore_ascii_case(text))
    }
}

/// One skill's rank/bonus row.
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Skill {
    pub rank: u32,
    pub bonus: i32,
}

/// The skill table, keyed by [`SkillKind`].
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SkillSet {
    pub skills: BTreeMap<SkillKind, Skill>,
    /// The training circle printed in the header (`" (at level N)"`).
    pub level: Option<u32>,
}

impl SkillSet {
    /// @0x0000000101b648e0 (116 B)
    pub fn get(&self, kind: SkillKind) -> Option<&Skill> {
        self.skills.get(&kind)
    }

    /// @0x0000000101b64954 (128 B) — ranks for one skill (0 when untrained).
    pub fn ranks(&self, kind: SkillKind) -> u32 {
        self.get(kind).map(|s| s.rank).unwrap_or(0)
    }

    /// @0x0000000101b649d4 (228 B) — the training circle.
    ///
    /// `// UNKNOWN:` whether the binary derives it from `level` or from the
    /// rank sum; the recovered body only reads the set, so the stored level is
    /// returned.
    pub fn circle(&self) -> u32 {
        self.level.unwrap_or(0)
    }

    /// @0x0000000101b64ab8 (740 B) — replace one skill row, returning the old.
    pub fn replace(&mut self, kind: SkillKind, skill: Skill) -> Option<Skill> {
        self.skills.insert(kind, skill)
    }
}

/// @0x0000000101b643e8 (492 B) — `"), your current skill bonuses and ranks"`
/// or `" (at level "`.
pub fn is_table_header(line: &str) -> bool {
    line.contains("), your current skill bonuses and ranks") || line.contains(" (at level ")
}

/// @0x0000000101b64294 (340 B) — the end-of-table marker.
///
/// `// UNKNOWN:` the exact text; the recovered body tests the line against the
/// literals used by `read_table`, which are all header/footer strings.
pub fn is_table_end(line: &str) -> bool {
    line.trim().is_empty() || line.contains("Training Points:") || line.contains("Physical Training")
}

/// @0x0000000101b645d4 (780 B) — `Training Points: N ...`
///
/// `// UNKNOWN:` the return shape (a small tuple in the binary); modelled as
/// the remaining training points.
pub fn training_footer(line: &str) -> Option<u32> {
    let rest = line
        .trim()
        .strip_prefix("Training Points:")
        .or_else(|| line.trim().strip_prefix("Training points:"))?;
    rest.trim()
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
}

/// One classified line of the `SKILLS` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillLine {
    pub kind: Option<SkillKind>,
    pub rank: u32,
    pub bonus: i32,
}

impl SkillLine {
    /// @0x0000000101b65c14 (1908 B) — name, ranks and bonus from one row.
    pub fn classify(line: &str) -> Option<SkillLine> {
        let line = line.trim_end();
        if line.is_empty() {
            return None;
        }
        // A row is "<name>....<rank>  <bonus>" (dots are the game's padding).
        let name_end = line.find(|c: char| c.is_ascii_digit()).unwrap_or(line.len());
        let name = line[..name_end].trim_end_matches(|c: char| !c.is_alphabetic());
        let kind = SkillKind::parse(name);
        let mut numbers = line[name_end..]
            .split_whitespace()
            .filter_map(|n| n.parse::<i32>().ok());
        let rank = numbers.next().unwrap_or(0).max(0) as u32;
        let bonus = numbers.next().unwrap_or(0);
        if kind.is_none() && rank == 0 && bonus == 0 {
            return None;
        }
        Some(SkillLine { kind, rank, bonus })
    }

    /// @0x0000000101b658b0 (868 B) — the same row from styled runs (bold names
    /// are the ones with a bonus). The rebuild loses styling, so this forwards
    /// to [`SkillLine::classify`].
    pub fn classify_with_bold(line: &str, _bold: bool) -> Option<SkillLine> {
        SkillLine::classify(line)
    }
}

/// @0x0000000101b63ef8 (924 B) — read the whole table.
///
/// Returns the number of rows stored.
pub fn read_table(set: &mut SkillSet, lines: &[&str]) -> usize {
    let mut stored = 0;
    for (index, line) in lines.iter().enumerate() {
        if is_table_header(line) {
            if let Some(level) = level_from_header(line) {
                set.level = Some(level);
            }
            continue;
        }
        if is_table_end(line) {
            break;
        }
        if let Some(parsed) = SkillLine::classify(line) {
            if let Some(kind) = parsed.kind {
                set.skills.insert(
                    kind,
                    Skill {
                        rank: parsed.rank,
                        bonus: parsed.bonus,
                    },
                );
                stored += 1;
            }
        }
        let _ = index;
    }
    stored
}

fn level_from_header(line: &str) -> Option<u32> {
    let (_, rest) = line.split_once(" (at level ")?;
    rest.split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|n| n.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_table_has_the_recovered_order() {
        assert_eq!(SkillKind::ALL.len(), 46);
        assert_eq!(SkillKind::ALL[0].key(), "Edged Weapons");
        assert_eq!(SkillKind::ALL[1].key(), "Blunt Weapons");
        assert_eq!(SkillKind::ALL[2].key(), "Two-Handed Weapons");
        assert_eq!(SkillKind::ALL[6].key(), "Ambush");
        assert_eq!(SkillKind::ALL[7].key(), "Multi Opponent Combat");
        assert_eq!(SkillKind::ALL[16].key(), "Elemental Lore - Air");
        assert_eq!(SkillKind::ALL[24].key(), "Sorcerous Lore - Necromancy");
        assert_eq!(SkillKind::ALL[29].key(), "Mental Lore - Transformation");
        assert_eq!(SkillKind::ALL[36].key(), "Pickpocketing");
        assert_eq!(SkillKind::ALL[39].key(), "Combat Maneuvers");
        assert_eq!(SkillKind::ALL[44].key(), "Brawling");
        assert_eq!(SkillKind::ArmorUse.key(), "Armor Use");
    }

    #[test]
    fn parse_is_case_insensitive_and_round_trips() {
        for kind in SkillKind::ALL {
            assert_eq!(SkillKind::parse(kind.key()), Some(*kind));
        }
        assert_eq!(
            SkillKind::parse("elemental lore - air"),
            Some(SkillKind::ElementalLoreAir)
        );
        assert_eq!(SkillKind::parse("Basket Weaving"), None);
    }

    #[test]
    fn skill_lines_split_name_rank_and_bonus() {
        let line = SkillLine::classify("Two Weapon Combat....................|   10   5").unwrap();
        assert_eq!(line.kind, Some(SkillKind::TwoWeaponCombat));
        assert_eq!(line.rank, 10);
        assert_eq!(line.bonus, 5);
    }

    #[test]
    fn skill_line_of_a_known_skill() {
        let line = SkillLine::classify("Perception...........................|   32   15").unwrap();
        assert_eq!(line.kind, Some(SkillKind::Perception));
        assert_eq!(line.rank, 32);
        assert_eq!(line.bonus, 15);
    }

    #[test]
    fn header_detection_uses_the_recovered_texts() {
        assert!(is_table_header("Kithar (at level 42), your current skill bonuses and ranks"));
        assert!(!is_table_header("Edged Weapons"));
    }

    #[test]
    fn set_accessors() {
        let mut set = SkillSet::default();
        assert_eq!(set.ranks(SkillKind::Dodging), 0);
        assert_eq!(set.get(SkillKind::Dodging), None);
        assert_eq!(set.circle(), 0);
        let old = set.replace(
            SkillKind::Dodging,
            Skill {
                rank: 12,
                bonus: 24,
            },
        );
        assert_eq!(old, None);
        assert_eq!(set.ranks(SkillKind::Dodging), 12);
        set.level = Some(20);
        assert_eq!(set.circle(), 20);
    }

    #[test]
    fn training_footer_reads_points() {
        assert_eq!(training_footer("Training Points: 7"), Some(7));
        assert_eq!(training_footer("nothing"), None);
    }

    #[test]
    fn read_table_stores_rows_and_level() {
        let mut set = SkillSet::default();
        let lines = [
            "Kithar (at level 42), your current skill bonuses and ranks",
            "Perception...........................|   32   15",
            "Dodging..............................|    8    4",
            "Training Points: 3",
        ];
        assert_eq!(read_table(&mut set, &lines), 2);
        assert_eq!(set.level, Some(42));
        assert_eq!(set.ranks(SkillKind::Perception), 32);
        assert_eq!(set.ranks(SkillKind::Dodging), 8);
    }
}
