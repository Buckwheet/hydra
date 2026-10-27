//! One attack, one creature and one spawn tree in detail.

use crate::report::{self, Scope};
use crate::store::{Attack, Hit, Store};
use crate::{Error, Lines};

/// The attack row `detail::attack_rows` reads (0x102410d48):
///
/// ```sql
///   SELECT a.name, a.outcome, a.outcomes_all, a.seq, a.occurred_at, a.hunt_id, a.creature_id,
///          c.name, c.exist_id, a.root_attack_id, a.parent_attack_id, a.parent, a.parent_weapon,
///          a.parent_confidence, a.via, a.target_kind, a.attacker, a.weapon, a.aimed, a.ambush,
///          …
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttackRow {
    pub id: i64,
    pub name: String,
    pub outcome: Option<String>,
    pub outcomes_all: Option<String>,
    pub seq: i64,
    pub occurred_at: i64,
    pub hunt_id: i64,
    pub creature_id: Option<i64>,
    pub creature_name: Option<String>,
    pub root_attack_id: Option<i64>,
    pub parent_attack_id: Option<i64>,
    pub parent: Option<String>,
    pub parent_weapon: Option<String>,
    pub via: Option<String>,
    pub target_kind: Option<String>,
    pub attacker: Option<String>,
    pub aimed: bool,
    pub ambush: bool,
}

impl AttackRow {
    fn of(store: &Store, a: &Attack) -> AttackRow {
        AttackRow {
            id: a.id,
            name: a.name.clone(),
            outcome: a.outcome.clone(),
            outcomes_all: a.outcomes_all.clone(),
            seq: a.seq,
            occurred_at: a.occurred_at,
            hunt_id: a.hunt_id,
            creature_id: a.creature_id,
            creature_name: a
                .creature_id
                .and_then(|c| store.creature(c))
                .map(|c| c.name.clone()),
            root_attack_id: a.root_attack_id,
            parent_attack_id: a.parent_attack_id,
            parent: a.parent.clone(),
            parent_weapon: a.parent_weapon.clone(),
            via: a.via.clone(),
            target_kind: a.target_kind.clone(),
            attacker: a.attacker.clone(),
            aimed: a.aimed,
            ambush: a.ambush,
        }
    }
}

/// The flags of an attack, in the order the binary checks them
/// (`INBOUNDFOREIGN CASTERUNOWNEDORPHANAIMEDAMBUSH`, 0x102410f05).
fn flag_words(a: &Attack) -> Vec<&'static str> {
    let mut out = Vec::new();
    if a.inbound {
        out.push("INBOUND");
    }
    if a.foreign_caster {
        out.push("FOREIGN CASTER");
    }
    if a.unowned {
        out.push("UNOWNED");
    }
    if a.orphan {
        out.push("ORPHAN");
    }
    if a.aimed {
        out.push("AIMED");
    }
    if a.ambush {
        out.push("AMBUSH");
    }
    out
}

