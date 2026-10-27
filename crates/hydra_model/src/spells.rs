//! The spell table and its helpers.
//!
//! Recovered (arm64, arm64 ABI):
//!
//! * `spells::all` / `spells::with_cooldowns` share the same address
//!   (`0x101bc4c20`, 112 B): an `OnceLock` accessor over a static slice.
//! * `spells::spell_named` (860 B): `trim`, ASCII-lowercase into a fresh
//!   `String` (`sub w11, w10, #0x41; cmp #0x1a; orr #0x20`), hash it
//!   (`BuildHasher::hash_one`), probe a `HashMap<&str, u16>`, then binary-search
//!   the sorted `u16` id array (`ldrh` scan over `count << 1` bytes) and index a
//!   pointer array (`ldr x8, [x8 + idx*8 + 0xef0]`). Returns `Option<&Spell>`
//!   with the null niche.
//! * `spells::spell` (200 B): the id lookup without the name hash.
//! * `Spell` stride is `0x68`: `String` name at `+0x00`, `f64` at `+0x18`,
//!   pattern at `+0x20`, optional second pattern at `+0x40`, `u16` id at
//!   `+0x60`.
//! * `timer_change` (348 B): `trim`, then for each spell run the pattern (and
//!   the optional second pattern) over the name; on a hit it clones the name,
//!   copies the `f64` and the `u16` id into the 40-byte return value.
//! * `Role::parse` (348 B): exact spellings `attack`, `offense`/`offensive`,
//!   `defense`, `utility`, `timer`, `bonus`; the `Option` `None` tag is `6`.
//! * `cooldown_landing` (1508 B) reads named groups (`noun`) out of a
//!   `"your group"`-flavoured message.
//!
//! `// UNKNOWN:` the spell table itself (a generated `spells` data file) is not
//! embedded; [`all`] returns an empty slice, so every lookup answers `None`.

pub mod expr;
pub mod extras;

use std::sync::OnceLock;

use crate::pattern::{compile, Pattern};

/// A spell row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spell {
    pub name: String,
    /// The `f64` at `+0x18` — the timer/cast duration used by
    /// [`timer_change`].
    pub seconds: f64,
    /// The message pattern at `+0x20`.
    pub pattern: Pattern,
    /// The optional second pattern at `+0x40`.
    pub pattern_alt: Option<Pattern>,
    /// The id at `+0x60` (the sorted key used by [`spell`]).
    pub id: u16,
    /// `// UNKNOWN:` where the role lives in the 0x68-byte row; the parser
    /// exists, so the field does too.
    pub role: Option<Role>,
}

impl Spell {
    /// Build a row from its parts.
    pub fn new(name: &str, seconds: f64, id: u16, pattern: &str) -> Spell {
        Spell {
            name: name.to_string(),
            seconds,
            pattern: compile(&format!("(?i){pattern}")),
            pattern_alt: None,
            id,
            role: None,
        }
    }

    /// @0x0000000101bc53d0 (208 B) — does this row have the given id?
    pub fn is(&self, id: u16) -> bool {
        self.id == id
    }

    /// Does either pattern match `line`?
    pub fn matches(&self, line: &str) -> bool {
        self.pattern.is_match(line)
            || self
                .pattern_alt
                .as_ref()
                .map(|alt| alt.is_match(line))
                .unwrap_or(false)
    }
}

/// What role a spell plays in a combat round.
///
/// Recovered discriminants: `attack` = 0, `offensive`/`offense` = 1,
/// `defense` = 2, `utility` = 3, `timer` = 4, `bonus` = 5; the `Option` `None`
/// tag is 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Attack,
    Offensive,
    Defense,
    Utility,
    Timer,
    Bonus,
}

impl Role {
    /// @0x0000000101bc5274 (348 B)
    pub fn parse(text: &str) -> Option<Role> {
        let text = text.trim();
        match text {
            "attack" => Some(Role::Attack),
            "offense" | "offensive" => Some(Role::Offensive),
            "defense" => Some(Role::Defense),
            "utility" => Some(Role::Utility),
            "timer" => Some(Role::Timer),
            "bonus" => Some(Role::Bonus),
            _ => None,
        }
    }

    pub fn index(self) -> u8 {
        match self {
            Role::Attack => 0,
            Role::Offensive => 1,
            Role::Defense => 2,
            Role::Utility => 3,
            Role::Timer => 4,
            Role::Bonus => 5,
        }
    }
}

/// A timer change reported by [`timer_change`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimerChange {
    pub name: String,
    pub seconds: f64,
    pub id: u16,
}

static SPELLS: OnceLock<Vec<Spell>> = OnceLock::new();

