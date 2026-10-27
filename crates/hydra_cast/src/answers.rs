//! What the game says back while a spell is being cast, and what the caster
//! makes of it.
//!
//! The recovered `hydra_cast::answers::answer` (0x1019a0564, 6540 bytes) is a
//! matcher over one line of game text.  It trims the line and then runs a long
//! chain of tests; the six whole-sentence tests below are `str::contains` calls
//! with the literal and its length in the instruction stream, so both the text
//! and its length are established:
//!
//! ```text
//! 0x1019a0d38  34 bytes  "You can only evoke certain spells."
//! 0x1019a0d54  52 bytes  "You can only channel certain spells for extra power."
//! 0x1019a0ef8  28 bytes  "Spells of War cannot be cast "
//! 0x1019a15ec  69 bytes  "As you begin to prepare the spell the wind blows small objects at you"
//! 0x1019a1a48  78 bytes  "As you focus on your magic, your vision swims with a swirling haze of crimson."
//! 0x1019a1ca8  38 bytes  "That is not something you can prepare."
//! ```
//!
//! The function returns a niche-encoded enum by value (its tags are
//! `0x8000_0000_0000_0000 | index`, so eight or more arms).  The remaining arms
//! compare short prefixes with immediates in the instruction stream and were
//! not enumerated; see `NOTES.md`.
//!
//! The other half of the pair, `answers::stance` (0x1019a1de8), is short enough
//! to be recovered whole and is implemented exactly.

/// One thing the game said about a cast.
///
/// UNKNOWN: the original enum's variant list and indices.  Each variant here is
/// named after the game line it recognises, so the mapping is
/// line -> variant rather than index -> variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// "You can only evoke certain spells."
    OnlyEvoke,
    /// "You can only channel certain spells for extra power."
    OnlyChannel,
    /// "Spells of War cannot be cast " — the sentence continues with the
    /// reason, so only the prefix is fixed.
    War,
    /// "As you begin to prepare the spell the wind blows small objects at you"
    Wind,
    /// "As you focus on your magic, your vision swims with a swirling haze of
    /// crimson."
    Haze,
    /// "That is not something you can prepare."
    NotPreparable,
}

/// The six whole-sentence tests, in the order the binary runs them.
const SAID: [(Answer, &str); 6] = [
    (Answer::OnlyEvoke, "You can only evoke certain spells."),
    (
        Answer::OnlyChannel,
        "You can only channel certain spells for extra power.",
    ),
    (Answer::War, "Spells of War cannot be cast "),
    (
        Answer::Wind,
        "As you begin to prepare the spell the wind blows small objects at you",
    ),
    (
        Answer::Haze,
        "As you focus on your magic, your vision swims with a swirling haze of crimson.",
    ),
    (
        Answer::NotPreparable,
        "That is not something you can prepare.",
    ),
];

/// Read one line of game text.
///
/// ESTABLISHED: the line is trimmed before it is matched (0x1019a0588
/// `str::trim_matches`) and every test is a `contains`, not a prefix.
///
/// UNKNOWN: the arms of the original that do not carry a whole-sentence
/// literal; those lines come back as `None` here.
pub fn answer(line: &str) -> Option<Answer> {
    let line = line.trim();
    for (answer, said) in SAID {
        if line.contains(said) {
            return Some(answer);
        }
    }
    None
}

/// Whether this line is the game confirming a stance change.
///
/// Recovered whole from 0x1019a1de8: a line of at least 16 bytes that starts
/// with `You are now in a` and ends with ` stance.`, or a line of at least 45
/// bytes that starts with `Cast Roundtime in effect: Setting stance to `.
pub fn stance(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() >= 16
        && line.starts_with("You are now in a")
        && line.ends_with(" stance.")
    {
        return true;
    }
    bytes.len() >= 45 && line.starts_with("Cast Roundtime in effect: Setting stance to ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_recovered_sentences() {
        assert_eq!(
            answer("You can only evoke certain spells."),
            Some(Answer::OnlyEvoke)
        );
        assert_eq!(
            answer("  You can only channel certain spells for extra power.  "),
            Some(Answer::OnlyChannel)
        );
        assert_eq!(
            answer("Spells of War cannot be cast from this stance."),
            Some(Answer::War)
        );
        assert_eq!(
            answer("As you begin to prepare the spell the wind blows small objects at you."),
            Some(Answer::Wind)
        );
        assert_eq!(
            answer("As you focus on your magic, your vision swims with a swirling haze of crimson."),
            Some(Answer::Haze)
        );
        assert_eq!(
            answer("That is not something you can prepare."),
            Some(Answer::NotPreparable)
        );
    }

    #[test]
    fn ignores_other_lines() {
        assert_eq!(answer("You prepare 901."), None);
        assert_eq!(answer(""), None);
    }

    #[test]
    fn recognises_the_stance_lines() {
        assert!(stance("You are now in a guarded stance."));
        assert!(stance(
            "Cast Roundtime in effect: Setting stance to guarded."
        ));
        assert!(!stance("You are now in a guarded stance"));
        assert!(!stance("You are now in a"));
        assert!(!stance("Cast Roundtime in effect: Setting stance"));
    }
}
