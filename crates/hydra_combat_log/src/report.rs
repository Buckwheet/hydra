//! Session and aggregate reports, and the small pieces they are built from.
//!
//! Every literal here was decoded out of the packed `fmt::Arguments` blobs the
//! binary keeps in `__TEXT,__const` (`<len><text>` pieces, `0xC0`-tagged
//! placeholders), and every SQL statement quoted in a `const` was read from the
//! same section.  See `NOTES.md` §0 for the decoding rule.

use crate::store::{Attack, Store};
use crate::{eastern, Error, Lines};
use std::collections::BTreeMap;

/// Which sessions a command refers to.
///
/// Recovered from `report::resolve` (0x10135306c), which switches on the first
/// word of the value it is handed: `1` = one session by id, `2` = all of them,
/// `3` = the last *n*, `0` = the newest, anything else = a closed range.  The
/// same shape is produced by `command::scope_from` (0x101391784) from the words
/// `last [n]` and `between <a> <b>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// `cstats session` with no id: the newest session.
    #[default]
    Last,
    /// `cstats session <id>`
    One(i64),
    /// `cstats sessions all`
    All,
    /// `cstats sessions last <n>`
    LastN(i64),
    /// `cstats sessions between <a> <b>`
    Between(i64, i64),
}

/// The sessions a report ran over, plus the words that name them.
///
/// `resolve` returns both together (its `Ok` payload is 0x48 bytes: two
/// `String`s and a `Vec<i64>`), which is why every heading can print a scope
/// such as `LAST 3 SESSIONS (#7-#9)` without looking anything up again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hunts {
    /// The heading words, e.g. `ALL 3 SESSIONS`.
    pub label: String,
    /// The hunts this scope covers, in id order.
    pub ids: Vec<i64>,
}

impl Hunts {
    /// `IN (1,2,3)`, or `IN (NULL)` when the scope is empty.
    ///
    /// The empty case is not a formatted string in the binary: `sql_in`
    /// (0x101352698) allocates nine bytes and stores `IN (NULL)` (0x102412471)
    /// in them, so an empty scope selects nothing instead of being a syntax
    /// error.  A non-empty scope is `IN ({})` over the ids joined with `,`
    /// (0x10241247a).
    pub fn sql_in(&self) -> String {
        if self.ids.is_empty() {
            return String::from("IN (NULL)");
        }
        let joined: Vec<String> = self.ids.iter().map(|id| id.to_string()).collect();
        format!("IN ({})", joined.join(","))
    }
}

/// Inbound swings and how they ended.
///
/// `report::Inbound::said` (0x101353020) formats four numbers with
/// `<isize as Display>`, in the order the struct holds them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inbound {
    pub attacks: i64,
    pub landed: i64,
    pub missed: i64,
    pub intercepted: i64,
}

impl Inbound {
    /// `{} ({} landed, {} missed, {} intercepted)` (0x103e67ac7).
    pub fn said(&self) -> String {
        format!(
            "{} ({} landed, {} missed, {} intercepted)",
            self.attacks, self.landed, self.missed, self.intercepted
        )
    }
}

