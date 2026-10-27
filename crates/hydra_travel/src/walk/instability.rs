//! Elemental instabilities: the "oddly warped air" rooms a walk has to feel
//! its way through.
//!
//! Recovered from `hydra_travel::walk::instability::*`
//! (`0x101555f40`..`0x101557924`) and the persisted `Seen` row.

use crate::json;

/// The element an instability is made of.
///
/// **INFERRED from the literal pool at `0x10241f460`**, which holds five
/// phrases and four names in one merged run:
/// `ATTUNE SET | gust of wind | air | burst of sparks | lightning | waft of
/// heat | earth | puff of rock dust | water | puff of mist | The air is oddly
/// warped and distorted here | Seen | room | element | seen`.
/// Four names against five phrases means one name covers two phrases;
/// `waft of heat` is paired with `earth` here because that is the order the
/// pool stores them in. See `NOTES.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Element {
    Air,
    Lightning,
    Earth,
    Water,
}

impl Element {
    pub const ALL: [Element; 4] = [
        Element::Air,
        Element::Lightning,
        Element::Earth,
        Element::Water,
    ];

    /// The name the game uses (`air`, `lightning`, `earth`, `water`).
    pub fn name(self) -> &'static str {
        match self {
            Element::Air => "air",
            Element::Lightning => "lightning",
            Element::Earth => "earth",
            Element::Water => "water",
        }
    }

    /// Every phrase that announces this element, from the same pool.
    pub fn phrases(self) -> &'static [&'static str] {
        match self {
            Element::Air => &["gust of wind"],
            Element::Lightning => &["burst of sparks"],
            Element::Earth => &["waft of heat", "puff of rock dust"],
            Element::Water => &["puff of mist"],
        }
    }

    pub fn parse(name: &str) -> Option<Element> {
        Element::ALL
            .into_iter()
            .find(|e| e.name().eq_ignore_ascii_case(name.trim()))
    }
}

impl std::fmt::Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The room message that announces an unstable room (`0x10241f4c4`).
pub const WARPED: &str = "The air is oddly warped and distorted here";

/// The game's attunement header (`0x10241f460`, length 10).
pub const ATTUNE: &str = "ATTUNE SET";

/// One remembered instability.
///
/// **ESTABLISHED**: the serde `Deserialize` visitor reports
/// `struct Seen with 3 elements` (`0x10241dfd6`) and the field names are
/// `room`, `element`, `seen` (`0x10241f4ee`); `Seen::serialize`
/// (`0x10155634c`) writes them in that order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// The room the instability was met in.
    pub room: u32,
    /// The element's name.
    pub element: String,
    /// How many times it has been seen / how it was answered.
    pub seen: u32,
}

impl Seen {
    pub fn new(room: u32, element: Element, seen: u32) -> Seen {
        Seen {
            room,
            element: element.name().to_string(),
            seen,
        }
    }

    pub fn element(&self) -> Option<Element> {
        Element::parse(&self.element)
    }

    pub fn to_json(&self) -> json::Value {
        let mut obj = std::collections::BTreeMap::new();
        obj.insert("room".to_string(), json::Value::Number(self.room as f64));
        obj.insert(
            "element".to_string(),
            json::Value::string(self.element.clone()),
        );
        obj.insert("seen".to_string(), json::Value::Number(self.seen as f64));
        json::Value::Object(obj)
    }

    pub fn from_json(value: &json::Value) -> Option<Seen> {
        Some(Seen {
            room: value.get("room")?.as_u32()?,
            element: value.get("element")?.as_str()?.to_string(),
            seen: value.get("seen").and_then(|v| v.as_u32()).unwrap_or_default(),
        })
    }
}

/// Does this line name an element?
///
/// **ESTABLISHED (shape)** (`0x101555f40`): the body walks a slice of
/// 24-byte (`String`-sized) entries, trims each, and calls
/// `str::contains` (`is_contained_in` at `0x1015539c4`) with a 10-byte
/// needle — `"element of"` (`0x656c/0x656d/0x746e/0x6f20` + `0x2066` at
/// `0x101555f78`) — after requiring the trimmed length to reach `0x22` (34).
/// The element itself is then chosen by the phrase.
pub fn element_in(lines: &[String], from: usize) -> Option<Element> {
    for line in lines.iter().skip(from) {
        let line = line.trim();
        if line.len() < 34 {
            continue;
        }
        if !line.contains("element of") {
            continue;
        }
        if let Some(element) = senses(line) {
            return Some(element);
        }
    }
    None
}

/// Which element a phrase names.
pub fn senses(line: &str) -> Option<Element> {
    let lower = line.to_ascii_lowercase();
    Element::ALL
        .into_iter()
        .find(|element| element.phrases().iter().any(|phrase| lower.contains(phrase)))
}

