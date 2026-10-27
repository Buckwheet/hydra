//! Elemental instabilities: the "oddly warped air" rooms a walk has to feel
//! its way through.
//!
//! Recovered from `hydra_travel::walk::instability::*`
//! (`0x101555f40`..`0x101557924`) and the persisted `Seen` row.

use serde::{Deserialize, Serialize};

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
///
/// **Phase 1 reconnection:** the derives below are the original's. `seen` is
/// `#[serde(default)]` because the previous reader let the row omit it; `room`
/// and `element` stay required, so a row with neither is not a `Seen`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    /// The room the instability was met in.
    pub room: u32,
    /// The element's name.
    pub element: String,
    /// How many times it has been seen / how it was answered.
    #[serde(default)]
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

    /// The row as the serializer writes it (`room`, `element`, `seen`).
    pub fn to_json(&self) -> json::Value {
        serde_json::to_value(self).unwrap_or(json::Value::Null)
    }

    pub fn from_json(value: &json::Value) -> Option<Seen> {
        serde_json::from_value(value.clone()).ok()
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

/// The question the game asked.
///
/// The four words and their tag numerals come straight out of [`asked`]'s
/// compare chain (`0x101556574`..`0x101556674`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Asked {
    /// `confluence` (10 bytes) — tag `0` (`mov w21, #0x0` at `0x10155663c`).
    Confluence,
    /// `confluence-hot` (14 bytes) — tag `1` (`mov w21, #0x1` at `0x10155672c`).
    ConfluenceHot,
    /// `confluence-cold` (15 bytes) — tag `2` (`mov w21, #0x2` at `0x10155660c`).
    ConfluenceCold,
    /// `instability` (11 bytes) — tag `3` (`mov w21, #0x3` at `0x1015565c0`).
    Instability,
}

impl Asked {
    /// Every word [`asked`] answers, in tag order.
    pub const ALL: [Asked; 4] = [
        Asked::Confluence,
        Asked::ConfluenceHot,
        Asked::ConfluenceCold,
        Asked::Instability,
    ];

    /// The tag numeral the original returned for this word.
    pub fn tag(self) -> u8 {
        match self {
            Asked::Confluence => 0,
            Asked::ConfluenceHot => 1,
            Asked::ConfluenceCold => 2,
            Asked::Instability => 3,
        }
    }

    /// The word itself.
    pub fn word(self) -> &'static str {
        match self {
            Asked::Confluence => "confluence",
            Asked::ConfluenceHot => "confluence-hot",
            Asked::ConfluenceCold => "confluence-cold",
            Asked::Instability => "instability",
        }
    }
}

/// The area a Confluence room sits in — the 22-byte string `asked` compares the
/// room's first path string against (`0x1015566b4`..`0x1015566e4`).
pub const CONFLUENCE_AREA: &str = "[Elemental Confluence]";