/// `{:02}:{:02}:{:02}` (0x10241247b) over the Eastern wall clock.
///
/// The arithmetic is the recovered one: local seconds (`at +
/// eastern::offset(at)`) reduced modulo a day, then `/3600`, `/60 % 60`, `% 60`.
pub fn clock(at: i64) -> String {
    let local = at + eastern::offset(at);
    let secs = local.rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// `{} {}` — the Eastern date, then [`clock`] (0x1013528f8).
pub fn stamp(at: i64) -> String {
    format!("{} {}", eastern::format(&eastern::date(at)), clock(at))
}

/// How long a session lasted.
///
/// Recovered from 0x101354e88: the argument is `Option<u64>` (the payload word is
/// only read when the tag is `1`); an open session prints `open`, anything under
/// a minute prints `{}s`, longer sessions print `{}m{:02}s` — so 125 seconds is
/// `2m05s` and 45 is `45s`.
pub fn duration(secs: Option<u64>) -> String {
    match secs {
        None => String::from("open"),
        Some(s) if s <= 59 => format!("{}s", s),
        Some(s) => format!("{}m{:02}s", s / 60, s % 60),
    }
}

/// `12/34 (35.3%)`, or `12/0 (-)` when the whole is zero (0x101354f60).
pub fn fraction(part: u64, whole: u64) -> String {
    if whole == 0 {
        format!("{}/{} (-)", part, whole)
    } else {
        format!(
            "{}/{} ({:.1}%)",
            part,
            whole,
            part as f64 * 100.0 / whole as f64
        )
    }
}

/// A `+`/`|` bordered table.
///
/// Recovered from 0x101351d38: the heading cells are joined with `+` and wrapped
/// in `+…+` (0x103e678e3), every body row is joined with `|` and wrapped in
/// `|…|` (0x103e678e9).
///
/// `// UNKNOWN:` the original builds the cells from `String`s it formats with
/// widths before they reach `grid`; the packing of those specs is not
/// recoverable, so cells are emitted exactly as handed over.
pub fn grid(heads: &[&str], rows: &[Vec<String>]) -> Vec<String> {
    let mut out = Vec::with_capacity(rows.len() + 1);
    out.push(format!("+{}+", heads.join("+")));
    for row in rows {
        out.push(format!("|{}|", row.join("|")));
    }
    out
}

/// A heading line followed by its table.
///
/// Recovered from 0x1013539bc, which builds a one-element `Vec<String>` holding
/// `title`, calls [`grid`] on the rest and appends it.
pub fn section(title: &str, heads: &[&str], rows: &[Vec<String>]) -> Vec<String> {
    let mut out = Vec::with_capacity(rows.len() + 2);
    out.push(title.to_string());
    out.extend(grid(heads, rows));
    out
}

/// The scope words for `scope` over every session of `character`.
///
/// Recovered from 0x10135306c:
///
/// * `Scope::All`   -> `ALL {} SESSIONS` (0x103e67af1)
/// * `Scope::Last`  -> `SESSION #{}` (0x103e67b02)
/// * `Scope::LastN` -> `LAST {} SESSIONS (#{}-#{})` (0x103e67b0e)
/// * `Scope::Between` -> `SESSIONS #{}-#{} ({})` (0x103e67b2a)
///
/// The session ids come from `SELECT id FROM hunts WHERE instance = ?1 AND
/// character = ?2 ORDER BY id` (0x102412509).
pub fn resolve(store: &Store, character: &str, scope: &Scope) -> Result<Hunts, Error> {
    // The statement 0x10135306c prepares through the statement cache —
    // `Store::hunts_of`, executed by SQLite.
    let ids = store.hunts_of(store.instance, character)?;
    if ids.is_empty() {
        return Err(Error::new("No hunts recorded yet."));
    }
    let count = ids.len() as i64;
    let (label, picked) = match scope {
        Scope::All => (format!("ALL {} SESSIONS", count), ids),
        Scope::Last => {
            let last = *ids.last().unwrap();
            (format!("SESSION #{}", last), vec![last])
        }
        Scope::One(id) => {
            if !ids.contains(id) {
                return Err(Error::new("No matching hunt."));
            }
            (format!("SESSION #{}", id), vec![*id])
        }
        Scope::LastN(n) => {
            let want = if *n < 0 { 0 } else { (*n).min(count) } as usize;
            let taken = ids[count as usize - want..].to_vec();
            let (first, last) = (taken[0], taken[taken.len() - 1]);
            (
                format!("LAST {} SESSIONS (#{}-#{})", want, first, last),
                taken,
            )
        }
        Scope::Between(a, b) => {
            let (lo, hi) = if a <= b { (*a, *b) } else { (*b, *a) };
            let taken: Vec<i64> = ids
                .iter()
                .copied()
                .filter(|id| *id >= lo && *id <= hi)
                .collect();
            if taken.is_empty() {
                return Err(Error::new("No matching hunt."));
            }
            (
                format!(
                    "SESSIONS #{}-#{} ({})",
                    taken[0],
                    taken[taken.len() - 1],
                    taken.len()
                ),
                taken,
            )
        }
    };
    Ok(Hunts {
        label,
        ids: picked,
    })
}

/// The outcome words the recorder stores (`hit evade block parry warded
/// ward_failed intercept unaffected fumble hindrance already_dead`, 0x102413321c).
pub(crate) fn is_hit(outcome: &str) -> bool {
    outcome == "hit"
}

/// Inbound counts for the selected sessions.
///
/// `// UNKNOWN:` the statement is `SELECT COUNT(*) FROM attacks a WHERE
/// a.hunt_id {} AND {} {}` (0x103e67557) with the two trailing predicates
/// supplied by the caller; the predicates are not recoverable from the call
/// site, so every inbound swing of the sessions is counted and classified by the
/// recovered outcome vocabulary.
pub fn inbound_of(store: &Store, hunts: &Hunts) -> Result<Inbound, Error> {
    let mut out = Inbound::default();
    for id in &hunts.ids {
        for a in store.attacks_of(*id)? {
            if !a.inbound {
                continue;
            }
            out.attacks += 1;
            match a.outcome.as_deref().unwrap_or("") {
                "hit" => out.landed += 1,
                "intercept" => out.intercepted += 1,
                _ => out.missed += 1,
            }
        }
    }
    Ok(out)
}

/// The flares that stop an attack before the roll.
///
/// `a.outcome = 'intercept' OR EXISTS (SELECT 1 FROM flares f WHERE
/// f.attack_id = a.id AND f.ours = 1 AND f.name IN ('barkskin',
/// 'mirror_image_def', 'wthorns640_block', 'sward319',
/// 'edefenseiii414_lorebenefit'))` — 0x10240ec2f.
pub(crate) fn barrier_name(name: &str) -> bool {
    matches!(
        name,
        "barkskin"
            | "mirror_image_def"
            | "wthorns640_block"
            | "sward319"
            | "edefenseiii414_lorebenefit"
    )
}

/// One row of the session list.
///
/// `report::hunt_row` (0x101354ff8) runs
/// `SELECT id, started_at, ended_at, (SELECT MAX(occurred_at) …)` and reads four
/// columns through `Row::get`.
///
/// `// UNKNOWN:` the four `{}`s of the row are `{:}` placeholders whose spec
/// bytes do not decode, so the column order below is inferred from the heading
/// constants that sit next to the format (`ID Status Attacks Damage`,
/// `Ended At Kills/hr` — 0x102413ede, 0x102412af8).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HuntRow {
    pub id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub last_event: i64,
    pub kills: i64,
    pub other_dead: i64,
    pub alive: i64,
    pub attacks: i64,
    pub damage: i64,
}

