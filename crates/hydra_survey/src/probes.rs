//! The two survey probes: whether magic works in a room, and whether the room
//! can be splashed.
//!
//! Both take the lines a command answered with (`&[String]`: 24-byte `String`
//! elements, the pointer read at `+0x8`) and answer with a three-way value. The
//! literal comparisons below are read straight out of the instruction stream;
//! every marker sits in `__const` at the address quoted.
//!
//! `hydra_gui::surveyor` shows the answer as `magic: works` / `magic: blocked`
//! and `water: yes` / `water: no`, skipping the third value, which pins the
//! meaning of `0`, `1` and `2` (`Picture::from_records` at `0x1007ea6e0` and
//! `0x1007ea690`).

/// What a probe found out.
///
/// The discriminants are the values the binary returns in `w0`: `0` for a plain
/// yes, `1` for a no, `2` when the answer says nothing either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Answer {
    /// The probe worked (`magic: works`, `water: yes`).
    #[default]
    Yes,
    /// It did not (`magic: blocked`, `water: no`).
    No,
    /// The answer was inconclusive — the caller should skip this room.
    Unknown,
}

/// Whether a spell cast in this room went off.
///
/// `@0x1013c1d8c`. Three passes over the lines, in this order:
///
/// 1. a line that contained `Your magic fizzles ineffectually` (32 bytes at
///    `0x102415960`), `forces the magic words back down your throat` (44 bytes
///    at `0x102415980`) or `motions for you to stop` (23 bytes at
///    `0x1024159ac`) means magic is blocked — [`Answer::No`];
/// 2. a line that contained `lapse in your concentration` (27 bytes at
///    `0x1024159c3`) means the cast was interrupted, which says nothing about
///    the room — [`Answer::Unknown`];
/// 3. a line that *is* `You gesture` or `You channel` (11 bytes each, compared
///    whole: first eight bytes and the eight bytes from `+3`) means the cast
///    went off — [`Answer::Yes`].
///
/// Anything else, and an empty answer, is [`Answer::Unknown`].
pub fn magic(lines: &[String]) -> Answer {
    const BLOCKED: [&str; 3] = [
        "Your magic fizzles ineffectually",
        "forces the magic words back down your throat",
        "motions for you to stop",
    ];
    if lines
        .iter()
        .any(|line| BLOCKED.iter().any(|marker| line.contains(marker)))
    {
        return Answer::No;
    }
    if lines
        .iter()
        .any(|line| line.contains("lapse in your concentration"))
    {
        return Answer::Unknown;
    }
    if lines
        .iter()
        .any(|line| line == "You gesture" || line == "You channel")
    {
        return Answer::Yes;
    }
    Answer::Unknown
}

/// Whether this room can be splashed (filled with water).
///
/// `@0x1013c1fe0`. In this order:
///
/// 1. a line that contained `You just splashed yourself!` (27 bytes at
///    `0x1024159de`) — [`Answer::Yes`];
/// 2. a line that contained `How do you plan to do that here?` (32 bytes at
///    `0x1024159fe`) — that is the game refusing, [`Answer::No`];
/// 3. a line that contained `You splash about in the` (23 bytes, compared whole
///    when the line is that long) *and* `water` (5 bytes at `0x1024159f9`) —
///    [`Answer::Yes`];
/// 4. anything else, and an empty answer — [`Answer::Unknown`].
pub fn splash(lines: &[String]) -> Answer {
    for line in lines {
        if line.contains("You just splashed yourself!") {
            return Answer::Yes;
        }
        if line.contains("How do you plan to do that here?") {
            return Answer::No;
        }
        if line.contains("You splash about in the") && line.contains("water") {
            return Answer::Yes;
        }
    }
    Answer::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| text.to_string()).collect()
    }

    #[test]
    fn magic_is_blocked_by_the_three_refusals() {
        for text in [
            "Your magic fizzles ineffectually.",
            "Something forces the magic words back down your throat.",
            "A voice motions for you to stop.",
        ] {
            assert_eq!(magic(&lines(&[text])), Answer::No, "{}", text);
        }
    }

    #[test]
    fn magic_works_when_the_cast_is_seen() {
        assert_eq!(magic(&lines(&["You gesture"])), Answer::Yes);
        assert_eq!(magic(&lines(&["You channel"])), Answer::Yes);
        // A line that merely starts the same way is not the marker.
        assert_eq!(magic(&lines(&["You gesture at a kobold."])), Answer::Unknown);
    }

    #[test]
    fn a_lapsed_cast_says_nothing_about_the_room() {
        assert_eq!(
            magic(&lines(&["You lapse in your concentration."])),
            Answer::Unknown
        );
        // ... and it wins over a gesture in the same answer.
        assert_eq!(
            magic(&lines(&["You gesture", "You lapse in your concentration."])),
            Answer::Unknown
        );
    }

    #[test]
    fn a_refusal_wins_over_everything_else() {
        assert_eq!(
            magic(&lines(&[
                "You gesture",
                "motions for you to stop",
                "You lapse in your concentration.",
            ])),
            Answer::No
        );
    }

    #[test]
    fn splash_reads_the_water_messages() {
        assert_eq!(splash(&lines(&["You just splashed yourself!"])), Answer::Yes);
        assert_eq!(
            splash(&lines(&["How do you plan to do that here?"])),
            Answer::No
        );
        assert_eq!(
            splash(&lines(&["You splash about in the water."])),
            Answer::Yes
        );
    }

    #[test]
    fn splash_needs_both_halves_of_the_marker() {
        assert_eq!(
            splash(&lines(&["You splash about in the dark."])),
            Answer::Unknown
        );
        assert_eq!(splash(&lines(&["water"])), Answer::Unknown);
    }

    #[test]
    fn splash_takes_the_first_marker_it_meets() {
        // The binary walks the lines once, checking the three markers per line,
        // so the first marker of the first matching line decides.
        assert_eq!(
            splash(&lines(&[
                "You just splashed yourself!",
                "How do you plan to do that here?"
            ])),
            Answer::Yes
        );
        assert_eq!(
            splash(&lines(&[
                "How do you plan to do that here?",
                "You just splashed yourself!"
            ])),
            Answer::No
        );
    }

    #[test]
    fn nothing_said_is_unknown() {
        assert_eq!(magic(&[]), Answer::Unknown);
        assert_eq!(splash(&[]), Answer::Unknown);
        assert_eq!(magic(&lines(&["You see a fountain."])), Answer::Unknown);
        assert_eq!(splash(&lines(&["You see a fountain."])), Answer::Unknown);
    }
}
