//! The paged attack and creature listings, and the word grammar they share.

use crate::report::{self, Scope};
use crate::store::Store;
use crate::{Error, Lines};

/// How a listing is ordered.
///
/// The two order words the binary splices into its SQL are
/// `c.last_seen DESC, c.id DESC` (newest first) and `dmg DESC, c.id DESC`
/// (by damage) — 0x102413e5a / 0x102413e75 — matching the two heading words
/// `newest first` and `by damage` (0x102413ede).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Newest,
    Damage,
}

impl Order {
    pub fn word(self) -> &'static str {
        match self {
            Order::Newest => "newest first",
            Order::Damage => "by damage",
        }
    }
}

/// The options a listing carries.
///
/// `Listing::words` (0x101381a9c) spells them `recent {}`, `page {}` and
/// `per {}`; `Listing::from_words` (0x10138154c) parses the same three (and
/// hands the rest to `command::scope_from`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub scope: Scope,
    /// 1-based page number.
    pub page: usize,
    /// Rows per page, `0` meaning "no limit".
    pub per: usize,
    /// `recent <n>` — only the last *n* rows.
    pub recent: usize,
    pub order: Order,
}

impl Default for Listing {
    fn default() -> Listing {
        Listing {
            scope: Scope::default(),
            page: 1,
            per: 0,
            recent: 0,
            order: Order::default(),
        }
    }
}

impl Listing {
    /// The words this listing was built from: command fragments, each already
    /// carrying its argument (`recent 5`, `page 2`, `per 20`), so joining them
    /// with spaces rebuilds the `cstats` tail.
    pub fn words(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.recent > 0 {
            out.push(format!("recent {}", self.recent));
        }
        if self.page > 1 {
            out.push(format!("page {}", self.page));
        }
        if self.per > 0 {
            out.push(format!("per {}", self.per));
        }
        out
    }

    /// Read a listing back out of its words.
    ///
    /// `// UNKNOWN:` `from_words` also calls `command::scope_from`, so the
    /// session scope comes out of the same words; which words select the order
    /// is not visible at the call site, so only `recent`, `page`, `per` and the
    /// two order words are consumed here.
    pub fn from_words(words: &[String]) -> Option<Listing> {
        let mut listing = Listing::default();
        let mut i = 0;
        while i < words.len() {
            match words[i].as_str() {
                "recent" => {
                    listing.recent = words.get(i + 1)?.parse().ok()?;
                    i += 2;
                }
                "page" => {
                    listing.page = words.get(i + 1)?.parse().ok()?;
                    i += 2;
                }
                "per" => {
                    listing.per = words.get(i + 1)?.parse().ok()?;
                    i += 2;
                }
                "damage" | "dmg" => {
                    listing.order = Order::Damage;
                    i += 1;
                }
                "newest" => {
                    listing.order = Order::Newest;
                    i += 1;
                }
                _ => i += 1,
            }
        }
        listing.scope = crate::command::scope_from(words).unwrap_or(Scope::Last);
        Some(listing)
    }
}

/// The page number quoted in a title such as
/// `Attacks by damage - ALL 3 SESSIONS (page 2 of 5, 20 a page, 93 rows)`.
///
/// Recovered from 0x1013811dc: the title is split on `(page ` (0x102413e24), the
/// part after it is split on `' '` or `','` (the char pair `,` and ` ` the
/// binary builds), and the first field is parsed as a `usize`.  Anything else —
/// including a title with no `(page ` — is `None`.
pub fn paged(title: &str) -> Option<usize> {
    let rest = title.split("(page ").nth(1)?;
    let field = rest.split([' ', ',']).next()?;
    field.parse().ok()
}

/// Slice `rows` down to the page the listing asks for, returning the page slice,
/// the page number and the page count.  `per == 0` means one page.
fn paginate(rows: Vec<Vec<String>>, listing: &Listing) -> (Vec<Vec<String>>, usize, usize) {
    let per = if listing.per == 0 {
        rows.len().max(1)
    } else {
        listing.per
    };
    let pages = rows.len().div_ceil(per).max(1);
    let page = listing.page.clamp(1, pages);
    let start = (page - 1) * per;
    let end = (start + per).min(rows.len());
    (rows[start.min(rows.len())..end].to_vec(), page, pages)
}