/// Which instability question a word asks.
///
/// **ESTABLISHED**: the body trims and lower-cases its argument
/// (`0x101556564`, `0x10155656c`) and then switches on its **byte length**
/// (`cmp x8, #0xd` / `#0xa` / `#0xb` / `#0xe` / `#0xf` at `0x101556574`):
///
/// | length | needle | tag |
/// |---|---|---|
/// | 10 | `confluence` (`0x6f63/0x666e/0x756c/0x6e65` then `ldrh` `"ce"`) | `0` |
/// | 11 | `instability` (`0x6e69/0x7473/0x6261/0x6c69` + `0x6174/0x6962/0x696c/0x7974`) | `3` |
/// | 14 | `confluence-hot` (`ldur` at `+6` = `"ence-hot"`) | `1` |
/// | 15 | `confluence-cold` (`ldur` at `+7` = `"nce-cold"`) | `2` |
///
/// Anything else falls through to a room test: when the caller's tag argument is
/// `1` the body asks `cena_map::map::Map::room` (`0x101556688`) and compares the
/// first of the room's path strings against [`CONFLUENCE_AREA`].
///
/// **Not reproduced:** that fall-through, and the room the original returned
/// beside the tag (`mov x0, x21; mov x1, x20` at `0x10155670c`) — both need the
/// caller's tag argument, which the reconstructed signature does not carry.
pub fn asked(word: &str) -> Option<Asked> {
    let word = word.trim().to_lowercase();
    Asked::ALL.into_iter().find(|asked| asked.word() == word)
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

/// Is this phrase one of the "huge … elemental" half tags? (`0x101557810`)
///
/// **ESTABLISHED:** the two tables were read out of
/// `__DATA_CONST` and the format string decoded — this is a **predicate**, not a
/// builder:
///
/// * `0x10443b8a8`, four `&str` pairs — `air` (3), `earth` (5), `ice` (3),
///   `water` (5); i.e. the *whole* forms;
/// * `0x10443b8e8`, four `&str` pairs — `fire` (4), `lava` (4), `steam` (5),
///   `lightning` (9); i.e. their *half* forms, pair for pair;
/// * `cmp w0, #0x0; csel x24, x9, x8, ne` (`0x101557840`) picks the second
///   table when the first argument is non-zero;
/// * the piece list at `0x103e6c7d2` is `"huge "` (5) + `" elemental"` (10);
/// * the loop formats `huge {word} elemental` and compares it with the second
///   argument by length (`cmp x19, x8`, `0x1015578a8`) and bytes
///   (`memcmp`, `0x1015578c0`), returning `1` on a match and `0` otherwise.
pub fn half_tag(half: bool, phrase: &str) -> bool {
    let words = if half { HALF } else { WHOLE };
    words
        .iter()
        .any(|word| phrase == format!("huge {word} elemental"))
}

/// The whole-element words, in the order `0x10443b8a8` holds them.
pub const WHOLE: [&str; 4] = ["air", "earth", "ice", "water"];

/// The half-element words, in the order `0x10443b8e8` holds them.
pub const HALF: [&str; 4] = ["fire", "lava", "steam", "lightning"];

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
        assert_eq!(json.get("room").unwrap().as_u64(), Some(1234));
        assert_eq!(json.get("element").unwrap().as_str(), Some("water"));
        assert_eq!(json.get("seen").unwrap().as_u64(), Some(2));
        assert_eq!(Seen::from_json(&json), Some(seen));
    }

    /// The recovered serializer (`0x10155634c`) writes `room`, `element`, `seen`
    /// in that order; `serde` now produces exactly that row.
    #[test]
    fn seen_serialises_in_the_recovered_order() {
        let seen = Seen::new(4412, Element::Water, 2);
        assert_eq!(
            serde_json::to_string(&seen).unwrap(),
            r#"{"room":4412,"element":"water","seen":2}"#
        );
        assert_eq!(
            serde_json::from_str::<Seen>(r#"{"room":4412,"element":"water","seen":2}"#).unwrap(),
            seen
        );
    }

    /// `seen` was the one field the previous reader defaulted; `room` and
    /// `element` stayed required, and an unknown field is ignored.
    #[test]
    fn seen_defaults_and_ignores_unknown_fields() {
        let seen: Seen =
            serde_json::from_str(r#"{"room":4412,"element":"air","when":"today"}"#).unwrap();
        assert_eq!(seen, Seen::new(4412, Element::Air, 0));
        // Missing `element` is not a `Seen` — `from_json` says so, it does not
        // invent one.
        assert_eq!(Seen::from_json(&serde_json::json!({"room": 4412})), None);
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
    fn asked_answers_the_recovered_needles() {
        assert_eq!(asked("  INSTABILITY "), Some(Asked::Instability));
        assert_eq!(asked("Confluence"), Some(Asked::Confluence));
        assert_eq!(asked("confluence-hot"), Some(Asked::ConfluenceHot));
        assert_eq!(asked(" CONFLUENCE-COLD\n"), Some(Asked::ConfluenceCold));
        // The four words are the only ones the compare chain carries; a bare
        // element name is not a question.
        assert_eq!(asked("water"), None);
        assert_eq!(asked("confluenc"), None);
        assert_eq!(asked("banana"), None);
    }

    /// The tag numerals the original returned, and the order they are numbered
    /// in. The words are dispatched on their byte length in the binary, so the
    /// lengths are part of the evidence.
    #[test]
    fn asked_tags_and_word_lengths_are_the_recovered_ones() {
        assert_eq!(Asked::Confluence.tag(), 0);
        assert_eq!(Asked::ConfluenceHot.tag(), 1);
        assert_eq!(Asked::ConfluenceCold.tag(), 2);
        assert_eq!(Asked::Instability.tag(), 3);
        assert_eq!(Asked::ALL.map(|a| a.word().len()), [10, 14, 15, 11]);
        assert_eq!(CONFLUENCE_AREA.len(), 22);
    }

    /// The two halves of the element words, read out of the two `&str` tables at
    /// `0x10443b8a8` and `0x10443b8e8`.
    #[test]
    fn half_tag_tables_are_the_recovered_words() {
        assert_eq!(WHOLE, ["air", "earth", "ice", "water"]);
        assert_eq!(HALF, ["fire", "lava", "steam", "lightning"]);
        assert_eq!(WHOLE.map(|w| w.len()), [3, 5, 3, 5]);
        assert_eq!(HALF.map(|w| w.len()), [4, 4, 5, 9]);
    }

    /// `half_tag` formats `huge {word} elemental` and compares for equality — the
    /// table is chosen by the first argument (`csel …, ne` at `0x101557840`).
    #[test]
    fn half_tag_matches_the_formatted_phrase_exactly() {
        assert!(half_tag(false, "huge air elemental"));
        assert!(half_tag(false, "huge earth elemental"));
        assert!(half_tag(false, "huge ice elemental"));
        assert!(half_tag(false, "huge water elemental"));
        assert!(half_tag(true, "huge fire elemental"));
        assert!(half_tag(true, "huge lava elemental"));
        assert!(half_tag(true, "huge steam elemental"));
        assert!(half_tag(true, "huge lightning elemental"));
        // Each table only knows its own half.
        assert!(!half_tag(false, "huge fire elemental"));
        assert!(!half_tag(true, "huge air elemental"));
        // It is an equality test, not a substring test.
        assert!(!half_tag(false, "a huge air elemental"));
        assert!(!half_tag(false, "huge air elemental!"));
        assert!(!half_tag(false, "huge  air elemental"));
        assert!(!half_tag(false, "Huge Air Elemental"));
        assert!(!half_tag(false, ""));
    }


    #[test]
    fn narrow_keeps_the_trailing_room_number() {
        assert_eq!(
            narrow("A puff of mist drifts through room 4412"),
            Some((Element::Water, Some(4412)))
        );
    }
}