/// The heading words of the session list.
pub const HUNT_HEADS: [&str; 8] = [
    "#", "Started", "Ended", "Kills", "Other-dead", "Alive", "Attacks", "Damage",
];

/// Add up one session's row.
pub fn hunt_row(store: &Store, hunt_id: i64) -> Result<HuntRow, Error> {
    let hunt = match store.hunt(hunt_id)? {
        Some(h) => h,
        None => return Ok(HuntRow::default()),
    };
    let mut row = HuntRow {
        id: hunt.id,
        started_at: hunt.started_at,
        ended_at: hunt.ended_at,
        last_event: hunt.started_at,
        ..HuntRow::default()
    };
    for a in store.attacks_of(hunt_id)? {
        row.last_event = row.last_event.max(a.occurred_at);
        if a.ours {
            row.attacks += 1;
        }
    }
    for c in store.creatures_of(hunt_id)? {
        row.last_event = row.last_event.max(c.last_seen);
        match c.kill_credit.as_deref() {
            Some("kill") => row.kills += 1,
            Some("assist") => row.other_dead += 1,
            _ if c.killed_at.is_some() => row.other_dead += 1,
            _ => row.alive += 1,
        }
    }
    // `SELECT COALESCE(SUM(damage), 0) FROM hits WHERE hunt_id = ?1`, the sum
    // the recovered row reads through `Row::get`.
    row.damage = store.hunt_damage(hunt_id)?;
    Ok(row)
}

impl HuntRow {
    /// The cells of this row, in [`HUNT_HEADS`] order.
    pub fn cells(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            stamp(self.started_at),
            duration(
                self.ended_at
                    .map(|e| e.saturating_sub(self.started_at) as u64),
            ),
            self.kills.to_string(),
            self.other_dead.to_string(),
            self.alive.to_string(),
            self.attacks.to_string(),
            self.damage.to_string(),
        ]
    }
}

/// `Hunts - {} (cstats <id> for one):` and the session table.
///
/// Recovered from 0x101355504, which calls `resolve`, formats `cstats {}` per
/// row, calls `stamp` three times and ends with [`section`].
pub fn sessions(store: &Store, character: &str, scope: &Scope) -> Lines {
    const HEAD: &str = "Hunts - {} (cstats <id> for one):";
    let hunts = resolve(store, character, scope)?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for id in &hunts.ids {
        rows.push(hunt_row(store, *id)?.cells());
    }
    Ok(section(
        &HEAD.replace("{}", &hunts.label),
        &HUNT_HEADS,
        &rows,
    ))
}

/// The per-session bookkeeping behind [`session`].
///
/// `report::totals` (0x101352a90) runs four statements:
/// `SELECT COUNT(*), COUNT(DISTINCT COALESCE(a.root_attack_id, a.id)) FROM
/// attacks a WHERE a.hunt_id {} AND a.ours = 1` (0x103e6792c), the kill count
/// (0x103e67850), and the two damage sums (0x103e679ad, 0x103e67a27).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Totals {
    pub attacks: i64,
    pub sequences: i64,
    pub inbound: Inbound,
    pub kills: i64,
    pub assists: i64,
    pub dealt: i64,
    pub taken: i64,
}

/// Add up one session.
pub fn totals(store: &Store, hunt_id: i64) -> Result<Totals, Error> {
    let mut t = Totals::default();
    // The recovered pair of counts: the own swings and the sequences they form.
    let (attacks, sequences) = store.attack_and_sequence_counts(hunt_id)?;
    t.attacks = attacks;
    t.sequences = sequences;
    t.inbound = inbound_of(
        store,
        &Hunts {
            label: String::new(),
            ids: vec![hunt_id],
        },
    )?;
    for c in store.creatures_of(hunt_id)? {
        match c.kill_credit.as_deref() {
            Some("kill") => t.kills += 1,
            Some("assist") => t.assists += 1,
            _ => {}
        }
    }
    // The two damage sums, in one pass over the session's hits: a hit of an own
    // attack or of an own flare is dealt, everything else is taken.
    let (dealt, taken) = store.damage_split(hunt_id)?;
    t.dealt = dealt;
    t.taken = taken;
    Ok(t)
}