/// The element given off by a room, from its prose.
///
/// **Partly recovered** (`0x101556134`): the body looks for the [`WARPED`]
/// message and the phrase table and reports the element the room gives off (as
/// opposed to the one it wants). The exact predicate that picks the reported
/// element is not recoverable from the instruction stream; this build reports
/// the phrase the room's prose carries.
pub fn element_given_off(line: &str) -> Option<Element> {
    senses(line)
}

/// Answer the instability question.
///
/// **Partly recovered** (`0x101556534`): the argument is trimmed and
/// ASCII-lowercased, then dispatched on length. One 11-byte needle spells
/// `"instability"` (`0x6e69/0x7473/0x6261/0x6c69` + `0x6174/0x6962/0x696c/
/// 0x7974`, returning tag `3`), and a 15-byte needle begins `"confu…"`. The
/// remaining needles and their tags are not recoverable, so only the recovered
/// ones are answered.
pub fn asked(word: &str) -> Option<Element> {
    let word = word.trim().to_ascii_lowercase();
    match word.as_str() {
        // 11 bytes, tag 3 in the original.
        "instability" => Some(Element::Earth),
        _ => Element::parse(&word),
    }
}

/// Narrow a full instability line down to its element and room.
///
/// **Partly recovered** (`0x10155680c`): the body trims, requires a length
/// floor, splits on the phrase table and keeps a trailing room number.
pub fn narrow(line: &str) -> Option<(Element, Option<u32>)> {
    let element = senses(line)?;
    let room = line
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .next_back()
        .and_then(|digits| digits.parse::<u32>().ok());
    Some((element, room))
}

/// Build the half of an element phrase that follows the adjective.
///
/// **Partly recovered** (`0x101557810`): the body selects one of two static
/// 64-byte tables (four `&str` entries each, at `0x10443b8a8` and
/// `0x10443b8e8`) on its first argument, then formats through the piece list at
/// `0x103e6c7d2`, which is `"huge "` (5) followed by a piece beginning
/// `" element…"`. The two tables' word lists are `&str` pairs in
/// `__DATA_CONST` and were not recovered, so this build keeps the two table
/// shapes and the recovered format.
pub fn half_tag(large: bool, element: Element) -> String {
    const SMALL: [&str; 4] = ["", "", "", ""];
    const LARGE: [&str; 4] = ["huge ", "", "", ""];
    let table = if large { LARGE } else { SMALL };
    let prefix = table[match element {
        Element::Air => 0,
        Element::Lightning => 1,
        Element::Earth => 2,
        Element::Water => 3,
    }];
    format!("{prefix} element of {element}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_map_to_the_recovered_elements() {
        assert_eq!(senses("A gust of wind rushes past."), Some(Element::Air));
        assert_eq!(
            senses("There is a burst of sparks."),
            Some(Element::Lightning)
        );
        assert_eq!(senses("A waft of heat rises."), Some(Element::Earth));
        assert_eq!(senses("A puff of rock dust settles."), Some(Element::Earth));
        assert_eq!(senses("A puff of mist drifts."), Some(Element::Water));
        assert_eq!(senses("nothing here"), None);
    }

    #[test]
    fn seen_row_matches_the_serde_field_names() {
        let seen = Seen::new(1234, Element::Water, 2);
        assert_eq!(seen.element, "water");
        let json = seen.to_json();
        assert_eq!(json.get("room").unwrap().as_u32(), Some(1234));
        assert_eq!(json.get("element").unwrap().as_str(), Some("water"));
        assert_eq!(json.get("seen").unwrap().as_u32(), Some(2));
        assert_eq!(Seen::from_json(&json), Some(seen));
    }

    #[test]
    fn element_in_needs_the_ten_byte_needle_and_thirty_four_chars() {
        let lines = vec![
            "You sense an element of fire here.".to_string(),
            "You sense a gust of wind, an element of air about the shrine.".to_string(),
        ];
        assert_eq!(element_in(&lines, 0), Some(Element::Air));
        assert_eq!(element_in(&lines, 2), None);
    }

    #[test]
    fn asked_answers_the_recovered_needle() {
        assert_eq!(asked("  INSTABILITY "), Some(Element::Earth));
        assert_eq!(asked("water"), Some(Element::Water));
        assert_eq!(asked("banana"), None);
    }

    #[test]
    fn narrow_keeps_the_trailing_room_number() {
        assert_eq!(
            narrow("A puff of mist drifts through room 4412"),
            Some((Element::Water, Some(4412)))
        );
    }
}
