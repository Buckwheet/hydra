//! Tag-pair and markup classification tables.
//!
//! Two of the original's small lookup functions live here; both are plain
//! byte comparisons after `text::tag_name`, with no table and no hash:
//!
//!   * `parser::is_paired` (572 B, `0x101c1f1f8`) — [`is_paired`]
//!   * `parser::markup::is_markup` (264 B, `0x101c3948c`) — [`is_markup`]
//!
//! `is_paired` is called twice from `parse_line` (`0x101c1ab9c`,
//! `0x101c1b774`); `is_markup` is called from `parse_runs_reporting`
//! (`0x101c19a24`), `parse_line` (`0x101c1ae0c`) and `dispatch`
//! (`0x101c1d840`).

use crate::text::tag_name;

/// Tags that come with a matching `</tag>` and therefore swallow the rest of
/// the line as their body.
///
/// Established from the length-dispatched compare chain in `is_paired`
/// (`0x101c1f1f8`), which first calls `text::tag_name` and then switches on
/// the byte length of the name:
///
/// | len | names |
/// |---|---|
/// | 3 | `inv` |
/// | 4 | `left`, `menu` |
/// | 5 | `spell`, `right` |
/// | 6 | `prompt` |
/// | 7 | `compass`, `pDefault`, `cmdlist` |
/// | 9 | `component` |
/// | 10 | `worldEvent`, `egMerchant`, `objectives` |
/// | 16 | `inventoryManager` |
///
/// Every other length and every other name answers `false` — the chain ends
/// in `cset w8, eq` on the last comparison, so e.g. `"invx"`, `"Inv"` and
/// `"inventory"` are all unpaired.
pub const PAIRED_TAGS: &[&str] = &[
    "inv",
    "left",
    "menu",
    "spell",
    "right",
    "prompt",
    "compass",
    "pDefault",
    "cmdlist",
    "component",
    "worldEvent",
    "egMerchant",
    "objectives",
    "inventoryManager",
];

/// Tags whose body is inline markup rather than prose.
///
/// Established from `parser::markup::is_markup` (`0x101c3948c`):
///
/// * length 1 — a single bit test over `b - b'a'` against the mask `0b1011`,
///   accepted only while `b - b'a' < 4`: `a`, `b`, `d` (`c` is excluded);
/// * length 5 — `style`;
/// * length 6 — `preset`, `output`;
/// * length 7 — `popBold`;
/// * length 8 — `pushBold`.
pub const MARKUP_TAGS: &[&str] = &["a", "b", "d", "style", "preset", "output", "popBold", "pushBold"];

/// True when `tag` is one of the tags that always has a closing partner.
///
/// The argument is a whole tag *or* a bare name: [`tag_name`] is applied
/// first, exactly as the binary does.
pub fn is_paired(tag: &str) -> bool {
    PAIRED_TAGS.contains(&tag_name(tag))
}

/// True when `name` is an inline-markup tag rather than a prose tag.
///
/// Unlike [`is_paired`] this does **not** call `text::tag_name` — the binary
/// starts straight from `cmp x1, #0x5` on the raw byte length, so callers in
/// `parse_runs_reporting`/`parse_line`/`dispatch` pass an already-stripped
/// name.  A whole tag such as `"<b>"` therefore answers `false`, which is
/// what the original does.
pub fn is_markup(name: &str) -> bool {
    MARKUP_TAGS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_paired_tag_is_paired_and_nothing_else_is() {
        // The exact table, recovered name-for-name from the compare chain.
        for name in [
            "inv",
            "left",
            "menu",
            "spell",
            "right",
            "prompt",
            "compass",
            "pDefault",
            "cmdlist",
            "component",
            "worldEvent",
            "egMerchant",
            "objectives",
            "inventoryManager",
        ] {
            assert!(is_paired(name), "{name} should be paired");
        }
        assert_eq!(PAIRED_TAGS.len(), 14);
        for name in [
            "", "in", "invo", "menuItem", "menuItems", "Prompts", "PROMPT", "spellx",
            "inventory", "dialogData", "roomDesc", "streamWindow", "output", "component2",
            "worldEvents", "eGmerchant", "objective", "settingsInfo",
        ] {
            assert!(!is_paired(name), "{name} should not be paired");
        }
    }

    #[test]
    fn paired_tags_accept_a_whole_tag() {
        assert!(is_paired("<compass>"));
        assert!(is_paired("</compass>"));
        assert!(is_paired("<prompt time=\"1\">"));
        assert!(!is_paired("<roomDesc>"));
    }

    #[test]
    fn the_single_letter_markup_mask_is_a_b_and_d() {
        assert!(is_markup("a"));
        assert!(is_markup("b"));
        assert!(is_markup("d"));
        // `0b1011` has bit 2 clear, so `c` is *not* markup.
        assert!(!is_markup("c"));
        // Everything outside `a..=d` is excluded by the `cmp #0x4` guard.
        for name in ["e", "A", "z", "0", "", "aa"] {
            assert!(!is_markup(name), "{name:?} should not be markup");
        }
    }

    #[test]
    fn the_named_markup_tags_are_exactly_the_recovered_set() {
        for name in ["style", "preset", "output", "popBold", "pushBold"] {
            assert!(is_markup(name), "{name} should be markup");
        }
        for name in [
            "pushBold2",
            "presets",
            "Style",
            "OUTPUT",
            "pushbold",
            "popbold",
            "bold",
            "i",
            "u",
        ] {
            assert!(!is_markup(name), "{name} should not be markup");
        }
    }

    #[test]
    fn markup_does_not_normalise_a_whole_tag() {
        // The binary compares the raw bytes, so a whole tag never matches.
        assert!(!is_markup("<pushBold/>"));
        assert!(!is_markup("</b>"));
        assert!(!is_markup("<a>"));
        // …but the tag name does.
        assert!(is_markup(tag_name("<pushBold/>")));
        assert!(is_markup(tag_name("</b>")));
    }
}
