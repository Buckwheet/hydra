//! The bestiary: what the game has told us about creatures.
//!
//! Recovered from `bestiary::{asked, answered, room_uid}` at `0x101a7aaf8`,
//! `0x101a7f850`, `0x101a800b4`.
//!
//! **ESTABLISHED** texts — `asked` and `answered` are the two halves of the
//! `.bestiary` command, and every literal they format was read out of
//! `__TEXT` (length-prefixed pieces decoded):
//!
//! ```text
//! `{}` is not a level: 29, or a band, 25-33.              0x103e7c570
//! bestiary level {}                                      0x103e7c59c
//! bestiary area {}                                       0x103e7c61f
//! bestiary family {}                                     0x103e7c5b5
//! map {}                                                 0x103e7c5ae
//! bestiary {}                                            0x103e2b408
//!   Type: {}        0x103e7c6bc      ASG {}             0x103e7c5ed
//!   HP: {}          0x103e7c6c7      Defense: {}        0x103e7c5f4
//!   Boss{}          0x103e7c6d0      TD: {}             0x103e7c600
//!   {} rooms        0x103e7c630      Defensive abilities: {}
//!   AS {}   0x103e7c646   CS {}      0x103e7c607
//! go2 u{}          0x103e7c63e      skin: {}           0x103e7c6d9
//!   {}             0x103e63a80      Treasure: {}       0x103e7c6e2
//! {}:{}  0x103e2c19d   {} ({})      0x103e28dbe
//! Found in {}:        0x103e7c69e   Found around room {}:   0x103e7c66a
//! The {} family:      0x103e7c6ac   Level {}:               0x103e7c693
//! Creatures like {}:  0x103e7c656   Levels {} to {}:        0x103e7c681
//! "The bestiary has no creature like {}."                   0x103e2b39c
//! ```
//!
//! **ESTABLISHED** `room_uid` (`0x101a800b4`): the room's uid, computed from
//! the room's title and description; the recovered code hashes strings with the
//! standard hasher and formats `"{}:{}"` (`uid:title`), which is exactly the
//! `{}_{}`-style key the notes use.
//!
//! **UNKNOWN**: which creature fields the game actually sends (the big match
//! arms are inlined), and the exact hash `room_uid` uses.  We expose the
//! documented structure and a stable FNV-1a hash so a uid does not change
//! between runs.

/// A band of creature levels, e.g. `25-33`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub low: u32,
    pub high: u32,
}

impl Band {
    /// The recovered text for a band: `{}` `-` `{}`.
    pub fn text(&self) -> String {
        format!("{}-{}", self.low, self.high)
    }
}

/// What a `.bestiary` argument asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// one level, `25`
    Level(u32),
    /// a band, `25-33`
    Band(Band),
    /// a creature name
    Creature(String),
    /// everything in an area
    Area(String),
    /// everything in a family
    Family(String),
}

/// The recovered refusal for an argument that is not a level or a band.
pub const NOT_A_LEVEL: &str = "`{}` is not a level: 29, or a band, 25-33.";

impl Asked {
    /// Parse the argument of a `.bestiary` command.
    ///
    /// **ESTABLISHED**: a bare number is a level, `a-b` is a band (`25-33`,
    /// the literal in [`NOT_A_LEVEL`]), and anything else is a name.
    pub fn parse(word: &str) -> Result<Asked, String> {
        let word = word.trim();
        if word.is_empty() {
            return Err(NOT_A_LEVEL.replace("{}", word));
        }
        if let Ok(level) = word.parse::<u32>() {
            if level == 0 || level > 200 {
                return Err(NOT_A_LEVEL.replace("{}", word));
            }
            return Ok(Asked::Level(level));
        }
        if let Some((low, high)) = word.split_once('-') {
            match (low.trim().parse::<u32>(), high.trim().parse::<u32>()) {
                (Ok(low), Ok(high)) if low >= 1 && high >= low && high <= 200 => {
                    return Ok(Asked::Band(Band { low, high }))
                }
                _ => return Err(NOT_A_LEVEL.replace("{}", word)),
            }
        }
        Ok(Asked::Creature(word.to_string()))
    }
}

