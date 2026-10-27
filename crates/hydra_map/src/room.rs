//! Finding a map room, and describing one.
//!
//! Both functions are about the map's room list: [`found`] searches it
//! (`0x1015b0bbc`) and [`described`] renders one room as the lines the UI shows
//! (`0x1015b11cc`).

use crate::cena::{Map, RoomId};

/// One search hit.
///
/// Built by `room::found::{{closure}}` (`0x1015b1090`), which fills exactly three
/// things: a string taken from the first of the room's area paths and trimmed of
/// `[`/`]` (`trim_matches` with the two-character pattern `['[', ']']` at
/// `0x1015b10cc`), the room's title (`+0x160`, cloned), and a `u32` read at
/// `+0x178`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// The room's outermost area path, without its brackets.
    pub path: String,
    pub title: String,
    /// The room's id (`+0x178`).
    pub id: u32,
}

/// Rooms matching `text`.
///
/// **Partly recovered.** Recovered:
///
/// * the input is `trim()`ed, an optional leading `u`/`U` is stripped, and the
///   rest is parsed as an `i64` with an optional sign (`0x1015b0bf4`..), so
///   `"u7116001"` and `"7116001"` are the same search;
/// * the input is then lower-cased (`0x1015b0cb8`) and every room is visited
///   (`madd x9, x9, #0x180, x8` — a 384-byte stride);
/// * the hits are sorted, and a small result is sorted with the insertion sort
///   (`0x1015b0ff4`).
///
/// Not recovered: the predicate that decides which rooms are kept, and the third
/// argument. Since the closure that builds each [`Found`] captures nothing, the
/// predicate was inlined into the collection loop; the third argument is stored
/// beside the lower-cased needle in the iterator environment. This build treats
/// it as `numbered` — the same flag [`described`] takes — and keeps a room when
/// its id equals the parsed number, or (when no number was given) its title,
/// area path or note contains every whitespace-separated word of the needle.
pub fn found(map: &Map, text: &str, numbered: bool) -> Vec<Found> {
    let needle = text.trim().to_lowercase();
    let number = parse_number(text);
    let words: Vec<&str> = needle.split_whitespace().collect();
    let mut hits: Vec<Found> = Vec::new();
    for room in map.rooms() {
        let keep = match number {
            Some(n) => i64::from(room.id) == n || (!numbered && matches_words(room, &words)),
            None => matches_words(room, &words),
        };
        if !keep {
            continue;
        }
        let path = room
            .areas
            .first()
            .map(|a| a.trim_matches(['[', ']']).to_string())
            .unwrap_or_default();
        hits.push(Found {
            path,
            title: room.title.clone(),
            id: room.id,
        });
    }
    hits.sort_by(|a, b| a.id.cmp(&b.id));
    hits
}

fn matches_words(room: &crate::cena::Room, words: &[&str]) -> bool {
    if words.iter().all(|w| w.is_empty()) {
        // An empty needle matches nothing: `found` returns early on an empty
        // trimmed input (`cbz x1, 0x1015b0c4c`).
        return false;
    }
    let mut hay = room.title.to_lowercase();
    hay.push(' ');
    hay.push_str(&room.areas.join(" ").to_lowercase());
    if let Some(note) = &room.note {
        hay.push(' ');
        hay.push_str(&note.to_lowercase());
    }
    words.iter().all(|w| hay.contains(w))
}

/// The lines that describe room `id`, or `None` when the map has no such room
/// (the `isize::MIN` niche at `0x1015b1238`).
///
/// **Partly recovered.** The two branches and the literal formats are exact:
///
/// * `numbered == true` → one line, `format!("#{} {}", id, title)`
///   (`0x103e2b401`: `01 23 c0 01 20 c0`);
/// * `numbered == false` → four or five lines:
///   `format!("#{} {}:", id, title)` (`0x103e6cba0`: `01 23 c0 c0 01 3a`),
///   `format!("{} {}", path_note, plain_note)` (`0x103e2252a`: `c0 c0`), then the
///   last element of the room's area `Vec<String>` (`+0x50`/`+0x58`) and the
///   last element of its spec `Vec<String>` (`+0x68`/`+0x70`), each cloned.
///
/// What is not exact is how the `path_note`/`plain_note` strings are built: the
/// `" (u{})"` format (`0x103e6cba7`: `03 20 28 75 c0 01 29`) is applied to the
/// last of the room's path ids, and `" ({})"` (`0x103e28dbe`: `02 20 28 c0 01
/// 29`) to an optional string on the room. Which of those two goes where is
/// inferred from the order the two `String`s are filled in the stack slots.
pub fn described(map: &Map, id: RoomId, numbered: bool) -> Option<Vec<String>> {
    let room = map.room(id)?;
    if numbered {
        return Some(vec![format!("#{} {}", id, room.title)]);
    }
    let path_note = match room.paths.last() {
        Some(uid) => format!(" (u{uid})"),
        None => String::new(),
    };
    let plain_note = match &room.note {
        Some(name) => format!(" ({name})"),
        None => String::new(),
    };
    let mut lines = vec![
        format!("#{} {}:", id, room.title),
        format!("{path_note} {plain_note}"),
    ];
    if let Some(area) = room.areas.last() {
        lines.push(area.clone());
    }
    if let Some(spec) = room.specs.last() {
        lines.push(spec.clone());
    }
    Some(lines)
}

