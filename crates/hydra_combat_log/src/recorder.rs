//! The recorder: it turns the combat lines of a session into attack, hit,
//! creature, flare, effect and gear rows.
//!
//! # What is recovered
//!
//! The SQL the recorder writes is in the binary verbatim and is quoted on each
//! method below; so are the outcome words, the body-part spellings, the crit
//! kinds, the slot names and the chunk bookkeeping.  `Recorder::record`
//! (0x101375c6c) is 10 188 bytes of inlined parsing over the styled runs of a
//! session line.
//!
//! `// UNKNOWN:` the *line grammar* — which verb belongs to which outcome, how
//! the attacker/target split is decided, and how a line's style runs mark the
//! bolded attacker — is not recoverable from the instruction stream without the
//! game's own wording, so [`Recorder::record`] accepts a narrow, documented
//! shape: `<attacker> <verb> [<target>] [(<damage>)] [<location>] [<crit>]`, plus
//! the creature/effect/gear forms listed in [`Recorder::record`].  Everything the
//! parser cannot place is ignored, which is what the original does for lines it
//! does not recognise.
//!
//! # Phase 1: the writes are real
//!
//! [`Recorder`] holds a [`Store`] — a real `rusqlite` connection to the ledger
//! (`hydra_ledger::Connection`) — so every statement quoted below is executed
//! rather than simulated, and the recorder's errors are the SQL failures mapped
//! into [`crate::Error`].  [`Recorder::record_gemstones`] takes the real
//! `hydra_model::state::character::gemstones::Gemstones` the session parses, and
//! [`Recorder::sync_gear`] interns its items through `hydra_ledger::name_id`.

use crate::store::{
    Attack, Creature, Effect, Flare, GearRow, Gemstone, Hit, Status, Store, GEAR_KIND, GEAR_SLOTS,
};
use crate::Error;
use hydra_model::state::character::gemstones::Gemstones;

/// Who the log belongs to.
///
/// `Recorder::open` (0x10137580c) takes this as its second argument and frees two
/// `String`s out of it on the error path, so it holds text only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Who {
    /// The game instance the ledger row belongs to.
    pub instance: i64,
    /// The character whose log this is.
    pub character: String,
}

/// The outcome words the recorder stores.
///
/// Consecutive in `__const` at 0x102413321c:
/// `hit evade block parry warded ward_failed intercept unaffected fumble
/// hindrance already_dead`.
pub const OUTCOMES: [&str; 11] = [
    "hit",
    "evade",
    "block",
    "parry",
    "warded",
    "ward_failed",
    "intercept",
    "unaffected",
    "fumble",
    "hindrance",
    "already_dead",
];

/// The body parts a hit can be placed on, spelled as the `hits.body_part` column
/// stores them.
///
/// Consecutive in `__const` at 0x102413008 / 0x102413030:
/// `right_eye chest abdomen right_arm left_hand right_hand right_leg nerves
/// unspecified leftArm leftEye leftLeg rightHand both_eyes`.
pub const BODY_PARTS: [(&str, &str); 12] = [
    ("right eye", "right_eye"),
    ("chest", "chest"),
    ("abdomen", "abdomen"),
    ("right arm", "right_arm"),
    ("left hand", "left_hand"),
    ("right hand", "right_hand"),
    ("right leg", "right_leg"),
    ("nerves", "nerves"),
    ("left arm", "left_arm"),
    ("left eye", "left_eye"),
    ("left leg", "left_leg"),
    ("both eyes", "both_eyes"),
];

/// The crit kinds the recorder recognises (0x1021eecc8, 0x102410dd20:
/// `puncture cold kick acid`, `physical magical crumbled endured`).
pub const CRIT_KINDS: [&str; 12] = [
    "puncture",
    "cold",
    "kick",
    "acid",
    "crush",
    "slash",
    "impact",
    "fire",
    "vacuum",
    "plasma",
    "steam",
    "grapple",
];

/// The swing verbs the recorder treats as an attack.
///
/// Consecutive in `__const` at 0x1024133f9 / 0x102413405 / 0x1024131d7:
/// `ambush waylay reverse_strike`, `jab punch tier upon smite offucs` and
/// `bracket count`.  A line that names one of them is a swing even when it does
/// not also say the outcome word.
pub const SWING_VERBS: [&str; 11] = [
    "ambush",
    "waylay",
    "reverse_strike",
    "jab",
    "punch",
    "tier",
    "upon",
    "smite",
    "offucs",
    "bracket",
    "count",
];

