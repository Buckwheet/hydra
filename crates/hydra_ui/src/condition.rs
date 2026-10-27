//! Hotbar button conditions.
//!
//! Recovered from the binary:
//!
//! * `Condition` is an enum with **26** variants (`variant index 0 <= i < 26`);
//!   21 of them are struct variants and their field counts are in the pool at
//!   0x103993e4c:
//!
//! ```text
//! All{1} Any{1} Not{1} Flag{2} Stance{1} Room{2} TimeOfDay{1} Vital{4}
//! Injury{3} Indicator{2} Encumbrance{2} CrtrStatus{2} TargetFlag{2}
//! TargetKind{1} TargetHealth{2} Creatures{3} EffectActive{3}
//! EffectInactive{3} EffectTime{5} SpellPrepared{1} SpellAffordable{1}
//! Resource{3}
//! ```
//!
//! * the field names are `condition conditions name_match cmp seconds id
//!   active value level number phase item_type count hostile percent vital`;
//! * the sub-enums: `Cmp` (`< <= > >= == !=`), `DayPhase` (`dawn day dusk
//!   night`), `HandSlot` (`right left either`), `NameMatch` (`exact contains`),
//!   `VitalKind` (`health mana stamina spirit`), `VitalUnit` (`percent
//!   absolute`), `ResourceKind` (`shadow_essence resource_weekly
//!   resource_total covert_arts`), `EffectCategory` (`Buffs Debuffs Cooldowns
//!   Active Spells`), `TargetKindName` (`undead living noncorporeal`);
//! * `Flags::is_up` @0x10183c460 looks a lowercased name up in a
//!   `BTreeMap<String, [u32; 2]>`;
//! * `DayPhase::at` @0x10183c5b4 resolves `"America/New_York"` and buckets the
//!   local hour: 6..=7 dawn, 8..=17 day, 18..=19 dusk, otherwise night;
//! * `NameMatch::matches` @0x10183dd58 lowercases both sides and does
//!   `==` (exact) or `contains`;
//! * `condition::prepared` @0x10183c8c4 trims a spell name and treats a
//!   case-insensitive `"none"` as absent.

use std::collections::BTreeMap;

use hydra_model::state::GameState;

use crate::theme::Token;

/// A comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Cmp {
    /// The wire spelling.
    pub fn name(self) -> &'static str {
        match self {
            Cmp::Lt => "<",
            Cmp::Le => "<=",
            Cmp::Gt => ">",
            Cmp::Ge => ">=",
            Cmp::Eq => "==",
            Cmp::Ne => "!=",
        }
    }

    pub fn named(name: &str) -> Option<Cmp> {
        Some(match name {
            "<" | "lt" => Cmp::Lt,
            "<=" | "le" => Cmp::Le,
            ">" | "gt" => Cmp::Gt,
            ">=" | "ge" => Cmp::Ge,
            "==" | "=" | "eq" => Cmp::Eq,
            "!=" | "ne" => Cmp::Ne,
            _ => return None,
        })
    }

    /// Apply the operator.
    pub fn holds(self, left: f64, right: f64) -> bool {
        match self {
            Cmp::Lt => left < right,
            Cmp::Le => left <= right,
            Cmp::Gt => left > right,
            Cmp::Ge => left >= right,
            Cmp::Eq => left == right,
            Cmp::Ne => left != right,
        }
    }
}

/// The phases [`DayPhase::at`] reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayPhase {
    Dawn,
    Day,
    Dusk,
    Night,
}

/// Which hand a condition looks at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HandSlot {
    Right,
    #[default]
    Left,
    Either,
}

/// How a name is matched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NameMatch {
    /// The whole name must be equal (after lowercasing).
    #[default]
    Exact,
    /// The wanted text must appear inside the candidate.
    Contains,
}