/// `SELECT DISTINCT f.name FROM flares f JOIN attacks a ON a.id = f.attack_id
/// WHERE a.hunt_id {} AND f.ours = 1 ORDER BY f.name` (0x103e686b0).
pub fn flare_names(store: &Store, character: &str, scope: &Scope) -> Result<Vec<String>, Error> {
    let hunts = report::resolve(store, character, scope)?;
    // `SELECT DISTINCT f.name FROM flares f JOIN attacks a ON a.id = f.attack_id
    // WHERE a.hunt_id IN (…) AND f.ours = 1 ORDER BY f.name`, executed by SQLite
    // with the recovered `IN ({})` text spliced in.
    store.flare_names(&hunts.ids)
}

/// The attack listing.
///
/// Recovered from 0x101381d94: the counts come from
/// `SELECT COUNT(DISTINCT name) FROM attacks a WHERE a.hunt_id {} AND a.ours = 1`
/// (0x103e68949) and `SELECT COUNT(*) …` (0x103e68736); the titles are
/// `Attacks, newest first - {} ({})` (0x103e68928) and
/// `Attacks by damage - {} ({})` (0x103e68bc1); paging prints
/// `page {} of {}, {} a page, {} rows` (0x103e68bf7); the grid heads are
/// `Attack Dmg Outcome Flares Target Avg` (0x102413df0) and the empty case is
/// `No attacks recorded.`
pub fn attacks(store: &Store, character: &str, listing: &Listing) -> Lines {
    const EMPTY: &str = "No attacks recorded.";
    let hunts = report::resolve(store, character, &listing.scope)?;
    let mut rows: Vec<(i64, Vec<String>)> = Vec::new();
    for id in &hunts.ids {
        for a in store.attacks_of(*id)? {
            if !a.ours {
                continue;
            }
            let hits = store.hits_of(a.id)?;
            let damage: i64 = hits.iter().map(|h| h.damage).sum();
            let flares = store.flares_of(a.id)?.len();
            let target = match a.creature_id {
                Some(c) => store
                    .creature_by_id(c)?
                    .map(|c| c.name)
                    .unwrap_or_else(|| String::from("no target")),
                None => String::from("no target"),
            };
            let avg = if hits.is_empty() {
                String::from("-")
            } else {
                (damage / hits.len() as i64).to_string()
            };
            rows.push((
                a.occurred_at,
                vec![
                    a.name.clone(),
                    damage.to_string(),
                    a.outcome
                        .clone()
                        .unwrap_or_else(|| String::from("no outcome recorded")),
                    flares.to_string(),
                    target,
                    avg,
                ],
            ));
        }
    }
    if listing.recent > 0 && rows.len() > listing.recent {
        rows = rows.split_off(rows.len() - listing.recent);
    }
    if rows.is_empty() {
        return Ok(vec![String::from(EMPTY)]);
    }
    match listing.order {
        Order::Newest => rows.sort_by(|a, b| b.0.cmp(&a.0)),
        Order::Damage => rows.sort_by_key(|(_, cells)| std::cmp::Reverse(cells[1].parse::<i64>().unwrap_or(0))),
    }
    let total = rows.len();
    let cells: Vec<Vec<String>> = rows.into_iter().map(|(_, c)| c).collect();
    let (page, page_no, pages) = paginate(cells, listing);
    let mut out = Vec::new();
    out.push(match listing.order {
        Order::Newest => format!(
            "Attacks, newest first - {} ({})",
            hunts.label, "cstats attacks"
        ),
        Order::Damage => format!("Attacks by damage - {} ({})", hunts.label, "cstats attacks"),
    });
    if listing.recent > 0 {
        out.push(format!("the last {}", listing.recent));
    }
    out.push(format!(
        "page {} of {}, {} a page, {} rows",
        page_no,
        pages,
        if listing.per == 0 { total } else { listing.per },
        total
    ));
    out.extend(report::grid(
        &["Attack", "Dmg", "Outcome", "Flares", "Target", "Avg"],
        &page,
    ));
    Ok(out)
}