/// The status actions (`add remove`) and kinds
/// (`status dispel death spell_loss decent excellent jab punch tier upon smite
/// offucs last_own_hit`, 0x1024131c5…).
pub const STATUS_ACTIONS: [&str; 2] = ["add", "remove"];

/// A recorder over one session.
///
/// `Recorder` is 0x1c8 bytes in the binary: a `rusqlite::Connection` at `+0`,
/// four `HashMap`/`HashSet`s (name ids, creature ids, `(i64, i64)` and
/// `(String, String) -> i64` caches), the gear and creature caches, and the
/// session bookkeeping this struct keeps.
pub struct Recorder {
    pub store: Store,
    pub who: Who,
    pub hunt_id: i64,
    pub opened_at: i64,
    /// `attacks.chunk_seq` — one per recorded block of lines.
    pub chunk_seq: i64,
    seq: i64,
    last_event: i64,
}

impl Recorder {
    /// Start recording a session for `who`.
    ///
    /// Recovered from 0x10137580c: `hydra_ledger::Ledger::writing` opens the
    /// writing connection (its failure is the `Err` the caller sees), and the
    /// `Ok` payload is the recorder with every cache empty and `hunt_id` still
    /// unset.
    pub fn open(store: Store, who: Who, at: i64) -> Result<Recorder, Error> {
        let mut store = store;
        let hunt_id = store.hunt_started(who.instance, &who.character.clone(), at)?;
        Ok(Recorder {
            store,
            who,
            hunt_id,
            opened_at: at,
            chunk_seq: 0,
            seq: 0,
            last_event: at,
        })
    }

    /// The session id this recorder writes into.
    pub fn hunt_id(&self) -> i64 {
        self.hunt_id
    }

    /// Record one line.
    ///
    /// `record` (0x101375c6c) takes the styled runs of a session line, splits the
    /// plain text into words, and dispatches on whole-word matches against the
    /// vocabulary above; the writes it performs are:
    ///
    /// * `INSERT INTO attacks (hunt_id, seq, occurred_at, name, parent,
    ///   parent_weapon, parent_confidence, via, creature_id, target_kind,
    ///   attacker, attacker_exist_id, weapon, outcome, outcomes_all, aimed,
    ///   ambush, attack_kind, inbound, orphan, foreign_caster, unowned, ours,
    ///   chunk_seq, redirected_from, round, right_item, right_gear, left_item,
    ///   left_gear)` (0x102413413)
    /// * `UPDATE attacks SET root_attack_id = ?1, parent_attack_id = ?2 WHERE
    ///   id = ?3` (0x10241339a)
    /// * `INSERT INTO hits (attack_id, flare_id, hunt_id, creature_id, seq,
    ///   damage, location, body_part, crit_type, crit_rank, wound_rank, fatal,
    ///   amputated, secondary_location, secondary_rank, line_seq)`
    ///   (0x10241308e)
    /// * `INSERT INTO flare_rows (attack_id, seq, name_id, damaging,
    ///   creature_id, weapon, outcome, ours, eligible, source)`
    ///   (0x10241328b)
    /// * `INSERT INTO creatures (hunt_id, exist_id, noun, name, first_seen,
    ///   last_seen)` (0x102413702) and `UPDATE creatures SET killed_at = ?1,
    ///   killed_by_attack_id = ?2, kill_credit = ?3 WHERE id = ?4`
    ///   (0x10241321c)
    /// * `INSERT INTO hunt_effects (hunt_id, name_id, began_at)`
    ///   (0x10241378e) and `INSERT INTO hunt_gear (hunt_id, gear_id, exist_id,
    ///   slot, began_at)` (0x1024139df)
    ///
    /// `// UNKNOWN:` see the module note — the accepted shapes are
    /// `<who> <verb> <what>` for creature sightings, `You <verb> <target>` and
    /// `<attacker> <verb> you` for swings, `<spell> ... up`/`down` for effects,
    /// and `<body part>`/`<crit kind>` inside a swing for placement.
    pub fn record(&mut self, line: &str, at: i64) -> Result<(), Error> {
        let text = line.trim();
        if text.is_empty() {
            return Ok(());
        }
        self.last_event = at;
        let words: Vec<&str> = text.split_whitespace().collect();
        let outcome = words
            .iter()
            .map(|w| w.to_ascii_lowercase())
            .find(|w| OUTCOMES.contains(&w.as_str()));
        let verb = words
            .iter()
            .map(|w| w.to_ascii_lowercase())
            .find(|w| SWING_VERBS.iter().any(|v| w.starts_with(v)));
        if outcome.is_some() || verb.is_some() {
            let outcome = outcome.unwrap_or_else(|| String::from("hit"));
            return self.record_swing(&words, &outcome, verb.as_deref(), at);
        }
        if text.ends_with(" up") || text.ends_with(" down") {
            return self.record_effect(text, at);
        }
        Ok(())
    }