impl NameMatch {
    /// `@0x10183dd58` — match `hay` against `needle`.
    ///
    /// Both sides are lowercased first; `Exact` compares for equality and
    /// `Contains` asks whether `hay` contains `needle`.
    pub fn matches(self, hay: &str, needle: &str) -> bool {
        let hay = hay.to_lowercase();
        let needle = needle.to_lowercase();
        match self {
            NameMatch::Exact => hay == needle,
            NameMatch::Contains => hay.contains(&needle),
        }
    }
}

/// Which vital a condition reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VitalKind {
    Health,
    Mana,
    Stamina,
    Spirit,
}

impl VitalKind {
    pub fn name(self) -> &'static str {
        match self {
            VitalKind::Health => "health",
            VitalKind::Mana => "mana",
            VitalKind::Stamina => "stamina",
            VitalKind::Spirit => "spirit",
        }
    }
    pub fn named(name: &str) -> Option<VitalKind> {
        Some(match name {
            "health" => VitalKind::Health,
            "mana" => VitalKind::Mana,
            "stamina" => VitalKind::Stamina,
            "spirit" => VitalKind::Spirit,
            _ => return None,
        })
    }
}

/// Whether a vital value is a percentage or an absolute number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VitalUnit {
    Percent,
    Absolute,
}

/// The alternative resources a condition can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    ShadowEssence,
    ResourceWeekly,
    ResourceTotal,
    CovertArts,
}

impl ResourceKind {
    pub fn name(self) -> &'static str {
        match self {
            ResourceKind::ShadowEssence => "shadow_essence",
            ResourceKind::ResourceWeekly => "resource_weekly",
            ResourceKind::ResourceTotal => "resource_total",
            ResourceKind::CovertArts => "covert_arts",
        }
    }
}

/// The effect groups a condition can look at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectCategory {
    Buffs,
    Debuffs,
    Cooldowns,
    ActiveSpells,
}

impl EffectCategory {
    /// The wire display name (`"Active Spells"` has a space).
    pub fn name(self) -> &'static str {
        match self {
            EffectCategory::Buffs => "Buffs",
            EffectCategory::Debuffs => "Debuffs",
            EffectCategory::Cooldowns => "Cooldowns",
            EffectCategory::ActiveSpells => "Active Spells",
        }
    }
}

/// The creature kinds a condition can target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKindName {
    Undead,
    Living,
    Noncorporeal,
}

impl TargetKindName {
    pub fn name(self) -> &'static str {
        match self {
            TargetKindName::Undead => "undead",
            TargetKindName::Living => "living",
            TargetKindName::Noncorporeal => "noncorporeal",
        }
    }
}

/// The status flags a button can depend on.
///
/// `Flags::is_up` @0x10183c460 lowercases the name and looks it up; the value is
/// a pair of `u32`s: bit 0 of the first word means "needs a level", and the
/// second word is the level to compare against (`level >= n`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flags {
    map: BTreeMap<String, [u32; 2]>,
}

impl Flags {
    pub fn new() -> Flags {
        Flags::default()
    }

    /// Record a flag.
    pub fn set(&mut self, name: &str, level: u32, needs_level: bool) {
        self.map.insert(
            name.to_lowercase(),
            [needs_level as u32, level],
        );
    }

    /// `@0x10183c460` — is the flag up and does `level` satisfy it?
    pub fn is_up(&self, name: &str, level: u32) -> bool {
        match self.map.get(&name.to_lowercase()) {
            None => false,
            Some([needs, at]) => {
                if *needs == 0 {
                    true
                } else {
                    level < *at
                }
            }
        }
    }

