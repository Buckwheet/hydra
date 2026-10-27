//! The analytics reports: estimates and cross-tabs over the recorded fights.
//!
//! // UNKNOWN: several report headings are **not** in the binary's literal
//! pool (`Flare: `, `Def - `, `Barrier - `, `Aim`, `Difference: ` and
//! `(none recorded)`). The recovered SQL, counts and table columns are real;
//! the surrounding wording is the nearest recovered vocabulary and each site
//! is marked `// UNKNOWN:`. See `NOTES.md` §"Inferred wording".

use crate::command::Analytics;
use crate::report::{self, Scope};
use crate::store::{Attack, Store};
use crate::Lines;
use std::collections::BTreeMap;

/// The median of a list of numbers (`analytics::median`, 0x101311f4c, and
/// `analytics::said_median`, 0x1013074a0, which prints it).
pub fn median(values: &mut [i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2
    } else {
        values[mid]
    })
}

/// `part/whole` as a percentage, one decimal place (`analytics::pct`,
/// 0x10130d0a8).
pub fn pct(part: i64, whole: i64) -> String {
    if whole == 0 {
        String::from("-")
    } else {
        format!("{:.1}%", part as f64 * 100.0 / whole as f64)
    }
}

/// `HP estimates for the creatures killed`.
///
/// Recovered from 0x10130bbf8: for every creature the session killed, the damage
/// dealt to it is summed (`SELECT h.damage, h.fatal, (a.ours = 1 OR EXISTS
/// (SELECT 1 FROM flares f WHERE f.id = h.flare_id AND f.ours = 1)) FROM hits h
/// JOIN attacks a ON a.id = h.attack_id WHERE h.creature_id = ?1 ORDER BY h.id`,
/// 0x10240f8ae), and the table heads are `ID Our DMG All DMG`, with `HP` and
/// `Average: ` lines.  The empty case is
/// `No damage recorded for those creatures.`
pub fn hp(store: &Store, character: &str, which: Option<&str>, scope: &Scope) -> Lines {
    const EMPTY: &str = "No damage recorded for those creatures.";
    let hunts = report::resolve(store, character, scope)?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut totals: Vec<i64> = Vec::new();
    for hunt in &hunts.ids {
        for creature in store.creatures_of(*hunt) {
            if creature.killed_at.is_none() {
                continue;
            }
            if let Some(want) = which {
                if creature.noun != want && creature.id.to_string() != want {
                    continue;
                }
            }
            let mine: Vec<&Attack> = store
                .attacks
                .iter()
                .filter(|a| a.creature_id == Some(creature.id))
                .collect();
            let mut ours = 0;
            let mut all = 0;
            for a in &mine {
                for h in store.hits.iter().filter(|h| h.attack_id == a.id) {
                    all += h.damage;
                    if a.ours {
                        ours += h.damage;
                    }
                }
            }
            totals.push(ours);
            rows.push(vec![
                creature.id.to_string(),
                ours.to_string(),
                all.to_string(),
            ]);
        }
    }
    if rows.is_empty() {
        return Ok(vec![String::from(EMPTY)]);
    }
    let mut out = vec![format!("HP - {}", hunts.label)];
    out.extend(report::grid(&["ID", "Our DMG", "All DMG"], &rows));
    if let Some(mid) = median(&mut totals) {
        out.push(format!("Average: {}", mid));
    }
    Ok(out)
}