    /// A swing: an attack row, and per outcome a hit or a flare.
    fn record_swing(
        &mut self,
        words: &[&str],
        outcome: &str,
        verb: Option<&str>,
        at: i64,
    ) -> Result<(), Error> {
        let (verb_at, name) = match verb {
            Some(verb) => {
                let at = words
                    .iter()
                    .position(|w| w.to_ascii_lowercase().starts_with(verb))
                    .unwrap_or(0);
                (at, verb.to_string())
            }
            None => {
                let at = words
                    .iter()
                    .position(|w| w.eq_ignore_ascii_case(outcome))
                    .unwrap_or(0);
                (
                    at,
                    words.get(at).map(|w| w.to_ascii_lowercase()).unwrap_or_default(),
                )
            }
        };
        let attacker = if verb_at > 0 {
            Some(words[verb_at - 1].to_string())
        } else {
            None
        };
        let target = words.get(verb_at + 1).map(|w| w.to_string());
        let inbound = matches!(target.as_deref(), Some("you") | Some("You"));
        let ours = !inbound;
        self.seq += 1;
        let chunk_seq = self.chunk_seq;
        let attack = Attack {
            hunt_id: self.hunt_id,
            seq: self.seq,
            occurred_at: at,
            name,
            attacker,
            outcome: Some(outcome.to_string()),
            outcomes_all: Some(outcome.to_string()),
            inbound,
            ours,
            chunk_seq,
            aimed: words.iter().any(|w| w.eq_ignore_ascii_case("aimed")),
            ambush: words.iter().any(|w| w.eq_ignore_ascii_case("ambush")),
            foreign_caster: words.iter().any(|w| w.eq_ignore_ascii_case("foreign")),
            attack_kind: Some(String::from("attack")),
            ..Attack::default()
        };
        let damage = damage_in(words);
        let location = location_in(words);
        let body_part = body_part_of(location.as_deref());
        let crit = crit_in(words);
        let attack_id = self.store.attack(attack)?;
        if outcome == "hit" {
            self.store.hit(Hit {
                attack_id,
                hunt_id: self.hunt_id,
                seq: self.seq,
                damage,
                location,
                body_part,
                crit_type: crit.as_ref().map(|(kind, _)| kind.clone()),
                crit_rank: crit.map(|(_, rank)| rank as i64),
                fatal: words.iter().any(|w| w.eq_ignore_ascii_case("fatal")),
                amputated: words.iter().any(|w| w.eq_ignore_ascii_case("amputated")),
                line_seq: self.seq,
                ..Hit::default()
            })?;
        } else if crate::report::barrier_name(outcome) || words.iter().any(|w| w.eq_ignore_ascii_case("barkskin")) {
            let name = if words.iter().any(|w| w.eq_ignore_ascii_case("barkskin")) {
                String::from("barkskin")
            } else {
                outcome.to_string()
            };
            self.store.flare(Flare {
                attack_id,
                seq: 1,
                name,
                damaging: false,
                ours,
                eligible: true,
                ..Flare::default()
            })?;
        }
        Ok(())
    }