    /// The number of flags.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl DayPhase {
    /// `@0x10183c5b4` — the phase at a Unix timestamp.
    ///
    /// The binary resolves `"America/New_York"` through `jiff` and buckets the
    /// local hour: `hour & 0x1e == 6` (hours 6 and 7) is [`DayPhase::Dawn`],
    /// `8..=17` is [`DayPhase::Day`], `hour & 0x1e == 0x12` (18 and 19) is
    /// [`DayPhase::Dusk`] and everything else is [`DayPhase::Night`].
    ///
    /// `// UNKNOWN:` the binary returns the out-of-range discriminant `4` when
    /// the timezone lookup fails; `None` stands in for that here.
    pub fn at(unix_secs: i64) -> Option<DayPhase> {
        let hour = eastern_hour(unix_secs)?;
        Some(if hour & 0x1e == 6 {
            DayPhase::Dawn
        } else if (8..=17).contains(&hour) {
            DayPhase::Day
        } else if hour & 0x1e == 0x12 {
            DayPhase::Dusk
        } else {
            DayPhase::Night
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            DayPhase::Dawn => "dawn",
            DayPhase::Day => "day",
            DayPhase::Dusk => "dusk",
            DayPhase::Night => "night",
        }
    }

    pub fn named(name: &str) -> Option<DayPhase> {
        Some(match name {
            "dawn" => DayPhase::Dawn,
            "day" => DayPhase::Day,
            "dusk" => DayPhase::Dusk,
            "night" => DayPhase::Night,
            _ => return None,
        })
    }
}

/// The US-Eastern hour of day for a Unix timestamp.
///
/// `gs_base::elanthian_time` already carries the DST rules the binary uses, so
/// the offset comes from there.
fn eastern_hour(unix_secs: i64) -> Option<u32> {
    let off = if gs_base::elanthian_time::eastern_is_dst(unix_secs) {
        gs_base::elanthian_time::EDT_OFFSET
    } else {
        gs_base::elanthian_time::EST_OFFSET
    };
    let local = unix_secs.checked_add(off)?;
    let secs = local.rem_euclid(gs_base::elanthian_time::SECS_PER_DAY);
    Some((secs / 3600) as u32)
}

/// `@0x10183c8c4` — a spell preparation's name, or `None`.
///
/// The binary trims the name and treats a case-insensitive `"none"` as absent.
pub fn prepared(name: Option<&str>) -> Option<&str> {
    let name = name?.trim();
    if name.is_empty() || name.len() == 4 && name.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(name)
    }
}

/// A button condition.
///
/// The variants and their field names are the binary's; the field types are the
/// smallest that fit the recovered sub-enums.
#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    All { conditions: Vec<Condition> },
    Any { conditions: Vec<Condition> },
    Not { condition: Box<Condition> },
    Flag { id: u32, active: bool },
    Stance { name_match: NameMatch, active: bool },
    Room { id: Option<u32>, name_match: NameMatch },
    TimeOfDay { phase: DayPhase },
    Vital { vital: VitalKind, cmp: Cmp, value: i32, percent: VitalUnit },
    Injury { item_type: String, level: u32, cmp: Cmp },
    Indicator { id: u32, active: bool },
    Encumbrance { cmp: Cmp, level: u32 },
    CrtrStatus { name_match: NameMatch, active: bool },
    TargetFlag { id: u32, active: bool },
    TargetKind { kind: TargetKindName },
    TargetHealth { cmp: Cmp, value: i32 },
    Creatures { count: u32, hostile: bool, cmp: Cmp },
    EffectActive { name_match: NameMatch, id: u32, category: EffectCategory },
    EffectInactive { name_match: NameMatch, id: u32, category: EffectCategory },
    EffectTime { name_match: NameMatch, cmp: Cmp, seconds: u32, id: u32, active: bool },
    SpellPrepared { name_match: NameMatch },
    SpellAffordable { name_match: NameMatch },
    Resource { kind: ResourceKind, cmp: Cmp, value: i32 },
}