/// A hit's crit, as the detail line spells it.
///
/// Recovered from `detail::crit_str` (0x10134d46c): the crit type is upper-cased,
/// a rank prints as `R{}` (0x103e674df) and the whole thing is bracketed with
/// `[{}]` (0x103e21ed5).
pub fn crit_str(hit: &Hit) -> String {
    let mut parts = Vec::new();
    if let Some(kind) = &hit.crit_type {
        parts.push(kind.to_uppercase());
    }
    if let Some(rank) = hit.crit_rank {
        if rank > 0 {
            parts.push(format!("R{}", rank));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("[{}]", parts.join(" "))
    }
}

/// `SELECT damage, location, crit_rank, crit_type, fatal FROM hits WHERE
/// attack_id = ?1 AND flare_id IS NULL ORDER BY seq` (0x102411706).
pub fn hits_of(store: &Store, attack_id: i64) -> Vec<&Hit> {
    store.hits_of(attack_id)
}

/// One attack in full.
///
/// Recovered literals (0x1013435dc): `No attack #{}`, the title
/// `=== ATTACK REPORT #{} ==={}`, `  Weapon: {}`, `Type: {}{}{}`,
/// `Outcome: {}`, `Sequence: {}   Session: {}`, `Occurred: {}`, `{} (#{})`,
/// `{} (attacker: {})`, `Target: {}`, `Attacker: {}`, `Flags: {}`,
/// ` (via {})`, `  ({:} {:})`, `, RESOLUTION:`, `STATUS EFFECTS: NONE  (window)`,
/// `FLARES: NONE [damaging]`, `CRIT`, ` AMPUTATED`, `[FATAL]`,
/// `  (this attack)`, `no outcome recorded`, `no target`.
pub fn attack_detail(store: &Store, id: i64, _deepen: bool) -> Lines {
    let attack = match store.attacks.iter().find(|a| a.id == id) {
        Some(a) => a,
        None => return Err(Error::new(format!("No attack #{}.", id))),
    };
    let row = AttackRow::of(store, attack);
    let mut out = Vec::new();
    let flags = flag_words(attack);
    out.push(format!(
        "=== ATTACK REPORT #{} ==={}",
        id,
        if flags.is_empty() {
            String::new()
        } else {
            format!(" {}", flags.join(", "))
        }
    ));
    if let Some(weapon) = attack.weapon {
        out.push(format!("  Weapon: {}", store.name(weapon)));
    }
    out.push(format!(
        "Type: {}{}{}",
        attack.attack_kind.clone().unwrap_or_default(),
        if attack.aimed { " aimed" } else { "" },
        if attack.ambush { " ambush" } else { "" }
    ));
    out.push(format!(
        "Outcome: {}",
        row.outcome
            .clone()
            .unwrap_or_else(|| String::from("no outcome recorded"))
    ));
    out.push(format!(
        "Sequence: {}   Session: {}",
        row.seq, row.hunt_id
    ));
    out.push(format!("Occurred: {}", report::stamp(row.occurred_at)));
    if let Some(name) = &row.creature_name {
        out.push(format!(
            "Target: {} (#{})",
            name,
            row.creature_id.unwrap_or(0)
        ));
    } else {
        // UNKNOWN: no `Target: no target` literal survives; `Target: ` and the
        // missing-target case are recovered separately.
        out.push(String::from("Target: no target"));
    }
    if let Some(attacker) = &row.attacker {
        out.push(format!("Attacker: {}", attacker));
    }
    if let Some(via) = &row.via {
        out.push(format!(" (via {})", via));
    }
    out.push(format!("Flags: {}", flags.join(", ")));

    out.push(String::from("RESOLUTION:"));
    let hits = hits_of(store, id);
    if hits.is_empty() {
        out.push(String::from("RESOLUTION: NONE\tPenalty: "));
    } else {
        for hit in &hits {
            let mut line = format!(
                "hit {} {}",
                hit.location.clone().unwrap_or_else(|| String::from("no target")),
                hit.damage
            );
            let crit = crit_str(hit);
            if !crit.is_empty() {
                line.push(' ');
                line.push_str(&crit);
            }
            if hit.amputated {
                line.push_str(" AMPUTATED");
            }
            if hit.fatal {
                line.push_str(" [FATAL]");
            }
            out.push(line);
        }
    }
    let flares = store.flares_of(id);
    if flares.is_empty() {
        out.push(String::from("FLARES: NONE"));
    } else {
        for flare in flares {
            out.push(format!(
                "{} [{}{}]{}",
                flare.name,
                if flare.damaging { "damaging" } else { "" },
                if flare.eligible { "eligible" } else { "" },
                if flare.ours { "  (this attack)" } else { "" }
            ));
        }
    }
    let statuses: Vec<_> = store.statuses.iter().filter(|s| s.attack_id == id).collect();
    if statuses.is_empty() {
        out.push(String::from("STATUS EFFECTS: NONE  (window)"));
    } else {
        for s in statuses {
            out.push(format!("  {} {} {}", s.kind, s.status, s.value.unwrap_or(0)));
        }
    }
    out.push(format!("cstats attack #{}", id));
    if let Some(creature) = row.creature_id {
        out.push(format!("cstats creature {}", creature));
    }
    Ok(out)
}

/// The most recent attack called `name`, or the list of names there are.
///
/// `detail::attack_names` (0x1013430dc) runs
/// `SELECT DISTINCT name FROM attacks WHERE hunt_id {} AND ours = 1 ORDER BY
/// name` (0x103e66ae2) and prints `Attacks recorded (cstats attack <name> for
/// one):` followed by `  {}` per name; `detail::attack_rows` links each one with
/// `cstats attack #{}`.
pub fn attack_by_name(
    store: &Store,
    character: &str,
    name: &str,
    nth: Option<i64>,
    scope: &Scope,
) -> Lines {
    const HEADS: &str = "Attacks recorded (cstats attack <name> for one):";
    let hunts = report::resolve(store, character, scope)?;
    let mut found: Vec<&Attack> = Vec::new();
    for id in &hunts.ids {
        for a in store.attacks_of(*id) {
            if a.name.eq_ignore_ascii_case(name) && a.ours {
                found.push(a);
            }
        }
    }
    if found.is_empty() {
        let mut names: Vec<String> = store
            .attacks
            .iter()
            .filter(|a| hunts.ids.contains(&a.hunt_id) && a.ours)
            .map(|a| a.name.clone())
            .collect();
        names.sort();
        names.dedup();
        let mut out = vec![format!("Unknown attack '{}'. {}", name, HEADS)];
        for n in names {
            out.push(format!("  {}", n));
        }
        return Ok(out);
    }
    found.sort_by_key(|a| a.occurred_at);
    let index = nth.unwrap_or(1).max(1) as usize;
    if index > found.len() {
        return Ok(vec![format!(
            "Index {} is out of range for {} ({} seen).",
            index,
            name,
            found.len()
        )]);
    }
    let attack = found[found.len() - index];
    attack_detail(store, attack.id, false)
}

/// The spawn tree of a root attack.
///
/// Recovered from 0x10134a6a8: `SELECT id, parent FROM attacks WHERE
/// parent_attack_id = ?1 ORDER BY seq` (0x102410c4a) walks the tree, each child
/// printing as `  {}{}` (0x103e2252a) with ` -> {}` (0x103e66ab5) arrows, and an
/// attack with no children prints
/// `SEQUENCE: single attack (no spawn tree); hit  (this attack)` (0x102411240).
pub fn tree(store: &Store, root: i64) -> Lines {
    let attack = match store.attacks.iter().find(|a| a.id == root) {
        Some(a) => a,
        None => return Err(Error::new(format!("No attack #{}.", root))),
    };
    let mut children: Vec<&Attack> = store
        .attacks
        .iter()
        .filter(|a| a.parent_attack_id == Some(root))
        .collect();
    children.sort_by_key(|a| a.seq);
    let mut out = Vec::new();
    if children.is_empty() {
        out.push(format!(
            "SEQUENCE: single attack (no spawn tree); hit  (this attack) #{}",
            root
        ));
        return Ok(out);
    }
    // UNKNOWN: the tree heading is built from the recovered `SEQUENCE: `
    // prefix; the ` (root)` marker is not a literal in the pool.
    out.push(format!("SEQUENCE: {} (root)", attack.name));
    for child in children {
        out.push(format!("  {} -> {}", child.seq, child.name));
    }
    Ok(out)
}

/// A creature's sequences.
///
/// Recovered literals (0x10134aff8): `Unknown creature '{}'. The nouns recorded:`
/// then `  {}` per noun, `No creature with id {}.`,
/// `Index {} is out of range for {} ({} seen).`, ` Name: {}`,
/// ` Noun: {:18} ID: {}   Session: {}`, `First: {:18} Last: {}   Duration: {}`,
/// `, by {} #{}`, `False ({}{})`, `  DMG: {:18} Hits: {:6} Alive: {}`,
/// `{} (root, on {})`, `  (no attacks recorded)`, `No creatures recorded yet.`
/// and `cstats creature <id>|<noun>: the top 20 by damage`.
pub fn creature(
    store: &Store,
    character: &str,
    id: Option<i64>,
    noun: Option<&str>,
    nth: Option<i64>,
    full: bool,
) -> Lines {
    const HEADS: &str = "cstats creature <id>|<noun>: the top 20 by damage";
    let hunts = report::resolve(store, character, &Scope::Last)?;
    let mut seen: Vec<&crate::store::Creature> = Vec::new();
    for hunt in &hunts.ids {
        seen.extend(store.creatures_of(*hunt));
    }
    if seen.is_empty() {
        return Ok(vec![String::from("No creatures recorded yet.")]);
    }
    let picked = match (id, noun) {
        (Some(id), _) => match store.creature(id) {
            Some(c) => c,
            None => return Ok(vec![format!("No creature with id {}.", id)]),
        },
        (None, Some(noun)) => match seen
            .iter()
            .find(|c| c.noun.eq_ignore_ascii_case(noun) || c.name.eq_ignore_ascii_case(noun))
        {
            Some(c) => *c,
            None => {
                let mut nouns: Vec<String> = seen.iter().map(|c| c.noun.clone()).collect();
                nouns.sort();
                nouns.dedup();
                let mut out = vec![format!("Unknown creature '{}'. The nouns recorded:", noun)];
                for n in nouns {
                    out.push(format!("  {}", n));
                }
                return Ok(out);
            }
        },
        (None, None) => {
            return Ok(vec![format!("Creature: {}", HEADS)]);
        }
    };
    if let Some(index) = nth {
        if index < 1 {
            return Ok(vec![format!(
                "Index {} is out of range for {} ({} seen).",
                index,
                picked.noun,
                seen.len()
            )]);
        }
    }
    let mut out = Vec::new();
    out.push(format!(" Name: {}", picked.name));
    out.push(format!(
        " Noun: {:<18} ID: {}   Session: {}",
        picked.noun, picked.id, picked.hunt_id
    ));
    out.push(format!(
        "First: {:<18} Last: {}   Duration: {}",
        report::stamp(picked.first_seen),
        report::stamp(picked.last_seen),
        report::duration(Some(
            picked.last_seen.saturating_sub(picked.first_seen) as u64
        ))
    ));
    match (&picked.kill_credit, picked.killed_by_attack_id) {
        (Some(credit), Some(by)) => out.push(format!(", by {} #{}", credit, by)),
        (Some(credit), None) => out.push(format!(", by {}", credit)),
        _ => out.push(format!(
            "False ({}{})",
            picked.killed_at.unwrap_or(0),
            picked.killed_by_attack_id.unwrap_or(0)
        )),
    }
    let attacks: Vec<&Attack> = store
        .attacks
        .iter()
        .filter(|a| a.creature_id == Some(picked.id))
        .collect();
    let damage: i64 = store
        .hits
        .iter()
        .filter(|h| attacks.iter().any(|a| a.id == h.attack_id && a.ours))
        .map(|h| h.damage)
        .sum();
    let hits = store
        .hits
        .iter()
        .filter(|h| attacks.iter().any(|a| a.id == h.attack_id))
        .count();
    out.push(format!(
        "  DMG: {:<18} Hits: {:<6} Alive: {}",
        damage,
        hits,
        picked.killed_at.is_none()
    ));
    if attacks.is_empty() {
        out.push(String::from("  (no attacks recorded)"));
    }
    if full {
        for a in &attacks {
            out.push(match a.root_attack_id {
                Some(root) if root != a.id => format!("{} (root, on {})", a.name, root),
                _ => format!("{} (root, on {})", a.name, a.occurred_at),
            });
        }
    }
    out.extend(wound_map(store, picked.id));
    out.push(format!("cstats creature {}", picked.id));
    Ok(out)
}

/// The body map of a creature's wounds.
///
/// Recovered from `detail::wound_map` (0x10134d874):
/// `SELECT body_part, MAX(COALESCE(wound_rank, 0)) FROM hits WHERE creature_id =
/// ?1 AND body_part IS NOT NULL GROUP BY body_part` (0x102411c01), rendered over
/// the body-part names the crate stores (`head neck back nsys dots ways them
/// ctrl`, `arms book boss wrap`, `leftEye`, `chest bk abdomen rightHand`,
/// `leftFoot rightLeg`) with `{} {}` cells.
///
/// `// UNKNOWN:` the exact ASCII art layout of the seven `format!`s at
/// 0x103e674e3…0x103e67544 is not recoverable (their cells are positional and
/// the drawings are assembled from slices), so this prints one labelled line per
/// wounded location in the recovered order.
pub fn wound_map(store: &Store, creature_id: i64) -> Vec<String> {
    const ORDER: [&str; 12] = [
        "head", "neck", "back", "chest", "abdomen", "left_arm", "right_arm", "left_hand",
        "right_hand", "left_leg", "right_leg", "nsys",
    ];
    let mut worst: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for hit in store.hits.iter().filter(|h| h.creature_id == Some(creature_id)) {
        if let Some(part) = &hit.body_part {
            let rank = hit.wound_rank.unwrap_or(0);
            let slot = worst.entry(part.clone()).or_insert(0);
            *slot = (*slot).max(rank);
        }
    }
    let mut out = Vec::new();
    for part in ORDER {
        if let Some(rank) = worst.get(part) {
            out.push(format!("    {}   {}", part, rank));
        }
    }
    out
}

/// `SELECT exist_id, name FROM creatures WHERE id = ?1` (0x10241166b).
pub fn creature_exist_name(store: &Store, id: i64) -> Option<(Option<String>, String)> {
    store
        .creature(id)
        .map(|c| (c.exist_id.clone(), c.name.clone()))
}

/// `SELECT name FROM creatures WHERE id = ?1` (0x102411287).
pub fn creature_name(store: &Store, id: i64) -> Option<String> {
    store.creature(id).map(|c| c.name.clone())
}

/// What an attack-name search turned up.  `drop_in_place<detail::Found>`
/// (0x10134f238) shows two string payloads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub name: String,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Attack, Hit, Store};

    fn store() -> Store {
        let mut s = Store::new();
        let a = s.hunt_started(1, "Testchar", 1_000_000);
        let kobold = s.creature_seen(a, "kobold", "a kobold", 1_000_100);
        let swing = s.attack(Attack {
            hunt_id: a,
            seq: 1,
            occurred_at: 1_000_200,
            name: String::from("jab"),
            attacker: Some(String::from("Testchar")),
            creature_id: Some(kobold),
            ours: true,
            outcome: Some(String::from("hit")),
            aimed: true,
            attack_kind: Some(String::from("attack")),
            ..Attack::default()
        });
        s.hit(Hit {
            attack_id: swing,
            hunt_id: a,
            creature_id: Some(kobold),
            seq: 1,
            damage: 25,
            location: Some(String::from("left leg")),
            body_part: Some(String::from("left_leg")),
            crit_type: Some(String::from("crush")),
            crit_rank: Some(3),
            wound_rank: Some(2),
            fatal: false,
            amputated: true,
            ..Hit::default()
        });
        let child = s.attack(Attack {
            hunt_id: a,
            seq: 2,
            occurred_at: 1_000_300,
            name: String::from("spawn"),
            parent_attack_id: Some(swing),
            ..Attack::default()
        });
        let _ = child;
        s
    }

    #[test]
    fn crit_str_brackets_kind_and_rank() {
        let hit = Hit {
            crit_type: Some(String::from("crush")),
            crit_rank: Some(3),
            ..Hit::default()
        };
        assert_eq!(crit_str(&hit), "[CRUSH R3]");
        assert_eq!(crit_str(&Hit::default()), "");
    }

    #[test]
    fn attack_detail_prints_the_recovered_headings() {
        let s = store();
        let id = s.attacks[0].id;
        let creature = s.creatures[0].id;
        let lines = attack_detail(&s, id, false).expect("runs");
        assert_eq!(lines[0], format!("=== ATTACK REPORT #{} === AIMED", id));
        assert_eq!(lines[1], "Type: attack aimed");
        assert_eq!(lines[2], "Outcome: hit");
        assert_eq!(lines[3], "Sequence: 1   Session: 1");
        assert!(lines.contains(&format!("Target: a kobold (#{})", creature)));
        assert!(lines
            .iter()
            .any(|l| l == "hit left leg 25 [CRUSH R3] AMPUTATED"));
        assert!(lines.contains(&String::from("FLARES: NONE")));
        assert!(lines.contains(&String::from("STATUS EFFECTS: NONE  (window)")));
    }

    #[test]
    fn attack_detail_unknown_id() {
        let s = store();
        assert_eq!(
            attack_detail(&s, 99, false).unwrap_err().0,
            "No attack #99."
        );
    }

    #[test]
    fn tree_walks_the_children() {
        let s = store();
        let root = s.attacks[0].id;
        let child = s.attacks[1].id;
        let lines = tree(&s, root).expect("runs");
        assert_eq!(lines[0], "SEQUENCE: jab (root)");
        assert_eq!(lines[1], "  2 -> spawn");
        // A leaf has no tree.
        let leaf = tree(&s, child).expect("runs");
        assert!(leaf[0].starts_with("SEQUENCE: single attack (no spawn tree)"));
    }

    #[test]
    fn creature_detail_reports_its_numbers() {
        let s = store();
        let id = s.creatures[0].id;
        let lines = creature(&s, "Testchar", Some(id), None, None, false).expect("runs");
        assert_eq!(lines[0], " Name: a kobold");
        assert_eq!(lines[1], format!(" Noun: kobold             ID: {}   Session: 1", id));
        assert!(lines[4].starts_with("  DMG: 25"));
        assert!(lines.iter().any(|l| l == "    left_leg   2"));
    }
}