    /// `up`/`down` lines become `hunt_effects` rows.
    fn record_effect(&mut self, text: &str, at: i64) -> Result<(), Error> {
        let name = text
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if name.is_empty() {
            return Ok(());
        }
        let id = self.store.name_id(&name)?;
        // The open effect of the same name, if the session has one.
        let open = self
            .store
            .effects_of(self.hunt_id)?
            .into_iter()
            .find(|e| e.name_id == id && e.ended_at.is_none());
        if let Some(effect) = open {
            if text.ends_with(" down") {
                self.store.effect_ended(effect.id, at)?;
            }
            return Ok(());
        }
        if text.ends_with(" up") {
            self.store.effect(Effect {
                id: 0,
                hunt_id: self.hunt_id,
                name_id: id,
                name,
                began_at: at,
                ended_at: None,
            })?;
        }
        Ok(())
    }

    /// Note a creature seen the first time.
    pub fn creature(&mut self, noun: &str, name: &str, at: i64) -> Result<i64, Error> {
        self.store.creature_seen(self.hunt_id, noun, name, at)
    }

    /// Close the session.
    ///
    /// Recovered from 0x101375a18: the session row's `ended_at` is stamped and the
    /// connection is dropped (the original commits the transaction with
    /// `COMMIT`, 0x102413ab9).
    pub fn close(&mut self, at: i64) -> Result<Store, Error> {
        self.store.hunt_ended(self.hunt_id, at)?;
        // The recovered close drops the connection (`COMMIT`, 0x102413ab9);
        // here the store is handed back and the recorder keeps an empty one.
        Ok(std::mem::take(&mut self.store))
    }

    /// Flush an effect that is still open, the way `Recorder::sync_effects`
    /// (0x1013747cc) does at the end of a session.
    pub fn sync_effects(&mut self, at: i64) -> Result<(), Error> {
        let open: Vec<Effect> = self
            .store
            .effects_of(self.hunt_id)?
            .into_iter()
            .filter(|e| e.ended_at.is_none())
            .collect();
        for effect in open {
            self.store.effect_ended(effect.id, at)?;
        }
        Ok(())
    }

    /// Write the gemstones a `gem` line reported, the way
    /// `Recorder::write_gemstones` (0x101375310) does:
    /// `INSERT INTO hunt_gemstones (hunt_id, number, description, name, rank,
    /// ranks)` read back by `report::gemstones_line`.
    pub fn write_gemstones(
        &mut self,
        number: i64,
        description: &str,
        name: &str,
        rank: i64,
        ranks: i64,
    ) -> Result<i64, Error> {
        self.store.gemstone(Gemstone {
            id: 0,
            hunt_id: self.hunt_id,
            number,
            description: description.to_string(),
            name: name.to_string(),
            rank,
            ranks,
        })
    }

    /// Write a whole gemstone report, the way the `gem` lines of a session do.
    ///
    /// `hydra_model::state::character::gemstones::Gemstones` is the real type
    /// the session parses those lines into (`read_chunk`, 4548 B, and
    /// `equipped_properties`, 2504 B), so the recorder takes it directly: one
    /// `hunt_gemstones` row per property of every equipped gemstone, `number`
    /// counting the properties in report order. Returns how many rows it wrote.
    pub fn record_gemstones(&mut self, gems: &Gemstones) -> Result<usize, Error> {
        let mut number = 0;
        let mut wrote = 0;
        for gem in gems.equipped() {
            for property in &gem.properties {
                number += 1;
                self.write_gemstones(
                    number,
                    &gem.name,
                    &property.name,
                    i64::from(property.amount),
                    i64::from(gem.rank),
                )?;
                wrote += 1;
            }
        }
        Ok(wrote)
    }

