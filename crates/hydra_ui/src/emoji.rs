//! `:shortcode:` emoji expansion for the story window.
//!
//! `@0x101939150` scans for `:name:`, where `name` is one or more of
//! `[A-Za-z0-9_+-]`, lowercases it, looks it up in a `phf::Map` and inserts the
//! emoji text in place of the whole `:name:` run.
//!
//! The map is `phf::map::Map<&str, u32>` @0x10449bdc8 with **1913** entries and
//! 383 displacements; the value indexes a 3944-slot table of `&str` cells at
//! 0x104452338 (stride 64: an emoji plus its skin-tone variants).  Both tables
//! were read out of the binary — see [`EMOJI`].
//!
//! The return type is `Option<String>`: the binary keeps the sentinel
//! `isize::MIN` in the result's capacity word when nothing was replaced, so
//! `None` means "unchanged".

use crate::emoji_table::EMOJI;

/// `true` when `c` may appear inside a `:shortcode:`.
///
/// Recovered from the byte tests: `-`, `+`, `0`..`9`, `_`, `A`..`Z`, `a`..`z`.
pub fn is_shortcode_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'+'
}

/// `@0x101939150` — expand every `:shortcode:` in `line`.
///
/// Returns `None` when nothing changed.
pub fn expanded(line: &str) -> Option<String> {
    if !line.contains(':') {
        return None;
    }
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut last = 0usize; // where the copy cursor is
    let mut i = 0usize;
    let mut changed = false;
    while i < bytes.len() {
        if bytes[i] != b':' {
            i += 1;
            continue;
        }
        // Collect the shortcode body.
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && is_shortcode_byte(bytes[end]) {
            end += 1;
        }
        // Needs at least one body byte and a closing colon.
        if end == start || end >= bytes.len() || bytes[end] != b':' {
            i = start.max(i + 1);
            continue;
        }
        let name = &line[start..end];
        let lower = name.to_ascii_lowercase();
        if let Some(glyph) = lookup(&lower) {
            out.push_str(&line[last..i]);
            out.push_str(glyph);
            changed = true;
            i = end + 1;
            last = i;
        } else {
            i = end + 1;
        }
    }
    if !changed {
        return None;
    }
    out.push_str(&line[last..]);
    Some(out)
}

/// Look a shortcode up without expanding anything.
pub fn lookup(shortcode: &str) -> Option<&'static str> {
    let lower = shortcode.to_ascii_lowercase();
    EMOJI
        .binary_search_by(|(k, _)| (*k).cmp(lower.as_str()))
        .ok()
        .map(|i| EMOJI[i].1)
}

/// How many shortcodes the binary knows.
pub const COUNT: usize = 1913;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_complete() {
        assert_eq!(EMOJI.len(), COUNT);
        for w in EMOJI.windows(2) {
            assert!(w[0].0 < w[1].0, "{} >= {}", w[0].0, w[1].0);
        }
        // Spot checks against the values read out of the binary.
        assert_eq!(lookup("eggplant"), Some("\u{1f346}"));
        assert_eq!(lookup("hamburger"), Some("\u{1f354}"));
        assert_eq!(lookup("bangbang"), Some("\u{203c}\u{fe0f}"));
    }

    #[test]
    fn shortcode_bytes() {
        for c in b"azAZ09_+-".iter() {
            assert!(is_shortcode_byte(*c), "{}", *c as char);
        }
        for c in b" :./".iter() {
            assert!(!is_shortcode_byte(*c), "{}", *c as char);
        }
    }

    #[test]
    fn expands_a_shortcode() {
        let got = expanded(":eggplant:").unwrap();
        assert_eq!(got, "\u{1f346}");
        let got = expanded("a :hamburger: b").unwrap();
        assert_eq!(got, "a \u{1f354} b");
        let got = expanded(":eggplant::hamburger:").unwrap();
        assert_eq!(got, "\u{1f346}\u{1f354}");
    }

    #[test]
    fn leaves_text_alone_when_nothing_matches() {
        assert_eq!(expanded("plain text"), None);
        assert_eq!(expanded("time 12:30"), None);
        assert_eq!(expanded(":not_a_shortcode:"), None);
        // A colon with nothing after it.
        assert_eq!(expanded(":"), None);
        assert_eq!(expanded("::"), None);
        // Unbalanced.
        assert_eq!(expanded(":eggplant"), None);
    }

    #[test]
    fn shortcodes_are_case_insensitive() {
        assert_eq!(expanded(":EGGPLANT:"), expanded(":eggplant:"));
        assert_eq!(expanded(":EggPlant:"), expanded(":eggplant:"));
    }
}