/// @0x0000000101bc4c20 (112 B) — every known spell.
pub fn all() -> &'static [Spell] {
    SPELLS.get_or_init(Vec::new)
}

/// @0x0000000101bc4c20 (same address as [`all`], 112 B)
pub fn with_cooldowns() -> &'static [Spell] {
    all()
}

/// Install a spell table (data-loader hook; the table itself is not embedded).
pub fn set_spells(table: Vec<Spell>) -> bool {
    SPELLS.set(table).is_ok()
}

/// @0x0000000101bc4768 (860 B)
pub fn spell_named(name: &str) -> Option<&'static Spell> {
    let needle = name.trim().to_lowercase();
    all()
        .iter()
        .find(|spell| spell.name.to_lowercase() == needle)
}

/// @0x0000000101bc54a0 (200 B)
pub fn spell(id: u16) -> Option<&'static Spell> {
    let mut ids: Vec<&Spell> = all().iter().collect();
    ids.sort_by_key(|spell| spell.id);
    ids.into_iter().find(|spell| spell.id == id)
}

/// @0x0000000101bc4ac4 (348 B)
///
/// `// UNKNOWN:` the `Option` encoding of the return value (the binary writes
/// the `0x8000_0000_0000_0001` niche word); modelled as `Option<TimerChange>`.
pub fn timer_change(name: &str) -> Option<TimerChange> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    all()
        .iter()
        .find(|spell| spell.matches(name))
        .map(|spell| TimerChange {
            name: spell.name.clone(),
            seconds: spell.seconds,
            id: spell.id,
        })
}

/// A cooldown "landing" message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CooldownLanding {
    pub spell: Option<String>,
    pub target: Option<String>,
    pub group: bool,
}

/// @0x0000000101bc4c90 (1508 B)
///
/// `// UNKNOWN:` the pattern; the recovered body reads the `noun` group and
/// tests for `"your group"`.
pub fn cooldown_landing(line: &str) -> Option<CooldownLanding> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let group = line.contains("your group");
    let target = line
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .filter(|word| !word.is_empty())
        .last()
        .map(|word| word.to_string());
    if !group && target.is_none() {
        return None;
    }
    Some(CooldownLanding {
        spell: None,
        target,
        group,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Vec<Spell> {
        vec![
            Spell::new("Minor Shock", 1.0, 10, "^A tiny bolt of lightning"),
            Spell::new("Fireball", 3.0, 20, "^A ball of fire"),
        ]
    }

    #[test]
    fn role_parse_accepts_the_recovered_spellings() {
        assert_eq!(Role::parse("attack"), Some(Role::Attack));
        assert_eq!(Role::parse("offense"), Some(Role::Offensive));
        assert_eq!(Role::parse("offensive"), Some(Role::Offensive));
        assert_eq!(Role::parse("defense"), Some(Role::Defense));
        assert_eq!(Role::parse("utility"), Some(Role::Utility));
        assert_eq!(Role::parse("timer"), Some(Role::Timer));
        assert_eq!(Role::parse("bonus"), Some(Role::Bonus));
        // `defensive` is *not* accepted by the recovered dispatch
        assert_eq!(Role::parse("defensive"), None);
        assert_eq!(Role::parse(" Attack "), None);
    }

    #[test]
    fn role_indices_are_the_recovered_discriminants() {
        assert_eq!(Role::Attack.index(), 0);
        assert_eq!(Role::Offensive.index(), 1);
        assert_eq!(Role::Defense.index(), 2);
        assert_eq!(Role::Utility.index(), 3);
        assert_eq!(Role::Timer.index(), 4);
        assert_eq!(Role::Bonus.index(), 5);
    }

    #[test]
    fn table_lookups() {
        assert!(spell_named("Fireball").is_none(), "no table is embedded");
        assert!(spell(20).is_none());
        assert_eq!(all().len(), with_cooldowns().len());
        assert!(timer_change("Fireball").is_none());
        assert!(timer_change("   ").is_none());
    }

    #[test]
    fn spell_row_accessors() {
        let spell = Spell::new("Fireball", 3.0, 20, "^A ball of fire");
        assert!(spell.is(20));
        assert!(!spell.is(21));
        assert!(spell.matches("A ball of fire erupts!"));
        assert!(!spell.matches("A gentle breeze."));
    }

    #[test]
    fn cooldown_landing_detects_the_group_flavour() {
        let landing = cooldown_landing("The spell lands on your group.").unwrap();
        assert!(landing.group);
        assert!(landing.target.is_some());
        assert!(cooldown_landing("").is_none());
    }
}