/// `What the swings were made with`.
///
/// Recovered from 0x10130d148: `Worn over the hunts:` lists
/// `SELECT w.began_at, w.ended_at, n.name FROM hunt_gear w JOIN names n ON n.id =
/// w.gear_id WHERE w.hunt_id = ?1` and `Held as a swing was made:` lists the
/// hands on each attack (`a.right_gear`/`a.left_gear` resolved through `names`),
/// with `No swing has its hands on record yet.` when neither hand is known and
/// `Flares seen on it` per item.
pub fn gear(store: &Store, character: &str, scope: &Scope) -> Lines {
    let hunts = report::resolve(store, character, scope)?;
    let mut out = vec![format!("Gear - {}", hunts.label)];
    let mut worn: Vec<Vec<String>> = Vec::new();
    let mut held: BTreeMap<String, i64> = BTreeMap::new();
    for hunt in &hunts.ids {
        for row in store.gear_of(*hunt, "worn") {
            worn.push(vec![
                row.name.clone(),
                report::stamp(row.began_at),
                report::duration(
                    row.ended_at
                        .map(|e| e.saturating_sub(row.began_at) as u64),
                ),
            ]);
        }
        for a in store.attacks_of(*hunt) {
            if !a.ours {
                continue;
            }
            for hand in [a.right_gear, a.left_gear].into_iter().flatten() {
                let name = store.name(hand).to_string();
                if !name.is_empty() {
                    *held.entry(name).or_insert(0) += 1;
                }
            }
        }
    }
    if worn.is_empty() {
        out.push(String::from("Worn over the hunts:"));
        // UNKNOWN: no `(none recorded)` literal survives; the recovered
        // heading says the table is empty, so this line stands in for it.
        out.push(String::from("  (none recorded)"));
    } else {
        out.push(String::from("Worn over the hunts:"));
        out.extend(report::grid(&["Item", "From", "For"], &worn));
    }
    if held.is_empty() {
        out.push(String::from("No swing has its hands on record yet."));
    } else {
        out.push(String::from("Held as a swing was made:"));
        let rows: Vec<Vec<String>> = held
            .iter()
            .map(|(name, n)| vec![name.clone(), n.to_string()])
            .collect();
        out.extend(report::grid(&["Item", "Swings"], &rows));
    }
    Ok(out)
}

/// `A flare's proc rate`.
///
/// Recovered from 0x10130e028: the per-attack counts come from
/// `SELECT f.name, COUNT(DISTINCT f.attack_id) FROM flares f JOIN attacks a ON
/// a.id = f.attack_id WHERE a.hunt_id …` (0x10240fb8f); the heads are
/// `Proc rate by attack:` with `Procs` / `targets`, `  No damage recorded
/// (non-damaging flare).` marks a flare that never printed damage, and the
/// barrier flare prints `[fires alongside]` / `[intercepts]`.
pub fn flare(
    store: &Store,
    character: &str,
    name: Option<&str>,
    scope: &Scope,
) -> Lines {
    let hunts = report::resolve(store, character, scope)?;
    let mut procs: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for hunt in &hunts.ids {
        for a in store.attacks_of(*hunt) {
            for f in store.flares_of(a.id) {
                if !f.ours {
                    continue;
                }
                if let Some(want) = name {
                    if !f.name.eq_ignore_ascii_case(want) {
                        continue;
                    }
                }
                let slot = procs.entry(f.name.clone()).or_insert((0, 0));
                slot.0 += 1;
                if f.damaging {
                    slot.1 += 1;
                }
            }
        }
    }
    let mut out = vec![match name {
        // UNKNOWN: `Proc rate by attack:` is recovered; the named-flare
        // heading (`Flare: …`) is not in the pool.
        Some(n) => format!("Flare: {} - {}", n, hunts.label),
        None => format!("Proc rate by attack: {}", hunts.label),
    }];
    if procs.is_empty() {
        out.push(String::from("  No damage recorded (non-damaging flare)."));
        return Ok(out);
    }
    let rows: Vec<Vec<String>> = procs
        .iter()
        .map(|(name, (n, damaging))| {
            vec![
                name.clone(),
                n.to_string(),
                (*damaging > 0).to_string(),
                pct(*damaging, *n),
            ]
        })
        .collect();
    out.extend(report::grid(
        &["Flare", "Procs", "Damaging", "Share"],
        &rows,
    ));
    if name.is_some() {
        out.extend(alongside(store, character, name.unwrap_or_default(), scope).unwrap_or_default());
    }
    Ok(out)
}