/// One session's report.
///
/// Recovered headings, in order (0x101353b18): `=== SESSION REPORT #{} ===`,
/// `Character: {}`, `Started: {}`, `Last Event: {}`, `Ended: {} ({})`,
/// `  Total Attacks: {}    Sequences: {}    Inbound: {}`,
/// `  Kills: {}    Assists: {}    Damage Dealt: {}    Damage Taken: {}`,
/// `CREATURES ENCOUNTERED ({}):` and the kit lines.
pub fn session(store: &Store, character: &str, scope: &Scope, full: bool) -> Lines {
    let hunts = match resolve(store, character, scope) {
        Ok(h) => h,
        Err(e) => {
            return match scope {
                Scope::One(id) => Err(Error::new(format!("No hunt #{}.", id))),
                _ => Err(e),
            }
        }
    };
    let id = hunts.ids[hunts.ids.len() - 1];
    let row = hunt_row(store, id)?;
    let mut out = Vec::new();
    out.push(format!("=== SESSION REPORT #{} ===", id));
    out.push(format!("Character: {}", character));
    out.push(format!("Started: {}", stamp(row.started_at)));
    out.push(format!("Last Event: {}", stamp(row.last_event)));
    out.push(match row.ended_at {
        Some(ended) => format!(
            "Ended: {} ({})",
            stamp(ended),
            duration(Some(ended.saturating_sub(row.started_at) as u64))
        ),
        // 0x102412550 holds the word the open case prints.
        None => String::from("Ended: ACTIVE"),
    });
    let t = totals(store, id)?;
    out.push(format!(
        "  Total Attacks: {}    Sequences: {}    Inbound: {}",
        t.attacks, t.sequences, t.inbound.attacks
    ));
    out.push(format!(
        "  Kills: {}    Assists: {}    Damage Dealt: {}    Damage Taken: {}",
        t.kills, t.assists, t.dealt, t.taken
    ));
    let creatures = store.creatures_of(id)?;
    if !full {
        out.push(format!("CREATURES ENCOUNTERED ({}):", creatures.len()));
    }
    if full {
        let rows: Vec<Vec<String>> = creatures
            .iter()
            .map(|c| {
                vec![
                    c.name.clone(),
                    c.noun.clone(),
                    match c.kill_credit.as_deref() {
                        Some("kill") => String::from("DEAD (kill)"),
                        Some("assist") => String::from("DEAD (assist)"),
                        _ if c.killed_at.is_some() => String::from("DEAD"),
                        _ => String::from("ALIVE"),
                    },
                    c.last_seen.to_string(),
                ]
            })
            .collect();
        out.extend(section(
            &format!("CREATURES ENCOUNTERED ({})", rows.len()),
            &["Creature", "Noun", "Status", "Seen"],
            &rows,
        ));
    }
    out.extend(kit_lines(store, id).unwrap_or_default());
    out.extend(quick_actions());
    Ok(out)
}

/// The effects line of a session: `  Up: {}`.
///
/// Recovered from 0x101350954: `SELECT n.name, e.began_at FROM hunt_effects e
/// JOIN names n ON n.id = e.name_id WHERE e.hunt_id = ?1 ORDER BY e.began_at`
/// (0x10241214d); when more than one effect came up the second line is
/// `  {} more came up during the hunt {}` (0x103e67788).
pub fn effects_line(store: &Store, hunt_id: i64) -> Lines {
    let mut names: Vec<String> = Vec::new();
    for effect in store.effects_of(hunt_id)? {
        if !names.iter().any(|n| n == &effect.name) {
            names.push(effect.name.clone());
        }
    }
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = vec![format!("  Up: {}", names.join(", "))];
    if names.len() > 1 {
        out.push(format!(
            "  {} more came up during the hunt {}",
            names.len() - 1,
            hunt_id
        ));
    }
    Ok(out)
}

/// `  Gemstones: {}`.
///
/// Recovered from 0x101351534: `SELECT description, name, rank, ranks FROM
/// hunt_gemstones WHERE hunt_id = ?1 ORDER BY number, id` (0x10241227f); each
/// row prints as `{} {}/{}` (0x103e67848).
pub fn gemstones_line(store: &Store, hunt_id: i64) -> Lines {
    let rows = store.gemstones_of(hunt_id)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let cells: Vec<String> = rows
        .iter()
        .map(|g| {
            if g.ranks > 0 {
                format!("{} {}/{}", g.description, g.rank, g.ranks)
            } else {
                g.description.clone()
            }
        })
        .collect();
    Ok(vec![format!("  Gemstones: {}", cells.join(", "))])
}

/// The worn-gear line.
///
/// Recovered from 0x1013581a8: `SELECT w.began_at, w.ended_at, n.name FROM
/// hunt_gear w JOIN names n ON n.id = w.gear_id WHERE w.hunt_id = ?1 AND
/// w.slot = 'worn' ORDER BY w.began_at, w.id` (0x102412a25) — the row type is
/// `(i64, Option<i64>, String)` — printing `  Worn: {}` and, for every later
/// change, `  Worn list changed at {}` (0x103e68245).
pub fn worn_line(store: &Store, hunt_id: i64) -> Lines {
    let rows = store.gear_of(hunt_id, "worn")?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let names: Vec<String> = rows.iter().map(|g| g.name.clone()).collect();
    let mut out = vec![format!("  Worn: {}", names.join(", "))];
    for g in rows.iter().skip(1) {
        out.push(format!("  Worn list changed at {}", stamp(g.began_at)));
    }
    Ok(out)
}

