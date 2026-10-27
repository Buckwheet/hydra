//! What has to be true before a walk may start.

/// Why a walk did not start.
///
/// **Partly recovered** (`0x101557924`, 1364 bytes): the body formats the
/// literal at `0x10241f52d` — `it needs a name: .go2 save den` — when a saved
/// target has no name, and otherwise answers with the room's refusal.
pub fn halted(see: &[String]) -> Option<&'static str> {
    crate::walk::CANNOT_MOVE
        .into_iter()
        .find(|refusal| see.iter().any(|line| line.contains(refusal)))
}

/// The literal a nameless save prints (`0x10241f52d`).
pub const NEEDS_A_NAME: &str = "it needs a name: .go2 save den";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_message_is_exact() {
        assert_eq!(NEEDS_A_NAME, "it needs a name: .go2 save den");
    }

    #[test]
    fn halted_passes_the_refusals_through() {
        assert_eq!(halted(&[]), None);
        assert_eq!(
            halted(&["You cannot do that while mounted.".to_string()]),
            Some("You cannot do that while mounted.")
        );
    }
}