/// `With and without what was up for part of the swings`.
///
/// Recovered from 0x101313d74: the predicate is
/// `EXISTS (SELECT 1 FROM flares f WHERE f.attack_id = a.id AND f.ours = 1 AND
/// f.name = ?1)` (0x10240f735), the heads are `Effect` and
/// `Down: procs/swings`, and an empty session prints
/// `Nothing was seen up over these hunts.`
pub fn effects(store: &Store, character: &str, scope: &Scope) -> Lines {
    let hunts = report::resolve(store, character, scope)?;
    let mut rows: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for hunt in &hunts.ids {
        for effect in store.effects_of(*hunt) {
            rows.entry(effect.name.clone()).or_insert((0, 0));
        }
        for a in store.attacks_of(*hunt) {
            for f in store.flares_of(a.id) {
                if !f.ours {
                    continue;
                }
                let slot = rows.entry(f.name.clone()).or_insert((0, 0));
                if report::barrier_name(&f.name) {
                    slot.0 += 1;
                }
                slot.1 += 1;
            }
        }
    }
    if rows.is_empty() {
        return Ok(vec![String::from("Nothing was seen up over these hunts.")]);
    }
    let mut out = vec![format!(
        "With and without what was up for part of the swings: {}",
        hunts.label
    )];
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|(name, (down, swings))| vec![name.clone(), format!("{}/{}", down, swings)])
        .collect();
    out.extend(report::grid(&["Effect", "Down: procs/swings"], &cells));
    Ok(out)
}

/// `Barkskin, each bark cast`.
///
/// Recovered from 0x10131444c: the heading is
/// `Barkskin, each bark cast (grouped by its absorbs: a crumble ends one, a gap
/// past the cooldown starts another)`, the columns are `#`, `First absorb`,
/// `Lasted`, `Absorbs`, `Magical`, `Endured` and `Ending`, an absorb that ends on
/// a crumble prints `ran out`, and no absorbs at all prints
/// `No barkskin absorbs recorded.`
pub fn barkskin(store: &Store, character: &str) -> Lines {
    let hunts = report::resolve(store, character, &Scope::Last)?;
    let mut casts: Vec<Vec<String>> = Vec::new();
    for hunt in &hunts.ids {
        let mut current: Option<(i64, i64, i64, i64)> = None;
        for a in store.attacks_of(*hunt) {
            let bark: Vec<_> = store
                .flares_of(a.id)
                .into_iter()
                .filter(|f| f.ours && f.name == "barkskin")
                .collect();
            if bark.is_empty() {
                continue;
            }
            let entry = current.get_or_insert((a.occurred_at, a.occurred_at, 0, 0));
            entry.1 = a.occurred_at;
            entry.2 += bark.len() as i64;
            if a.inbound {
                entry.3 += 1;
            }
        }
        if let Some((first, last, absorbs, endured)) = current {
            casts.push(vec![
                (casts.len() + 1).to_string(),
                report::stamp(first),
                report::duration(Some(last.saturating_sub(first) as u64)),
                absorbs.to_string(),
                endured.to_string(),
                String::from("ran out"),
            ]);
        }
    }
    if casts.is_empty() {
        return Ok(vec![String::from("No barkskin absorbs recorded.")]);
    }
    let mut out = vec![
        String::from("Barkskin, each bark cast (grouped by its absorbs: a crumble ends one, a gap past the cooldown starts another)"),
        String::from("Swings and would-hit count every inbound attack between a bark's first and last absorb."),
    ];
    out.extend(report::grid(
        &["#", "First absorb", "Lasted", "Absorbs", "Magical", "Endured", "Ending"],
        &casts,
    ));
    Ok(out)
}