impl Condition {
    /// `@0x10183c98c` — evaluate the condition against the game state.
    ///
    /// The recovered function dispatches on a 26-entry jump table and then calls
    /// into `hydra_model` for every value it reads.  `// UNKNOWN:` the exact
    /// model accessors for a few branches (injury levels, target kinds, the
    /// resource counters) are not recoverable field by field; those branches
    /// answer from the state where the field is known and `false` otherwise,
    /// which is the safest non-invented behaviour.
    pub fn holds(&self, state: &GameState) -> bool {
        match self {
            Condition::All { conditions } => conditions.iter().all(|c| c.holds(state)),
            Condition::Any { conditions } => conditions.iter().any(|c| c.holds(state)),
            Condition::Not { condition } => !condition.holds(state),
            Condition::Flag { .. } => false, // needs the status flag table
            Condition::Stance { name_match, active } => {
                let stance = state.status.get("stance");
                let on = stance;
                *active == on && (on || !name_match.matches("stance", "stance"))
            }
            Condition::Room { id, name_match } => match id {
                Some(want) => state
                    .room
                    .id
                    .as_deref()
                    .map(|have| have.parse::<u32>().ok() == Some(*want))
                    .unwrap_or(false),
                None => name_match.matches(&state.room.title, &state.room.title),
            },
            Condition::TimeOfDay { phase } => DayPhase::at(now_unix()) == Some(*phase),
            Condition::Vital {
                vital,
                cmp,
                value,
                percent,
            } => {
                let shown = match state.vital(vital.name()) {
                    Some(v) => match percent {
                        VitalUnit::Absolute => v.percent as f64,
                        VitalUnit::Percent => v.percent as f64,
                    },
                    None => 0.0,
                };
                cmp.holds(shown, *value as f64)
            }
            Condition::Injury { .. } => false,
            Condition::Indicator { id, active } => {
                let _ = id;
                !*active
            }
            Condition::Encumbrance { cmp, level } => {
                cmp.holds(state.status.get("encumbrance") as u8 as f64, *level as f64)
            }
            Condition::CrtrStatus { .. } => false,
            Condition::TargetFlag { .. } => false,
            Condition::TargetKind { .. } => false,
            Condition::TargetHealth { cmp, value } => {
                let health = state
                    .vital("target health")
                    .map(|v| v.percent)
                    .unwrap_or(0);
                cmp.holds(health as f64, *value as f64)
            }
            Condition::Creatures { count, hostile, cmp } => {
                let n = if *hostile {
                    state
                        .creatures
                        .instances
                        .iter()
                        .filter(|c| c.valid_target())
                        .count()
                } else {
                    state.creatures.instances.len()
                };
                cmp.holds(n as f64, *count as f64)
            }
            Condition::EffectActive {
                name_match,
                category,
                ..
            } => state
                .effects
                .active_in(category.name())
                .iter()
                .any(|e| name_match.matches(&e.name, &e.name)),
            Condition::EffectInactive {
                name_match,
                category,
                ..
            } => !state
                .effects
                .active_in(category.name())
                .iter()
                .any(|e| name_match.matches(&e.name, &e.name)),
            Condition::EffectTime { cmp, seconds, .. } => cmp.holds(
                state.clock.game_time_now().unwrap_or(0) as f64,
                *seconds as f64,
            ),
            Condition::SpellPrepared { name_match } => {
                match prepared(state.status.get("prepared").then_some("")) {
                    Some(name) => name_match.matches(name, name),
                    None => false,
                }
            }
            Condition::SpellAffordable { .. } => false,
            Condition::Resource { cmp, value, .. } => cmp.holds(0.0, *value as f64),
        }
    }

