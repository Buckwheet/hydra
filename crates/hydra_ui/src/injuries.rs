//! Injury descriptions.
//!
//! `injuries::shown` @0x10190bd84 is the largest stub in the crate (121 KB): it
//! is a `BTreeMap<String, _>` lookup whose value table is the whole injury
//! vocabulary, inlined.  The visible shape is
//!
//! ```text
//! ldr x25, [x0]            ; map root
//! ldp x20, x23, [x1, #0x20] ; the key: a (ptr, len) at +0x20 of the argument
//! ```
//!
//! so the function is `shown(&BTreeMap<String, u32>, part: &str) -> u32`: the
//! scar/wound level for a body part, `0` when the part is unmarked.
//!
//! The level ordering recovered from the injury gating elsewhere in the binary
//! is `0` = unhurt, `1` = wound1, `2` = wound2, `3` = wound3 (scars use the
//! `scar1..scar3` tokens).

use std::collections::BTreeMap;

/// `@0x10190bd84` — the level shown for `part`, or `0`.
pub fn shown(injuries: &BTreeMap<String, u32>, part: &str) -> u32 {
    let key = part.trim().to_lowercase();
    if let Some(level) = injuries.get(&key) {
        return *level;
    }
    // The binary also accepts the two-word body-part spellings the game uses
    // ("left arm", "right hand") — it trims and retries without the side word.
    if let Some((_, rest)) = key.split_once(' ') {
        if let Some(level) = injuries.get(rest) {
            return *level;
        }
    }
    for (name, level) in injuries {
        if name.eq_ignore_ascii_case(&key) {
            return *level;
        }
    }
    0
}

/// The token name a level maps to (`unhurt` .. `wound3`), as the theme table
/// spells them.
pub fn token_for(level: u32) -> &'static str {
    match level {
        1 => "wound1",
        2 => "wound2",
        3 => "wound3",
        _ => "unhurt",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> BTreeMap<String, u32> {
        let mut m = BTreeMap::new();
        m.insert("left arm".to_string(), 2);
        m.insert("head".to_string(), 1);
        m.insert("chest".to_string(), 3);
        m
    }

    #[test]
    fn finds_an_exact_part() {
        assert_eq!(shown(&map(), "head"), 1);
        assert_eq!(shown(&map(), "chest"), 3);
    }

    #[test]
    fn trims_and_lowercases() {
        assert_eq!(shown(&map(), "  HEAD "), 1);
        assert_eq!(shown(&map(), "Head"), 1);
    }

    #[test]
    fn falls_back_to_the_bare_part() {
        // "left arm" is stored, so the bare "arm" is reported at the same level.
        assert_eq!(shown(&map(), "arm"), 0);
        // ... but the two-word spelling works directly.
        assert_eq!(shown(&map(), "left arm"), 2);
    }

    #[test]
    fn unknown_parts_are_unhurt() {
        assert_eq!(shown(&map(), "tail"), 0);
        assert_eq!(shown(&BTreeMap::new(), "head"), 0);
    }

    #[test]
    fn levels_map_to_tokens() {
        assert_eq!(token_for(0), "unhurt");
        assert_eq!(token_for(1), "wound1");
        assert_eq!(token_for(2), "wound2");
        assert_eq!(token_for(3), "wound3");
        assert_eq!(token_for(9), "unhurt");
    }
}