/// The three kit lines of a session.
///
/// Recovered from 0x101358010: `gemstones_line`, `worn_line`, `effects_line`, in
/// that order, each appended to the same vector.
pub fn kit_lines(store: &Store, hunt_id: i64) -> Lines {
    let mut out = gemstones_line(store, hunt_id)?;
    out.extend(worn_line(store, hunt_id)?);
    out.extend(effects_line(store, hunt_id)?);
    Ok(out)
}

/// The quick-action rows.
///
/// Recovered from 0x101350ee8: the ten command names are the recovered words
/// (`[sessions] cstats sessions`, `[all hunts] cstats all`, `[abilities] cstats
/// abilities`, `[attacks] cstats attacks`, `[creatures] cstats creatures`,
/// `cstats flare`, `[defenses] cstats def`, `[accuracy] cstats accuracy`, `[kill
/// efficiency] cstats ttk`, `[hp] cstats hp`), printed ten to a row with two
/// spaces between the cells (`  {:}  {:} …`, 0x103e6780f).
pub fn quick_actions() -> Vec<String> {
    const ACTIONS: [&str; 10] = [
        "[sessions] cstats sessions",
        "[all hunts] cstats all",
        "[abilities] cstats abilities",
        "[attacks] cstats attacks",
        "[creatures] cstats creatures",
        "cstats flare",
        "[defenses] cstats def",
        "[accuracy] cstats accuracy",
        "[kill efficiency] cstats ttk",
        "[hp] cstats hp",
    ];
    ACTIONS
        .chunks(10)
        .map(|chunk| format!("  {}", chunk.join("  ")))
        .collect()
}

/// `=== AGGREGATE REPORT - {} ===` and its body.
///
/// Recovered headings (0x101356848): the title, `Character: {}`,
/// `Hunts: {}    First: {}    Last: {}`, `Hunt time: {} (avg {} per hunt)`,
/// the inbound line, the kills line,
/// `  Per hour: {} kills    {} damage    {} attacks    {} taken`,
/// `CREATURES ENCOUNTERED ({}):` and `{} hunts; {} lists them.`
pub fn aggregate(store: &Store, character: &str, scope: &Scope) -> Lines {
    let hunts = resolve(store, character, scope)?;
    let mut out = Vec::new();
    out.push(format!("=== AGGREGATE REPORT - {} ===", hunts.label));
    out.push(format!("Character: {}", character));
    let first = store.hunt(hunts.ids[0])?.map(|h| h.started_at).unwrap_or(0);
    let mut last = first;
    for id in &hunts.ids {
        if let Some(h) = store.hunt(*id)? {
            last = last.max(h.ended_at.unwrap_or(h.started_at));
        }
    }
    let mut total_time = 0u64;
    let mut t = Totals::default();
    let mut creatures: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for id in &hunts.ids {
        let row = hunt_row(store, *id)?;
        if let Some(ended) = row.ended_at {
            total_time += ended.saturating_sub(row.started_at) as u64;
        }
        let one = totals(store, *id)?;
        t.attacks += one.attacks;
        t.sequences += one.sequences;
        t.kills += one.kills;
        t.assists += one.assists;
        t.dealt += one.dealt;
        t.taken += one.taken;
        t.inbound.attacks += one.inbound.attacks;
        t.inbound.landed += one.inbound.landed;
        t.inbound.missed += one.inbound.missed;
        t.inbound.intercepted += one.inbound.intercepted;
        for c in store.creatures_of(*id)? {
            let slot = creatures.entry(c.noun.clone()).or_insert((0, 0));
            slot.0 += 1;
            if c.killed_at.is_some() {
                slot.1 += 1;
            }
        }
    }
    let hunts_n = hunts.ids.len() as i64;
    out.push(format!(
        "Hunts: {}    First: {}    Last: {}",
        hunts_n,
        stamp(first),
        stamp(last)
    ));
    let avg = if hunts_n > 0 {
        Some(total_time / hunts_n as u64)
    } else {
        None
    };
    out.push(format!(
        "Hunt time: {} (avg {} per hunt)",
        duration(Some(total_time)),
        duration(avg)
    ));
    out.push(format!("  {}", t.inbound.said()));
    out.push(format!(
        "  Kills: {}    Assists: {}    Damage Dealt: {}    Damage Taken: {}",
        t.kills, t.assists, t.dealt, t.taken
    ));
    let hours = total_time as f64 / 3600.0;
    let per = |n: i64| {
        if hours == 0.0 {
            String::from("-")
        } else {
            format!("{:.1}", n as f64 / hours)
        }
    };
    out.push(format!(
        "  Per hour: {} kills    {} damage    {} attacks    {} taken",
        per(t.kills),
        per(t.dealt),
        per(t.attacks),
        per(t.taken)
    ));
    out.push(format!("CREATURES ENCOUNTERED ({}):", creatures.len()));
    let rows: Vec<Vec<String>> = creatures
        .iter()
        .map(|(noun, (seen, dead))| vec![noun.clone(), seen.to_string(), dead.to_string()])
        .collect();
    out.extend(grid(&["Creature", "Seen", "Dead"], &rows));
    out.push(format!("{} hunts; {} lists them.", hunts_n, "cstats sessions"));
    out.extend(quick_actions());
    Ok(out)
}