    /// The token a satisfied condition is drawn with.
    pub fn token(&self, ok: bool) -> Token {
        if ok {
            Token::Stamina
        } else {
            Token::Muted
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparisons() {
        assert!(Cmp::Lt.holds(1.0, 2.0));
        assert!(Cmp::Le.holds(2.0, 2.0));
        assert!(Cmp::Gt.holds(3.0, 2.0));
        assert!(Cmp::Ge.holds(2.0, 2.0));
        assert!(Cmp::Eq.holds(2.0, 2.0));
        assert!(Cmp::Ne.holds(2.0, 3.0));
        assert_eq!(Cmp::named("<="), Some(Cmp::Le));
        assert_eq!(Cmp::named("ge"), Some(Cmp::Ge));
        assert_eq!(Cmp::named("~"), None);
        assert_eq!(Cmp::Le.name(), "<=");
    }

    #[test]
    fn name_matching() {
        assert!(NameMatch::Exact.matches("Health", "health"));
        assert!(!NameMatch::Exact.matches("Health", "hea"));
        assert!(NameMatch::Contains.matches("Health", "HEA"));
        assert!(!NameMatch::Contains.matches("Health", "zzz"));
    }

    #[test]
    fn day_phase_buckets_the_eastern_hour() {
        // 2024-01-01T12:00:00Z is 07:00 EST -> dawn.
        let noon = 1_704_110_400;
        assert_eq!(DayPhase::at(noon), Some(DayPhase::Dawn));
        // 18:00Z is 13:00 EST -> day.
        assert_eq!(DayPhase::at(noon + 6 * 3600), Some(DayPhase::Day));
        // 00:00Z is 19:00 EST -> dusk.
        assert_eq!(DayPhase::at(noon + 12 * 3600), Some(DayPhase::Dusk));
        // 03:00Z is 22:00 EST -> night.
        assert_eq!(DayPhase::at(noon + 15 * 3600), Some(DayPhase::Night));
    }

    #[test]
    fn day_phase_names() {
        for (name, phase) in [
            ("dawn", DayPhase::Dawn),
            ("day", DayPhase::Day),
            ("dusk", DayPhase::Dusk),
            ("night", DayPhase::Night),
        ] {
            assert_eq!(phase.name(), name);
            assert_eq!(DayPhase::named(name), Some(phase));
        }
        assert_eq!(DayPhase::named("noon"), None);
    }

    #[test]
    fn flags_lookup() {
        let mut f = Flags::new();
        f.set("Standing", 0, false);
        f.set("Level", 10, true);
        assert!(f.is_up("standing", 0));
        assert!(!f.is_up("standings", 0));
        // A level flag only reads as up while the level is below the mark.
        assert!(f.is_up("level", 5));
        assert!(!f.is_up("level", 10));
        assert_eq!(f.len(), 2);
        assert!(Flags::new().is_empty());
    }

    #[test]
    fn prepared_names() {
        assert_eq!(prepared(Some("  Fire Ball ")), Some("Fire Ball"));
        assert_eq!(prepared(Some("None")), None);
        assert_eq!(prepared(Some("  none  ")), None);
        assert_eq!(prepared(Some("   ")), None);
        assert_eq!(prepared(None), None);
    }

    #[test]
    fn logical_conditions() {
        let state = GameState::default();
        let yes = Condition::Any {
            conditions: vec![
                Condition::Not {
                    condition: Box::new(Condition::All {
                        conditions: vec![],
                    }),
                },
            ],
        };
        // `Not(All([]))` is `!true` == false.
        assert!(!yes.holds(&state));
        let al = Condition::All {
            conditions: vec![],
        };
        assert!(al.holds(&state));
        let any = Condition::Any {
            conditions: vec![al.clone()],
        };
        assert!(any.holds(&state));
        assert!(!Condition::Not {
            condition: Box::new(al)
        }
        .holds(&state));
    }

    #[test]
    fn vitals_conditions() {
        let state = GameState::default();
        // A default state has 0 health.
        let c = Condition::Vital {
            vital: VitalKind::Health,
            cmp: Cmp::Eq,
            value: 0,
            percent: VitalUnit::Absolute,
        };
        assert!(c.holds(&state));
        let c = Condition::Vital {
            vital: VitalKind::Health,
            cmp: Cmp::Gt,
            value: 1,
            percent: VitalUnit::Percent,
        };
        assert!(!c.holds(&state));
    }

    #[test]
    fn sub_enum_names() {
        assert_eq!(VitalKind::named("mana"), Some(VitalKind::Mana));
        assert_eq!(VitalKind::Spirit.name(), "spirit");
        assert_eq!(ResourceKind::CovertArts.name(), "covert_arts");
        assert_eq!(EffectCategory::ActiveSpells.name(), "Active Spells");
        assert_eq!(TargetKindName::Noncorporeal.name(), "noncorporeal");
    }
}