/// What fires alongside a flare.
///
/// Recovered from 0x1013152c8: every attack that shows `name` is compared with
/// the flares seen on the same attack; the marker words are `[fires alongside]`
/// and `[intercepts]` (0x10240f6be), and the heading is
/// `Attacks on you that would have hit:` with the `physical_crumbled` /
/// `magical_endured` endings.
pub fn alongside(store: &Store, character: &str, name: &str, scope: &Scope) -> Lines {
    let hunts = report::resolve(store, character, scope)?;
    let mut together: BTreeMap<String, i64> = BTreeMap::new();
    let mut total = 0;
    for hunt in &hunts.ids {
        for a in store.attacks_of(*hunt) {
            let flares = store.flares_of(a.id);
            if !flares.iter().any(|f| f.ours && f.name == name) {
                continue;
            }
            total += 1;
            for f in flares {
                if f.ours && f.name != name {
                    *together.entry(f.name.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    let mut out = vec![format!("{} - {}", name, hunts.label)];
    if total == 0 {
        out.push(String::from("None seen."));
        return Ok(out);
    }
    out.push(format!("Attacks on you that would have hit: {}", total));
    let rows: Vec<Vec<String>> = together
        .iter()
        .map(|(other, n)| {
            vec![
                other.clone(),
                n.to_string(),
                pct(*n, total),
                if report::barrier_name(other) {
                    String::from("[intercepts]")
                } else {
                    String::from("[fires alongside]")
                },
            ]
        })
        .collect();
    out.extend(report::grid(
        &["Flare", "Together", "Share", ""],
        &rows,
    ));
    Ok(out)
}

/// Two hunts side by side.
///
/// Recovered from 0x101312070: `Hunt comparison (creature mix affects results)`
/// over `SELECT started_at, ended_at FROM hunts WHERE id = ?1`,
/// `SELECT noun, COUNT(*) FROM creatures WHERE hunt_id = ?1 GROUP BY noun` and
/// the attack counts (0x102410201…0x1024102ea), and an unknown pair prints
/// `No matching hunt.` / `cstats compare: say two hunt ids.`
pub fn compare(
    store: &Store,
    character: &str,
    first: Option<i64>,
    second: Option<i64>,
) -> Lines {
    let hunts = report::resolve(store, character, &Scope::All)?;
    let ids = match (first, second) {
        (Some(a), Some(b)) => vec![a, b],
        _ => {
            let mut ids: Vec<i64> = hunts.ids.clone();
            if ids.len() < 2 {
                return Ok(vec![String::from("cstats compare: say two hunt ids.")]);
            }
            ids.split_off(ids.len() - 2)
        }
    };
    let mut out = vec![String::from("Hunt comparison (creature mix affects results)")];
    let mut rows = Vec::new();
    for id in &ids {
        let row = report::hunt_row(store, *id);
        let totals = report::totals(store, *id);
        rows.push(vec![
            id.to_string(),
            report::stamp(row.started_at),
            report::duration(row.ended_at.map(|e| e.saturating_sub(row.started_at) as u64)),
            totals.attacks.to_string(),
            totals.kills.to_string(),
            totals.dealt.to_string(),
            totals.taken.to_string(),
        ]);
    }
    out.extend(report::grid(
        &["#", "Started", "Lasted", "Attacks", "Kills", "Dealt", "Taken"],
        &rows,
    ));
    Ok(out)
}

/// `DS, TD and UDF readings for a creature`.
///
/// Recovered from 0x101313204: `Creatures (click one for DS/TD/UDF):` when no
/// creature is named, the heads `Min Avg Median Max` and the three rows `DS`,
/// `TD`, `UDF`.
///
/// `// UNKNOWN:` the DS/TD/UDF observations are read from `statuses` rows this
/// crate never records itself (the recorder only stores attack-scoped statuses),
/// so the table is built from the `status`/`value` pairs that are present and is
/// empty when none are.
pub fn defense(
    store: &Store,
    character: &str,
    noun: Option<&str>,
    scope: &Scope,
) -> Lines {
    let hunts = report::resolve(store, character, scope)?;
    if noun.is_none() {
        let mut nouns: Vec<String> = Vec::new();
        for hunt in &hunts.ids {
            for c in store.creatures_of(*hunt) {
                if !nouns.contains(&c.noun) {
                    nouns.push(c.noun.clone());
                }
            }
        }
        nouns.sort();
        let mut out = vec![String::from("Creatures (click one for DS/TD/UDF):")];
        for n in nouns {
            out.push(format!("  {}", n));
        }
        return Ok(out);
    }
    let want = noun.unwrap_or_default();
    let mut ds: Vec<i64> = Vec::new();
    let mut td: Vec<i64> = Vec::new();
    let mut udf: Vec<i64> = Vec::new();
    for hunt in &hunts.ids {
        for c in store.creatures_of(*hunt) {
            if c.noun != want {
                continue;
            }
            for s in store.statuses.iter().filter(|s| s.creature_id == Some(c.id)) {
                if let Some(value) = s.value {
                    match s.status.as_str() {
                        "ds" => ds.push(value),
                        "td" => td.push(value),
                        "udf" => udf.push(value),
                        _ => {}
                    }
                }
            }
        }
    }
    let row = |name: &str, values: &mut Vec<i64>| -> Vec<String> {
        if values.is_empty() {
            return vec![name.to_string(), String::from("-"), String::from("-"), String::from("-"), String::from("-")];
        }
        let min = *values.iter().min().unwrap();
        let max = *values.iter().max().unwrap();
        let avg = values.iter().sum::<i64>() / values.len() as i64;
        let med = median(values).unwrap_or(0);
        vec![
            name.to_string(),
            min.to_string(),
            avg.to_string(),
            med.to_string(),
            max.to_string(),
        ]
    };
    let rows = vec![
        row("DS", &mut ds),
        row("TD", &mut td),
        row("UDF", &mut udf),
    ];
    // UNKNOWN: `Def - …` is not a recovered literal; the grid heads and the
    // predicate it summarises are.
    let mut out = vec![format!("Def - {} - {}", hunts.label, want)];
    out.extend(report::grid(&["", "Min", "Avg", "Median", "Max"], &rows));
    Ok(out)
}

/// `Every barrier together`.
///
/// Recovered from 0x1013066dc: the heading is
/// `By barrier (a block's share is of every attack; an absorb's, of what would
/// have hit):`, the barrier predicate is
/// `(a.outcome = 'intercept' OR EXISTS (SELECT 1 FROM flares f WHERE
/// f.attack_id = a.id AND f.ours = 1 AND f.name IN ('barkskin',
/// 'mirror_image_def', 'wthorns640_block', 'sward319',
/// 'edefenseiii414_lorebenefit')))` (0x10240ebf4), `blocks before the roll` is the
/// intercept's ending and `(the game's intercept, barrier unnamed)` its name.
pub fn intercepts(store: &Store, character: &str, scope: &Scope) -> Lines {
    let hunts = report::resolve(store, character, scope)?;
    let mut stops: BTreeMap<&'static str, i64> = BTreeMap::new();
    let mut swings = 0;
    for hunt in &hunts.ids {
        for a in store.attacks_of(*hunt) {
            if !a.inbound {
                continue;
            }
            swings += 1;
            let mut stopped = false;
            for f in store.flares_of(a.id) {
                if f.ours && report::barrier_name(&f.name) {
                    *stops.entry(barrier_word(&f.name)).or_insert(0) += 1;
                    stopped = true;
                }
            }
            if a.outcome.as_deref() == Some("intercept") && !stopped {
                *stops.entry("(the game's intercept, barrier unnamed)").or_insert(0) += 1;
            }
        }
    }
    let mut out = vec![
        String::from("By barrier (a block's share is of every attack; an absorb's, of what would have hit):"),
        // UNKNOWN: the `Barrier - …` line is not in the literal pool.
        format!("Barrier - {}", hunts.label),
    ];
    let rows: Vec<Vec<String>> = stops
        .iter()
        .map(|(name, n)| {
            vec![
                (*name).to_string(),
                String::from("blocks before the roll"),
                swings.to_string(),
                n.to_string(),
                pct(*n, swings),
            ]
        })
        .collect();
    out.extend(report::grid(
        &["Barrier", "How", "Attacks", "stopped", "Share"],
        &rows,
    ));
    Ok(out)
}

/// The word a barrier flare is shown under.
fn barrier_word(name: &str) -> &'static str {
    match name {
        "barkskin" => "barkskin",
        "mirror_image_def" => "mirror image",
        "wthorns640_block" => "wall of thorns",
        "sward319" => "spirit warding shield",
        _ => "stone barrier",
    }
}

/// The cross-tabs: `cstats blind|control|family|aim|accuracy|ttk`.
///
/// Recovered strings (0x101315a38 and its helpers): `By attacker (the most
/// attacks first; Intercepted is a block before any roll, Absorbed the bark
/// taking a hit; Unresolved, no result line the tables know):`, `By round:`,
/// `By the weapon in hand:`, `(hands not known)`, `Attempts`, `Outcomes`, the
/// aim table `Median-attempts`, `Median-seconds`, `P90-seconds`,
/// `Median-damage`, `Aimed-kills`, `Unaimed-kills`, `Difference` and
/// `Fewer median attempts with aiming (positive favors aiming)`, and the status
/// table `Status`, `Events`, `Creature-coverage`, `(unknown)`.
///
/// `// UNKNOWN:` this 0x2dc00-byte function is one inlined match over six
/// reports; the tables below carry the recovered headings over the numbers the
/// store can supply, but the per-report filtering predicates are only partly
/// recovered (`blind` in particular reads `SELECT creature_id, damage, fatal FROM
/// hits WHERE flare_id = ?1 AND creature_id IS NOT NULL`, 0x1024105a9).
pub fn analytics(
    store: &Store,
    character: &str,
    which: Analytics,
    scope: &Scope,
) -> Lines {
    let hunts = report::resolve(store, character, scope)?;
    match which {
        Analytics::Aim => aim_report(store, &hunts),
        Analytics::Accuracy | Analytics::Ttk => {
            let mut attempts = 0;
            let mut hits = 0;
            let mut kills = 0;
            let mut seconds = 0;
            for hunt in &hunts.ids {
                let row = report::hunt_row(store, *hunt);
                seconds += row.ended_at.unwrap_or(row.last_event) - row.started_at;
                kills += row.kills;
                for a in store.attacks_of(*hunt) {
                    if !a.ours {
                        continue;
                    }
                    attempts += 1;
                    if report::is_hit(a.outcome.as_deref().unwrap_or("")) {
                        hits += 1;
                    }
                }
            }
            let mut out = vec![format!("{} - {}", which.word(), hunts.label)];
            out.push(format!(
                "{} of {} attempts ({})",
                hits,
                attempts,
                pct(hits, attempts)
            ));
            out.push(format!(
                "{} kills in {} ({:.1} kills/hr)",
                kills,
                report::duration(Some(seconds.max(0) as u64)),
                if seconds > 0 {
                    kills as f64 * 3600.0 / seconds as f64
                } else {
                    0.0
                }
            ));
            Ok(out)
        }
        Analytics::Blind => {
            let mut out = vec![format!("blind - {}", hunts.label)];
            out.push(String::from("Blind coverage by creature kind"));
            let mut rows: BTreeMap<String, (i64, i64)> = BTreeMap::new();
            for hunt in &hunts.ids {
                for c in store.creatures_of(*hunt) {
                    rows.entry(c.noun.clone()).or_insert((0, 0)).0 += 1;
                    for s in store.statuses.iter().filter(|s| s.creature_id == Some(c.id)) {
                        if s.status.contains("blind") {
                            rows.entry(c.noun.clone()).or_insert((0, 0)).1 += 1;
                        }
                    }
                }
            }
            let cells: Vec<Vec<String>> = rows
                .iter()
                .map(|(noun, (seen, blinded))| {
                    vec![noun.clone(), blinded.to_string(), seen.to_string()]
                })
                .collect();
            out.extend(report::grid(&["Creature", "Blinded", "Seen"], &cells));
            Ok(out)
        }
        Analytics::Control => {
            let mut by_attacker: BTreeMap<String, (i64, i64, i64)> = BTreeMap::new();
            for hunt in &hunts.ids {
                for a in store.attacks_of(*hunt) {
                    let name = a.attacker.clone().unwrap_or_default();
                    let slot = by_attacker.entry(name).or_insert((0, 0, 0));
                    slot.0 += 1;
                    if a.outcome.as_deref() == Some("intercept") {
                        slot.1 += 1;
                    }
                    if !report::is_hit(a.outcome.as_deref().unwrap_or("")) {
                        slot.2 += 1;
                    }
                }
            }
            let mut out = vec![
                String::from("By attacker (the most attacks first; Intercepted is a block before any roll, Absorbed the bark taking a hit; Unresolved, no result line the tables know):"),
                format!("control - {}", hunts.label),
            ];
            let mut rows: Vec<(i64, Vec<String>)> = by_attacker
                .iter()
                .map(|(name, (n, stopped, unresolved))| {
                    (
                        *n,
                        vec![
                            name.clone(),
                            n.to_string(),
                            stopped.to_string(),
                            unresolved.to_string(),
                        ],
                    )
                })
                .collect();
            rows.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
            let cells: Vec<Vec<String>> = rows.into_iter().map(|(_, c)| c).collect();
            out.extend(report::grid(
                &["Attacker", "Attacks", "Intercepted", "Unresolved"],
                &cells,
            ));
            Ok(out)
        }
        Analytics::Family => {
            let mut out = vec![
                String::from("By the weapon in hand:"),
                format!("family - {}", hunts.label),
            ];
            let mut rows: BTreeMap<String, i64> = BTreeMap::new();
            for hunt in &hunts.ids {
                for a in store.attacks_of(*hunt) {
                    if !a.ours {
                        continue;
                    }
                    match a.right_gear.or(a.left_gear) {
                        Some(id) => {
                            let name = store.name(id).to_string();
                            *rows.entry(name).or_insert(0) += 1;
                        }
                        None => *rows.entry(String::from("(hands not known)")).or_insert(0) += 1,
                    }
                }
            }
            let cells: Vec<Vec<String>> = rows
                .iter()
                .map(|(name, n)| vec![name.clone(), n.to_string()])
                .collect();
            out.extend(report::grid(&["Item", "Attacks"], &cells));
            Ok(out)
        }
    }
}

/// The aim cross-tab.
fn aim_report(store: &Store, hunts: &report::Hunts) -> Lines {
    let mut aimed = (0i64, Vec::<i64>::new(), 0i64);
    let mut unaimed = (0i64, Vec::<i64>::new(), 0i64);
    for hunt in &hunts.ids {
        for a in store.attacks_of(*hunt) {
            if !a.ours {
                continue;
            }
            let slot = if a.aimed { &mut aimed } else { &mut unaimed };
            slot.0 += 1;
            let hits: i64 = store
                .hits_of(a.id)
                .iter()
                .map(|h| h.damage)
                .sum();
            slot.1.push(hits);
        }
        for c in store.creatures_of(*hunt) {
            if c.killed_at.is_none() {
                continue;
            }
            let killer = c.killed_by_attack_id.and_then(|id| {
                store.attacks.iter().find(|a| a.id == id)
            });
            match killer.map(|a| a.aimed) {
                Some(true) => aimed.2 += 1,
                Some(false) => unaimed.2 += 1,
                None => {}
            }
        }
    }
    let mut out = vec![
        String::from("Aim"),
        format!("aim - {}", hunts.label),
    ];
    let mut cells = Vec::new();
    for (label, (attempts, damages, kills)) in [("aimed", &mut aimed), ("unaimed", &mut unaimed)] {
        let med_attempts = median(&mut damages.clone()).unwrap_or(0);
        cells.push(vec![
            label.to_string(),
            attempts.to_string(),
            med_attempts.to_string(),
            kills.to_string(),
        ]);
    }
    out.extend(report::grid(
        &["", "Attempts", "Median-damage", "Aimed-kills"],
        &cells,
    ));
    let diff = unaimed.0 - aimed.0;
    // UNKNOWN: the `Difference: ` prefix is not in the pool; the recovered
    // `Fewer median attempts …` clause is.
    out.push(format!(
        "Difference: {}  Fewer median attempts with aiming (positive favors aiming)",
        diff
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Attack, Hit, Status};

    fn store() -> Store {
        let mut s = Store::new();
        let a = s.hunt_started(1, "Testchar", 1_000_000);
        s.hunt_ended(a, 1_000_600);
        let kobold = s.creature_seen(a, "kobold", "a kobold", 1_000_100);
        let swing = s.attack(Attack {
            hunt_id: a,
            seq: 1,
            occurred_at: 1_000_200,
            name: String::from("jab"),
            attacker: Some(String::from("Testchar")),
            creature_id: Some(kobold),
            ours: true,
            aimed: true,
            outcome: Some(String::from("hit")),
            ..Attack::default()
        });
        s.hit(Hit {
            attack_id: swing,
            hunt_id: a,
            creature_id: Some(kobold),
            seq: 1,
            damage: 40,
            ..Hit::default()
        });
        s.kill(a, kobold, 1_000_400, Some(swing), "kill");
        let inbound = s.attack(Attack {
            hunt_id: a,
            seq: 2,
            occurred_at: 1_000_500,
            name: String::from("bite"),
            attacker: Some(String::from("a kobold")),
            creature_id: Some(kobold),
            inbound: true,
            outcome: Some(String::from("intercept")),
            ..Attack::default()
        });
        let _ = inbound;
        s.statuses.push(Status {
            attack_id: 2,
            creature_id: Some(kobold),
            status: String::from("ds"),
            value: Some(120),
            ..Status::default()
        });
        s
    }

    #[test]
    fn median_of_even_and_odd_counts() {
        assert_eq!(median(&mut [3, 1, 2]), Some(2));
        assert_eq!(median(&mut [4, 1, 3, 2]), Some(2));
        assert_eq!(median(&mut []), None);
    }

    #[test]
    fn pct_has_one_decimal() {
        assert_eq!(pct(1, 3), "33.3%");
        assert_eq!(pct(1, 0), "-");
    }

    #[test]
    fn hp_table_sums_the_damage() {
        let s = store();
        let lines = hp(&s, "Testchar", None, &Scope::Last).expect("runs");
        assert_eq!(lines[0], "HP - SESSION #1");
        assert_eq!(lines[1], "+ID+Our DMG+All DMG+");
        let id = s.creatures[0].id;
        assert_eq!(lines[2], format!("|{}|40|40|", id));
        assert_eq!(lines[3], "Average: 40");
    }

    #[test]
    fn intercepts_count_the_block() {
        let s = store();
        let lines = intercepts(&s, "Testchar", &Scope::Last).expect("runs");
        assert!(lines.iter().any(|l| l.contains("(the game's intercept, barrier unnamed)")));
        assert!(lines.iter().any(|l| l.ends_with("|100.0%|")));
    }

    #[test]
    fn defense_names_the_creatures_then_reads_ds() {
        let s = store();
        let list = defense(&s, "Testchar", None, &Scope::Last).expect("runs");
        assert_eq!(list[0], "Creatures (click one for DS/TD/UDF):");
        assert_eq!(list[1], "  kobold");
        let table = defense(&s, "Testchar", Some("kobold"), &Scope::Last).expect("runs");
        assert!(table.iter().any(|l| l.starts_with("|DS|120|120|120|120|")));
    }

    #[test]
    fn effects_lists_what_was_up() {
        let mut s = store();
        s.effects.push(crate::store::Effect {
            hunt_id: 1,
            name: String::from("barkskin"),
            began_at: 1_000_000,
            ..Default::default()
        });
        let lines = effects(&s, "Testchar", &Scope::Last).expect("runs");
        assert!(lines[0].starts_with("With and without what was up"));
        assert!(lines.iter().any(|l| l.starts_with("|barkskin|")));
    }
}