/// `=== ABILITIES - {} ===` and its three tables.
///
/// Recovered headings (0x101355d60): the title,
/// `Damage by ability (own swings; Landed = swings with a hit, Hits = damage
/// components):` with the grid `Ability Landed Dmg Avg Crits Fatal`
/// (0x10241268f), `Flares (own; cstats flare <name> for one):` with
/// `Flare Procs` (0x10241283f), and `Attacks against you:` with
/// `Attack N Missed Intercepted` (0x1024129e9).
pub fn abilities(store: &Store, character: &str, scope: &Scope) -> Lines {
    let hunts = resolve(store, character, scope)?;
    let mut out = Vec::new();
    out.push(format!("=== ABILITIES - {} ===", hunts.label));
    out.push(String::from(
        "Damage by ability (own swings; Landed = swings with a hit, Hits = damage components):",
    ));
    let mut by_name: BTreeMap<String, (i64, i64, i64, i64, i64)> = BTreeMap::new();
    let mut flares: BTreeMap<String, i64> = BTreeMap::new();
    let mut inbound: BTreeMap<String, (i64, i64, i64)> = BTreeMap::new();
    for id in &hunts.ids {
        for a in store.attacks_of(*id)? {
            for f in store.flares_of(a.id)? {
                if f.ours {
                    *flares.entry(f.name.clone()).or_insert(0) += 1;
                }
            }
            if a.inbound {
                let slot = inbound.entry(a.name.clone()).or_insert((0, 0, 0));
                slot.0 += 1;
                match a.outcome.as_deref().unwrap_or("") {
                    "hit" => slot.1 += 1,
                    "intercept" => slot.2 += 1,
                    _ => {}
                }
                continue;
            }
            if !a.ours {
                continue;
            }
            let hits = store.hits_of(a.id)?;
            let slot = by_name.entry(a.name.clone()).or_insert((0, 0, 0, 0, 0));
            slot.0 += 1;
            if !hits.is_empty() {
                slot.1 += 1;
            }
            for h in &hits {
                slot.2 += h.damage;
                if h.crit_rank.unwrap_or(0) > 0 {
                    slot.3 += 1;
                }
                if h.fatal {
                    slot.4 += 1;
                }
            }
        }
    }
    let rows: Vec<Vec<String>> = by_name
        .iter()
        .map(|(name, (swings, landed, damage, crits, fatal))| {
            let avg = if *landed > 0 {
                (damage / landed).to_string()
            } else {
                String::from("-")
            };
            vec![
                name.clone(),
                landed.to_string(),
                damage.to_string(),
                avg,
                crits.to_string(),
                fatal.to_string(),
            ]
        })
        .collect();
    out.extend(grid(
        &["Ability", "Landed", "Dmg", "Avg", "Crits", "Fatal"],
        &rows,
    ));
    out.push(String::from("Flares (own; cstats flare <name> for one):"));
    let rows: Vec<Vec<String>> = flares
        .iter()
        .map(|(name, procs)| vec![name.clone(), procs.to_string()])
        .collect();
    out.extend(grid(&["Flare", "Procs"], &rows));
    out.push(String::from("Attacks against you:"));
    let rows: Vec<Vec<String>> = inbound
        .iter()
        .map(|(name, (n, hit, stop))| {
            vec![
                name.clone(),
                n.to_string(),
                hit.to_string(),
                stop.to_string(),
            ]
        })
        .collect();
    out.extend(grid(&["Attack", "N", "Missed", "Intercepted"], &rows));
    out.extend(quick_actions());
    Ok(out)
}