/// One creature, as much as the game tells us.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Creature {
    /// the creature's name, as the game shows it
    pub name: String,
    /// its level, if known
    pub level: Option<u32>,
    /// its family (`kobold`, `giant`, …)
    pub family: String,
    /// its type
    pub kind: String,
    /// hit points
    pub hp: Option<u32>,
    /// whether it is a boss
    pub boss: bool,
    /// attack strength
    pub as_: Option<u32>,
    /// cast strength
    pub cs: Option<u32>,
    /// target defence
    pub td: Option<u32>,
    /// the skin it yields
    pub skin: String,
    /// what it treasures
    pub treasure: String,
    /// the rooms it is found in
    pub rooms: Vec<String>,
}

impl Creature {
    /// A creature with a name.
    pub fn new(name: &str) -> Self {
        Creature {
            name: name.to_string(),
            ..Creature::default()
        }
    }
}

/// The bestiary itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bestiary {
    /// the creatures, in insertion order
    pub creatures: Vec<Creature>,
}

impl Bestiary {
    /// An empty bestiary.
    pub fn new() -> Self {
        Bestiary::default()
    }

    /// Add a creature (replacing one with the same name).
    pub fn put(&mut self, creature: Creature) {
        match self
            .creatures
            .iter_mut()
            .find(|c| c.name.eq_ignore_ascii_case(&creature.name))
        {
            Some(slot) => *slot = creature,
            None => self.creatures.push(creature),
        }
    }

    /// The creatures whose name matches `like` (case insensitive substring).
    pub fn like(&self, like: &str) -> Vec<&Creature> {
        let needle = like.to_lowercase();
        self.creatures
            .iter()
            .filter(|c| c.name.to_lowercase().contains(&needle))
            .collect()
    }

    /// The creatures in a level or a band.
    pub fn at(&self, asked: &Asked) -> Vec<&Creature> {
        match asked {
            Asked::Level(level) => self
                .creatures
                .iter()
                .filter(|c| c.level == Some(*level))
                .collect(),
            Asked::Band(band) => self
                .creatures
                .iter()
                .filter(|c| {
                    c.level
                        .map(|l| l >= band.low && l <= band.high)
                        .unwrap_or(false)
                })
                .collect(),
            Asked::Creature(name) => self.like(name),
            Asked::Area(area) => self
                .creatures
                .iter()
                .filter(|c| c.rooms.iter().any(|r| r.eq_ignore_ascii_case(area)))
                .collect(),
            Asked::Family(family) => self
                .creatures
                .iter()
                .filter(|c| c.family.eq_ignore_ascii_case(family))
                .collect(),
        }
    }
}

/// The answer to a `.bestiary` question.
///
/// **ESTABLISHED** texts: every arm formats one of the recovered headers and
/// then one `  <field>: <value>` line per known field.
pub fn answered(asked: &Asked, hits: &[&Creature]) -> Vec<String> {
    let mut out = Vec::new();
    match asked {
        Asked::Level(level) => {
            out.push(format!("Level {level}:"));
            for creature in hits {
                out.push(format!("  {}", creature.name));
            }
        }
        Asked::Band(band) => {
            out.push(format!("Levels {} to {}:", band.low, band.high));
            for creature in hits {
                out.push(format!("  {}", creature.name));
            }
        }
        Asked::Creature(name) => {
            if hits.is_empty() {
                out.push(format!("The bestiary has no creature like {name}."));
                return out;
            }
            for creature in hits {
                out.push(asked_header(creature));
                out.extend(detail(creature));
            }
        }
        Asked::Family(family) => {
            out.push(format!("The {family} family:"));
            for creature in hits {
                out.push(format!("  {}", creature.name));
            }
        }
        Asked::Area(area) => {
            out.push(format!("Found in {area}:"));
            for creature in hits {
                out.push(format!("  {}", creature.name));
            }
        }
    }
    out
}

fn asked_header(creature: &Creature) -> String {
    match creature.level {
        Some(level) => format!("{} ({level})", creature.name),
        None => creature.name.clone(),
    }
}