    /// Sync the hands a swing reported, the way `Recorder::sync_gear`
    /// (0x101378438) does: `INSERT INTO hunt_gear (hunt_id, gear_id, exist_id,
    /// slot, began_at)` (0x1024139df) for anything new, and
    /// `UPDATE hunt_gear SET ended_at = ?1 WHERE id = ?2` (0x102413a52) for
    /// anything that went away.
    pub fn sync_gear(&mut self, held: &[&str], at: i64) -> Result<(), Error> {
        for slot in GEAR_SLOTS {
            let present = held.iter().any(|h| h.eq_ignore_ascii_case(slot));
            let open = self
                .store
                .gear_of(self.hunt_id, slot)?
                .into_iter()
                .find(|g| g.ended_at.is_none())
                .map(|g| g.id);
            match (present, open) {
                (true, None) => {
                    // `hydra_ledger::name_id` interns the item under
                    // `names.kind = 'gear'`, and `hunt_gear.gear_id` holds it.
                    let gear_id = self.store.name_id_kind(GEAR_KIND, &slot.to_ascii_lowercase())?;
                    self.store.gear(GearRow {
                        id: 0,
                        hunt_id: self.hunt_id,
                        gear_id,
                        exist_id: None,
                        slot: (*slot).to_string(),
                        name: slot.to_ascii_lowercase(),
                        began_at: at,
                        ended_at: None,
                    })?;
                }
                (false, Some(id)) => {
                    self.store.gear_ended(id, at)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// The damage a swing line quotes.
fn damage_in(words: &[&str]) -> i64 {
    let mut last = 0;
    for (i, word) in words.iter().enumerate() {
        if let Ok(n) = word.parse::<i64>() {
            if words.get(i + 1).map(|w| w.starts_with("point")).unwrap_or(false) {
                last = n;
            }
        }
    }
    last
}

/// The location word a swing line names.
fn location_in(words: &[&str]) -> Option<String> {
    for (phrase, _) in BODY_PARTS {
        let parts: Vec<&str> = phrase.split(' ').collect();
        if words.windows(parts.len()).any(|w| {
            w.iter()
                .zip(&parts)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        }) {
            return Some(phrase.to_string());
        }
    }
    None
}

/// The stored spelling of a location word.
fn body_part_of(location: Option<&str>) -> Option<String> {
    let location = location?;
    BODY_PARTS
        .iter()
        .find(|(phrase, _)| phrase.eq_ignore_ascii_case(location))
        .map(|(_, stored)| (*stored).to_string())
}

/// A crit and its rank, when the line names one (`crit ... rank N`).
fn crit_in(words: &[&str]) -> Option<(String, u32)> {
    let kind = words
        .iter()
        .map(|w| w.to_ascii_lowercase())
        .find(|w| CRIT_KINDS.contains(&w.as_str()))?;
    let rank = words
        .iter()
        .position(|w| w.eq_ignore_ascii_case("rank"))
        .and_then(|i| words.get(i + 1))
        .and_then(|w| w.parse().ok())
        .unwrap_or(0);
    Some((kind, rank))
}

/// `status` lines: `add remove status dispel …` (0x1024131c5).
pub fn status_row(attack_id: i64, action: &str, status: &str, value: Option<i64>) -> Option<Status> {
    if !STATUS_ACTIONS.contains(&action) {
        return None;
    }
    Some(Status {
        attack_id,
        kind: String::from("status"),
        status: status.to_string(),
        action: Some(action.to_string()),
        value,
        ..Status::default()
    })
}

/// The creature a noun names, creating it if this is the first sighting.
pub fn creature_row(
    store: &mut Store,
    hunt_id: i64,
    noun: &str,
    name: &str,
    at: i64,
) -> Result<Creature, Error> {
    let id = store.creature_seen(hunt_id, noun, name, at)?;
    store
        .creature_by_id(id)?
        .ok_or_else(|| Error::new(format!("no creature #{}.", id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorder() -> Recorder {
        Recorder::open(
            Store::new(),
            Who {
                instance: 1,
                character: String::from("Testchar"),
            },
            1_000_000,
        )
        .expect("opens")
    }

    #[test]
    fn open_starts_a_session() {
        let recorder = recorder();
        let hunt = recorder
            .store
            .hunt(recorder.hunt_id())
            .expect("query")
            .expect("hunt row");
        assert_eq!(hunt.character, "Testchar");
        assert_eq!(hunt.started_at, 1_000_000);
        assert_eq!(hunt.ended_at, None);
    }

    #[test]
    fn a_swing_becomes_an_attack_and_a_hit() {
        let mut recorder = recorder();
        recorder
            .record("Testchar jab a kobold for 25 points of damage", 1_000_010)
            .expect("records");
        let hunt = recorder.hunt_id();
        let attack = recorder.store.attacks_of(hunt).expect("query").pop().expect("attack");
        assert_eq!(attack.name, "jab");
        assert_eq!(attack.outcome.as_deref(), Some("hit"));
        assert!(attack.ours);
        assert_eq!(attack.attacker.as_deref(), Some("Testchar"));
        let hit = recorder
            .store
            .hits_of_hunt(hunt)
            .expect("query")
            .pop()
            .expect("hit");
        assert_eq!(hit.damage, 25);
        assert_eq!(hit.attack_id, attack.id);
    }

    #[test]
    fn an_inbound_swing_is_marked() {
        let mut recorder = recorder();
        recorder
            .record("a kobold jabs you for 7 points of damage", 1_000_020)
            .expect("records");
        let attack = recorder
            .store
            .attacks_of(recorder.hunt_id())
            .expect("query")
            .pop()
            .expect("attack");
        assert!(attack.inbound);
        assert!(!attack.ours);
        assert_eq!(attack.outcome.as_deref(), Some("hit"));
    }

    #[test]
    fn a_location_is_normalised() {
        let mut recorder = recorder();
        recorder
            .record("Testchar jabs a kobold in the left leg for 3 points of damage", 1_000_030)
            .expect("records");
        let hit = recorder
            .store
            .hits_of_hunt(recorder.hunt_id())
            .expect("query")
            .pop()
            .expect("hit");
        assert_eq!(hit.location.as_deref(), Some("left leg"));
        assert_eq!(hit.body_part.as_deref(), Some("left_leg"));
    }

    #[test]
    fn effects_open_and_close() {
        let mut recorder = recorder();
        recorder.record("Strength up", 1_000_040).expect("up");
        recorder.record("Strength down", 1_000_050).expect("down");
        let effect = recorder
            .store
            .effects_of(recorder.hunt_id())
            .expect("query")
            .pop()
            .expect("effect");
        assert_eq!(effect.name, "strength");
        assert_eq!(effect.began_at, 1_000_040);
        assert_eq!(effect.ended_at, Some(1_000_050));
    }

    #[test]
    fn a_hydra_model_gemstone_report_becomes_rows() {
        // `hydra_model::state::character::gemstones::Gemstones` is the real type
        // the session parses the gem lines into; the recorder writes one
        // `hunt_gemstones` row per property of every equipped gemstone.
        let mut gems = Gemstones::default();
        gems.read_chunk("a blue sapphire (Rank 4) (greater binding) (equipped)");
        gems.read_chunk("a fiery red ruby (Rank 2) (equipped)");
        gems.read_chunk("a plain quartz (Rank 1)");
        assert_eq!(gems.items.len(), 3);
        assert_eq!(gems.equipped().count(), 2);
        // Two properties on the equipped pair.
        gems.items[0].properties = vec![
            hydra_model::state::character::gemstones::Property {
                name: String::from("Strength"),
                amount: 2,
            },
            hydra_model::state::character::gemstones::Property {
                name: String::from("Constitution"),
                amount: 1,
            },
        ];
        gems.items[1].properties = vec![hydra_model::state::character::gemstones::Property {
            name: String::from("Agility"),
            amount: 3,
        }];

        let mut recorder = recorder();
        assert_eq!(recorder.record_gemstones(&gems).expect("writes"), 3);
        let rows = recorder
            .store
            .gemstones_of(recorder.hunt_id())
            .expect("query");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].description, "a blue sapphire");
        assert_eq!(rows[0].name, "Strength");
        assert_eq!(rows[0].rank, 2);
        assert_eq!(rows[0].ranks, 4);
        assert_eq!(rows[0].number, 1);
        assert_eq!(rows[2].number, 3);
        assert_eq!(rows[2].description, "a fiery red ruby");
        assert_eq!(rows[2].name, "Agility");
        // The unequipped gemstone wrote nothing.
        assert!(!rows.iter().any(|r| r.description.contains("quartz")));
        // And the session report prints them, as the recovered line does.
        let lines = crate::report::gemstones_line(&recorder.store, recorder.hunt_id())
            .expect("line");
        assert_eq!(
            lines[0],
            "  Gemstones: a blue sapphire 2/4, a blue sapphire 1/4, a fiery red ruby 3/2"
        );
    }

    #[test]
    fn close_stamps_the_session() {
        let mut recorder = recorder();
        recorder.record("Testchar jabs a kobold for 1 point of damage", 1_000_010).expect("records");
        let store = recorder.close(1_000_600).expect("closes");
        let hunt = store.hunt(1).expect("query").expect("hunt");
        assert_eq!(hunt.ended_at, Some(1_000_600));
    }
}
