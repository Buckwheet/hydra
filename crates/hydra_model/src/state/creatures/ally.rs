//! Friendly/ally detection.
//!
//! `ally::classify` (940 B) uses `creature::by_name` and `gameobj::classify`;
//! the literal `"passive npc"` and the `"\t"` separator come from the bestiary
//! row it consults.
//!
//! `// UNKNOWN:` the full noun list; only the recovered markers are used.

/// Is `text` one of the player's allies?
pub fn classify(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    if let Some(template) = crate::state::creature::by_name(text) {
        return template.ally;
    }
    // The recovered `gameobj::classify` fallback: passive NPCs are allies.
    matches!(
        crate::state::gameobj::classify(text),
        Some(crate::state::gameobj::ObjectKind::PassiveNpc)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_not_an_ally() {
        assert!(!classify(""));
    }

    #[test]
    fn passive_npcs_are_allies() {
        assert!(classify("the town guard is a passive npc"));
    }

    #[test]
    fn monsters_are_not_allies() {
        assert!(!classify("a scruffy kobold"));
    }
}