fn detail(creature: &Creature) -> Vec<String> {
    let mut out = Vec::new();
    if !creature.kind.is_empty() {
        out.push(format!("  Type: {}", creature.kind));
    }
    if let Some(hp) = creature.hp {
        out.push(format!("  HP: {hp}"));
    }
    if creature.boss {
        out.push("  Boss".to_string());
    }
    if let Some(as_) = creature.as_ {
        out.push(format!("  AS {as_}"));
    }
    if let Some(cs) = creature.cs {
        out.push(format!("  CS {cs}"));
    }
    if let Some(td) = creature.td {
        out.push(format!("  TD: {td}"));
    }
    if !creature.skin.is_empty() {
        out.push(format!("  skin: {}", creature.skin));
    }
    if !creature.treasure.is_empty() {
        out.push(format!("  Treasure: {}", creature.treasure));
    }
    if !creature.rooms.is_empty() {
        out.push(format!("  {} rooms", creature.rooms.len()));
        for room in &creature.rooms {
            out.push(format!("go2 u{room}"));
        }
    }
    out
}

/// A stable room uid from the room's title (and optionally its description).
///
/// **ESTABLISHED** shape (`0x101a800b4`): hashes the strings and formats
/// `"{}:{}"` — the uid, a colon, and the title.  The hash function itself is
/// inlined, so this is the documented equivalent (FNV-1a, which is stable
/// across runs, unlike `DefaultHasher`).
pub fn room_uid(title: &str, description: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in title.bytes().chain(description.bytes()) {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}:{title}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_levels_bands_and_names() {
        assert_eq!(Asked::parse("29"), Ok(Asked::Level(29)));
        assert_eq!(
            Asked::parse("25-33"),
            Ok(Asked::Band(Band { low: 25, high: 33 }))
        );
        assert_eq!(
            Asked::parse("kobold"),
            Ok(Asked::Creature("kobold".to_string()))
        );
        assert_eq!(
            Asked::parse("0").unwrap_err(),
            "`0` is not a level: 29, or a band, 25-33."
        );
        assert_eq!(
            Asked::parse("33-25").unwrap_err(),
            "`33-25` is not a level: 29, or a band, 25-33."
        );
        assert!(Asked::parse("").is_err());
        assert_eq!(Band { low: 25, high: 33 }.text(), "25-33");
    }

    #[test]
    fn answering_a_level_and_a_missing_creature() {
        let mut bestiary = Bestiary::new();
        let mut kobold = Creature::new("kobold");
        kobold.level = Some(3);
        kobold.kind = "humanoid".into();
        kobold.hp = Some(40);
        bestiary.put(kobold);

        let hits = bestiary.at(&Asked::Level(3));
        assert_eq!(hits.len(), 1);
        assert_eq!(
            answered(&Asked::Level(3), &hits),
            vec!["Level 3:", "  kobold"]
        );

        let hits = bestiary.at(&Asked::Creature("kobold".to_string()));
        assert_eq!(
            answered(&Asked::Creature("kobold".to_string()), &hits),
            vec!["kobold (3)", "  Type: humanoid", "  HP: 40"]
        );

        let hits = bestiary.at(&Asked::Creature("dragon".to_string()));
        assert_eq!(
            answered(&Asked::Creature("dragon".to_string()), &hits),
            vec!["The bestiary has no creature like dragon."]
        );
    }

    #[test]
    fn bands_and_families() {
        let mut bestiary = Bestiary::new();
        for (name, level, family) in [("kobold", 3, "kobold"), ("orc", 30, "giant")] {
            let mut c = Creature::new(name);
            c.level = Some(level);
            c.family = family.into();
            bestiary.put(c);
        }
        let band = Asked::Band(Band { low: 25, high: 33 });
        let hits = bestiary.at(&band);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "orc");
        let family = Asked::Family("kobold".to_string());
        assert_eq!(bestiary.at(&family).len(), 1);
        assert_eq!(
            answered(&Asked::Level(3), &bestiary.at(&Asked::Level(3))),
            vec!["Level 3:", "  kobold"]
        );
    }

    #[test]
    fn a_room_uid_is_stable_and_carries_the_title() {
        let uid = room_uid("Town Square, Center", "You stand in the square.");
        assert_eq!(uid, room_uid("Town Square, Center", "You stand in the square."));
        assert!(uid.ends_with(":Town Square, Center"));
        assert_ne!(uid, room_uid("Town Square, Center", "different"));
        assert_eq!(uid.split(':').count(), 2);
    }
}