/// The decimal reader the crate shares: an optional leading `u`/`U`, an optional
/// sign, then digits. Any other byte makes it `None`.
pub(crate) fn parse_number(text: &str) -> Option<i64> {
    let t = text.trim();
    let t = t.strip_prefix(['u', 'U']).unwrap_or(t);
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: i64 = digits.parse().ok()?;
    Some(if neg { -n } else { n })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cena::Room;

    fn map() -> Map {
        Map::from_rooms(vec![
            Room {
                id: 7116001,
                title: "The Tenebrous Cauldron".into(),
                paths: vec![7116000, 7116001],
                areas: vec!["[North Blue]".into(), "Inside".into()],
                specs: vec!["first".into(), "marble pillars".into()],
                note: Some("Sleeping Drake Harbor".into()),
                exits: Vec::new(),
            },
            Room {
                id: 7116002,
                title: "A Dusty Road".into(),
                paths: Vec::new(),
                areas: vec!["[North Blue]".into()],
                specs: Vec::new(),
                note: None,
                exits: Vec::new(),
            },
        ])
    }

    #[test]
    fn searching_by_number_and_by_word() {
        let m = map();
        let by_number = found(&m, "u7116002", true);
        assert_eq!(by_number.len(), 1);
        assert_eq!(by_number[0].id, 7116002);
        assert_eq!(by_number[0].title, "A Dusty Road");
        assert_eq!(by_number[0].path, "North Blue", "brackets are trimmed");

        let by_word = found(&m, "cauldron", false);
        assert_eq!(by_word.len(), 1);
        assert_eq!(by_word[0].id, 7116001);

        // Every word has to match.
        assert_eq!(found(&m, "cauldron dusty", false).len(), 0);
        assert_eq!(found(&m, "  ", false).len(), 0);
        // A number that is not in the map finds nothing by id …
        assert_eq!(found(&m, "5", true).len(), 0);
    }

    #[test]
    fn described_numbered_is_one_line() {
        let m = map();
        assert_eq!(
            described(&m, 7116001, true).unwrap(),
            vec!["#7116001 The Tenebrous Cauldron".to_string()]
        );
        assert!(described(&m, 1, true).is_none());
    }

    #[test]
    fn described_lists_the_place_the_notes_and_the_last_area_and_spec() {
        let m = map();
        assert_eq!(
            described(&m, 7116001, false).unwrap(),
            vec![
                "#7116001 The Tenebrous Cauldron:".to_string(),
                " (u7116001)  (Sleeping Drake Harbor)".to_string(),
                "Inside".to_string(),
                "marble pillars".to_string(),
            ]
        );
        // A room with neither paths nor a note still gets its two header lines.
        assert_eq!(
            described(&m, 7116002, false).unwrap(),
            vec![
                "#7116002 A Dusty Road:".to_string(),
                " ".to_string(),
                "[North Blue]".to_string(),
            ]
        );
    }

    #[test]
    fn the_number_reader_accepts_the_u_prefix_and_a_sign() {
        assert_eq!(parse_number("u7116001"), Some(7116001));
        assert_eq!(parse_number("U12"), Some(12));
        assert_eq!(parse_number(" -12 "), Some(-12));
        assert_eq!(parse_number("+7"), Some(7));
        assert_eq!(parse_number("-"), None);
        assert_eq!(parse_number("12a"), None);
        assert_eq!(parse_number(""), None);
    }
}
