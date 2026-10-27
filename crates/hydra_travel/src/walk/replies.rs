//! Reading the answer to a question the walk asked.
//!
//! Recovered from `hydra_travel::walk::replies::{word_after, numbered}`
//! (`0x101527e08`, `0x101527f10`).

/// The word that follows `needle` on a line (`word_after`, 264 bytes at
/// `0x101527e08`).
pub fn word_after<'a>(line: &'a str, needle: &str) -> Option<&'a str> {
    let at = line.find(needle)?;
    line[at + needle.len()..].split_whitespace().next()
}

/// The number on a numbered list line (`numbered`, 192 bytes at
/// `0x101527f10`).
///
/// The recovered list shape is the `Obvious exits` / `Obvious paths` pair plus
/// the numbered menu the game prints, e.g. `  1. north`.
pub fn numbered(line: &str) -> Option<(u32, &str)> {
    let trimmed = line.trim_start();
    let digits: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &trimmed[digits.len()..];
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    Some((digits.parse().ok()?, rest.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_after_needles() {
        assert_eq!(
            word_after("The gate is locked from the south.", "is "),
            Some("locked")
        );
        assert_eq!(word_after("nothing", "is "), None);
    }

    #[test]
    fn numbered_lines() {
        assert_eq!(numbered("  1. north"), Some((1, "north")));
        assert_eq!(numbered("12) go gate"), Some((12, "go gate")));
        assert_eq!(numbered("north"), None);
        assert_eq!(numbered("1 north"), None);
    }
}