/// The creature listing.
///
/// Recovered from 0x101382a94: `SELECT COUNT(*) FROM creatures c WHERE c.hunt_id
/// {} AND c.exist_id IS NOT NULL` (0x103e68c1a) and a per-creature damage sum
/// filtered with `(a.ours = 1 OR EXISTS (SELECT 1 FROM flares f WHERE f.id =
/// h.flare_id AND f.ours = 1))` (0x102413e75); the title is
/// `Creatures, {} - {} ({})` (0x103e68e10), `the last {}` (0x103e68778) limits
/// the rows, the empty case is `No creatures recorded.` and the grid heads are
/// `ID Status Attacks Damage` (0x102413ede).
pub fn creatures(store: &Store, character: &str, listing: &Listing) -> Lines {
    const EMPTY: &str = "No creatures recorded.";
    let hunts = report::resolve(store, character, &listing.scope)?;
    let mut rows: Vec<(i64, Vec<String>)> = Vec::new();
    for id in &hunts.ids {
        for c in store.creatures_of(*id)? {
            // The own swings at the creature and the damage they did, counted by
            // SQLite rather than by walking every attack and hit in memory.
            let attacks = store.creature_swings(c.id)?;
            let damage = store.creature_damage(c.id)?;
            let status = match c.kill_credit.as_deref() {
                Some("kill") => String::from("DEAD (kill)"),
                Some("assist") => String::from("DEAD (assist)"),
                _ if c.killed_at.is_some() => String::from("DEAD"),
                _ => String::from("ALIVE"),
            };
            rows.push((
                c.last_seen,
                vec![
                    c.id.to_string(),
                    status,
                    attacks.to_string(),
                    damage.to_string(),
                ],
            ));
        }
    }
    if listing.recent > 0 && rows.len() > listing.recent {
        rows = rows.split_off(rows.len() - listing.recent);
    }
    if rows.is_empty() {
        return Ok(vec![String::from(EMPTY)]);
    }
    match listing.order {
        Order::Newest => rows.sort_by(|a, b| b.0.cmp(&a.0)),
        Order::Damage => rows.sort_by_key(|(_, cells)| std::cmp::Reverse(cells[3].parse::<i64>().unwrap_or(0))),
    }
    let total = rows.len();
    let cells: Vec<Vec<String>> = rows.into_iter().map(|(_, c)| c).collect();
    let (page, page_no, pages) = paginate(cells, listing);
    let mut out = Vec::new();
    out.push(format!(
        "Creatures, {} - {} ({})",
        page_no,
        pages,
        listing.order.word()
    ));
    if listing.recent > 0 {
        out.push(format!("the last {}", listing.recent));
    }
    out.push(format!(
        "page {} of {}, {} a page, {} rows",
        page_no,
        pages,
        if listing.per == 0 { total } else { listing.per },
        total
    ));
    out.extend(report::grid(&["ID", "Status", "Attacks", "Damage"], &page));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paged_reads_the_title() {
        assert_eq!(
            paged("Attacks by damage - ALL 3 SESSIONS (page 2 of 5, 20 a page, 93 rows)"),
            Some(2)
        );
        assert_eq!(
            paged("Attacks, newest first - SESSION #1 (page 1 of 1, 3 a page, 3 rows)"),
            Some(1)
        );
        assert_eq!(paged("No attacks recorded."), None);
        assert_eq!(paged("(page x)"), None);
    }

    #[test]
    fn words_round_trip() {
        let listing = Listing {
            recent: 5,
            page: 2,
            per: 20,
            ..Listing::default()
        };
        assert_eq!(
            listing.words(),
            vec![
                "recent 5".to_string(),
                "page 2".to_string(),
                "per 20".to_string()
            ]
        );
        let words: Vec<String> = listing
            .words()
            .iter()
            .flat_map(|f| f.split(' ').map(|w| w.to_string()).collect::<Vec<_>>())
            .collect();
        let back = Listing::from_words(&words).expect("parses");
        assert_eq!(back.recent, 5);
        assert_eq!(back.page, 2);
        assert_eq!(back.per, 20);
        assert_eq!(back.scope, Scope::Last);
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;
    use crate::store::{Attack, Creature, Flare, Hit};

    fn store() -> Store {
        let mut s = Store::new();
        let a = s.hunt_started(1, "Testchar", 1_000_000).expect("hunt");
        s.hunt_ended(a, 1_000_600).expect("ended");
        let kobold = s
            .creature_seen(a, "kobold", "a kobold", 1_000_100)
            .expect("creature");
        let swing = s.attack(Attack {
            hunt_id: a,
            seq: 1,
            occurred_at: 1_000_200,
            name: String::from("jab"),
            attacker: Some(String::from("Testchar")),
            creature_id: Some(kobold),
            ours: true,
            outcome: Some(String::from("hit")),
            ..Attack::default()
        })
        .expect("attack");
        s.hit(Hit {
            attack_id: swing,
            hunt_id: a,
            creature_id: Some(kobold),
            seq: 1,
            damage: 25,
            ..Hit::default()
        })
        .expect("hit");
        s.flare(Flare {
            attack_id: swing,
            seq: 1,
            name: String::from("acid_flare"),
            ours: true,
            ..Flare::default()
        })
        .expect("flare");
        s.flare(Flare {
            attack_id: swing,
            seq: 2,
            name: String::from("mirror_image_def"),
            ours: true,
            ..Flare::default()
        })
        .expect("flare");
        s.creature(Creature {
            hunt_id: a,
            ..Creature::default()
        })
        .expect("creature");
        s
    }

    #[test]
    fn attacks_listing_reports_the_row() {
        let s = store();
        let listing = Listing::default();
        let lines = attacks(&s, "Testchar", &listing).expect("runs");
        assert_eq!(
            lines[0],
            "Attacks, newest first - SESSION #1 (cstats attacks)"
        );
        assert!(lines
            .iter()
            .any(|l| l == "|jab|25|hit|2|a kobold|25|"));
    }

    #[test]
    fn attacks_listing_pages_and_limits() {
        let s = store();
        let listing = Listing {
            per: 1,
            page: 2,
            ..Listing::default()
        };
        let lines = attacks(&s, "Testchar", &listing).expect("runs");
        // One row only, so page two clamps to page one.
        assert!(lines[1].starts_with("page 1 of 1, 1 a page, 1 rows"));
    }

    #[test]
    fn creatures_listing_reports_the_status() {
        let s = store();
        let lines = creatures(&s, "Testchar", &Listing::default()).expect("runs");
        assert_eq!(lines[0], "Creatures, 1 - 1 (newest first)");
        let id = s.creatures_of(1).unwrap()[0].id;
        assert!(lines.iter().any(|l| l == &format!("|{}|ALIVE|1|25|", id)));
    }

    #[test]
    fn flare_names_are_sorted_and_unique() {
        let s = store();
        let names = flare_names(&s, "Testchar", &Scope::Last).expect("runs");
        assert_eq!(names, vec!["acid_flare", "mirror_image_def"]);
    }

    #[test]
    fn empty_listings_say_so() {
        let mut s = Store::new();
        let a = s.hunt_started(1, "Testchar", 1_000_000).expect("hunt");
        let _ = a;
        let listing = Listing::default();
        assert_eq!(
            attacks(&s, "Testchar", &listing).unwrap(),
            vec![String::from("No attacks recorded.")]
        );
        assert_eq!(
            creatures(&s, "Testchar", &listing).unwrap(),
            vec![String::from("No creatures recorded.")]
        );
    }
}