/// `report::linked` (0x101352a84) is a twelve-byte tail call into
/// `<Vec<T> as SpecFromIter>::from_iter`, i.e. a collect over an already-built
/// list.
///
/// `// UNKNOWN:` the element type is `String` (the caller drops the result as
/// `Vec<String>` and hands it to `hydra_session::notice::Notice::linked`), so
/// this re-collects the same list.
pub fn linked(lines: &[String]) -> Vec<String> {
    lines.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Attack, Creature, Effect, Flare, Gemstone, GearRow, Hit};

    /// Three sessions for `Testchar`, the first of which holds one own swing, one
    /// inbound swing, a kill and a crit.
    fn store() -> Store {
        let mut s = Store::new();
        let a = s.hunt_started(1, "Testchar", 1_000_000).expect("hunt");
        let b = s.hunt_started(1, "Testchar", 2_000_000).expect("hunt");
        let c = s.hunt_started(1, "Testchar", 3_000_000).expect("hunt");
        s.hunt_started(1, "Other", 4_000_000).expect("hunt");
        s.hunt_ended(a, 1_000_600).expect("ended");
        s.hunt_ended(b, 2_001_200).expect("ended");
        let kobold = s.creature_seen(a, "kobold", "a kobold", 1_000_100).expect("creature");
        let swing = s.attack(Attack {
            hunt_id: a,
            seq: 1,
            occurred_at: 1_000_200,
            name: String::from("jab"),
            attacker: Some(String::from("Testchar")),
            creature_id: Some(kobold),
            ours: true,
            outcome: Some(String::from("hit")),
            root_attack_id: Some(0),
            ..Attack::default()
        })
        .expect("attack");
        s.hit(Hit {
            attack_id: swing,
            hunt_id: a,
            creature_id: Some(kobold),
            seq: 1,
            damage: 25,
            location: Some(String::from("left leg")),
            body_part: Some(String::from("left_leg")),
            crit_rank: Some(3),
            crit_type: Some(String::from("crush")),
            ..Hit::default()
        })
        .expect("hit");
        s.kill(a, kobold, 1_000_400, Some(swing), "kill")
            .expect("kill");
        let inbound = s.attack(Attack {
            hunt_id: a,
            seq: 2,
            occurred_at: 1_000_500,
            name: String::from("jab"),
            attacker: Some(String::from("a kobold")),
            creature_id: Some(kobold),
            inbound: true,
            outcome: Some(String::from("hit")),
            ..Attack::default()
        })
        .expect("attack");
        s.hit(Hit {
            attack_id: inbound,
            hunt_id: a,
            creature_id: Some(kobold),
            seq: 1,
            damage: 7,
            ..Hit::default()
        })
        .expect("hit");
        s.flare(Flare {
            attack_id: inbound,
            seq: 1,
            name: String::from("barkskin"),
            ours: true,
            eligible: true,
            ..Flare::default()
        })
        .expect("flare");
        s.effect(Effect {
            hunt_id: a,
            name_id: 0,
            name: String::from("strength"),
            began_at: 1_000_050,
            ended_at: Some(1_000_450),
            ..Effect::default()
        })
        .expect("effect");
        s.effect(Effect {
            hunt_id: a,
            name_id: 0,
            name: String::from("bravery"),
            began_at: 1_000_060,
            ended_at: Some(1_000_460),
            ..Effect::default()
        })
        .expect("effect");
        s.gear(GearRow {
            hunt_id: a,
            gear_id: 0,
            slot: String::from("worn"),
            name: String::from("a cuirbouilli coat"),
            began_at: 1_000_000,
            ..GearRow::default()
        })
        .expect("gear");
        s.gemstone(Gemstone {
            hunt_id: a,
            number: 1,
            description: String::from("a blue sapphire"),
            name: String::from("sapphire"),
            rank: 2,
            ranks: 4,
            ..Gemstone::default()
        })
        .expect("gemstone");
        s.creature(Creature {
            hunt_id: c,
            ..Creature::default()
        })
        .expect("creature");
        s
    }

    #[test]
    fn clock_is_eastern() {
        // 0 unix seconds is 19:00 the previous day in EST.
        assert_eq!(clock(0), "19:00:00");
        assert_eq!(clock(86_400), "19:00:00");
    }

    #[test]
    fn stamp_prints_date_then_clock() {
        assert_eq!(stamp(0), "1969-12-31 19:00:00");
    }

    #[test]
    fn duration_spells_open_seconds_and_minutes() {
        assert_eq!(duration(None), "open");
        assert_eq!(duration(Some(0)), "0s");
        assert_eq!(duration(Some(45)), "45s");
        assert_eq!(duration(Some(59)), "59s");
        assert_eq!(duration(Some(60)), "1m00s");
        assert_eq!(duration(Some(125)), "2m05s");
    }

    #[test]
    fn fraction_prints_a_share() {
        assert_eq!(fraction(1, 3), "1/3 (33.3%)");
        assert_eq!(fraction(3, 3), "3/3 (100.0%)");
        assert_eq!(fraction(1, 0), "1/0 (-)");
    }

    #[test]
    fn inbound_said_lists_the_four_counts() {
        let said = Inbound {
            attacks: 10,
            landed: 7,
            missed: 2,
            intercepted: 1,
        };
        assert_eq!(said.said(), "10 (7 landed, 2 missed, 1 intercepted)");
    }

    #[test]
    fn sql_in_joins_or_selects_nothing() {
        let hunts = Hunts {
            label: String::new(),
            ids: vec![1, 2, 3],
        };
        assert_eq!(hunts.sql_in(), "IN (1,2,3)");
        let empty = Hunts::default();
        assert_eq!(empty.sql_in(), "IN (NULL)");
    }

    #[test]
    fn grid_borders_its_cells() {
        let rows = vec![vec![String::from("1"), String::from("2")]];
        assert_eq!(grid(&["A", "B"], &rows), vec!["+A+B+", "|1|2|"]);
        assert_eq!(
            section("Title", &["A"], &rows),
            vec!["Title", "+A+", "|1|2|"]
        );
    }

    #[test]
    fn resolve_names_every_scope() {
        let s = store();
        assert_eq!(
            resolve(&s, "Testchar", &Scope::All).unwrap().label,
            "ALL 3 SESSIONS"
        );
        assert_eq!(
            resolve(&s, "Testchar", &Scope::Last).unwrap().ids,
            vec![3]
        );
        assert_eq!(
            resolve(&s, "Testchar", &Scope::One(2)).unwrap().label,
            "SESSION #2"
        );
        assert_eq!(
            resolve(&s, "Testchar", &Scope::LastN(2)).unwrap().label,
            "LAST 2 SESSIONS (#2-#3)"
        );
        assert_eq!(
            resolve(&s, "Testchar", &Scope::Between(1, 2)).unwrap().label,
            "SESSIONS #1-#2 (2)"
        );
        assert!(resolve(&s, "Nobody", &Scope::All).is_err());
        assert!(resolve(&s, "Testchar", &Scope::One(9)).is_err());
    }

    #[test]
    fn inbound_of_classifies_outcomes() {
        let s = store();
        let hunts = resolve(&s, "Testchar", &Scope::One(1)).unwrap();
        let inbound = inbound_of(&s, &hunts).unwrap();
        assert_eq!(inbound.attacks, 1);
        assert_eq!(inbound.landed, 1);
        assert_eq!(inbound.missed, 0);
        assert_eq!(inbound.intercepted, 0);
    }

    #[test]
    fn session_report_has_the_recovered_heading_and_numbers() {
        let s = store();
        let lines = session(&s, "Testchar", &Scope::One(1), false).unwrap();
        assert_eq!(lines[0], "=== SESSION REPORT #1 ===");
        assert_eq!(lines[1], "Character: Testchar");
        assert_eq!(lines[2], "Started: 1970-01-12 08:46:40");
        assert!(lines[4].starts_with("Ended: "));
        assert!(lines[4].ends_with("(10m00s)"));
        assert_eq!(
            lines[5],
            "  Total Attacks: 1    Sequences: 1    Inbound: 1"
        );
        assert_eq!(
            lines[6],
            "  Kills: 1    Assists: 0    Damage Dealt: 25    Damage Taken: 7"
        );
        assert_eq!(lines[7], "CREATURES ENCOUNTERED (1):");
    }

    #[test]
    fn sessions_table_lists_each_hunt() {
        let s = store();
        let lines = sessions(&s, "Testchar", &Scope::All).unwrap();
        assert_eq!(lines[0], "Hunts - ALL 3 SESSIONS (cstats <id> for one):");
        assert_eq!(lines[1], "+#+Started+Ended+Kills+Other-dead+Alive+Attacks+Damage+");
        assert!(lines[2].starts_with("|1|"));
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn kit_lines_report_gemstones_worn_and_effects() {
        let s = store();
        let lines = kit_lines(&s, 1).unwrap();
        assert_eq!(lines[0], "  Gemstones: a blue sapphire 2/4");
        assert_eq!(lines[1], "  Worn: a cuirbouilli coat");
        assert_eq!(lines[2], "  Up: strength, bravery");
        assert_eq!(lines[3], "  1 more came up during the hunt 1");
    }

    #[test]
    fn abilities_table_counts_swings_hits_and_crits() {
        let s = store();
        let lines = abilities(&s, "Testchar", &Scope::One(1)).unwrap();
        assert_eq!(lines[0], "=== ABILITIES - SESSION #1 ===");
        assert!(lines
            .iter()
            .any(|l| l == "|jab|1|25|25|1|0|"));
        assert!(lines.iter().any(|l| l == "|barkskin|1|"));
    }

    #[test]
    fn aggregate_adds_the_sessions_up() {
        let s = store();
        let lines = aggregate(&s, "Testchar", &Scope::All).unwrap();
        assert_eq!(lines[0], "=== AGGREGATE REPORT - ALL 3 SESSIONS ===");
        assert!(lines[2].starts_with("Hunts: 3    First: "));
        assert_eq!(lines[3], "Hunt time: 30m00s (avg 10m00s per hunt)");
        assert_eq!(lines[4], "  1 (1 landed, 0 missed, 0 intercepted)");
    }

    #[test]
    fn quick_actions_are_one_row_of_ten() {
        let lines = quick_actions();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("  [sessions] cstats sessions  [all hunts] cstats all"));
        assert!(lines[0].ends_with("[hp] cstats hp"));
    }

    #[test]
    fn hunt_row_counts_the_session() {
        let s = store();
        let row = hunt_row(&s, 1).expect("row");
        assert_eq!(row.id, 1);
        assert_eq!(row.attacks, 1);
        assert_eq!(row.kills, 1);
        assert_eq!(row.alive, 0);
        assert_eq!(row.damage, 32);
        assert_eq!(row.last_event, 1_000_500);
    }
}
