//! The `mural_of_deities` routine.
//!
//! **Recovered:** `0x10156ac38`: `touch mural` and `The face falls quiet and goes still.` (`0x102420bf7`), plus the twenty-deity poem table at `0x10443cf78` (see `DEITIES` below).

use super::{Next, Routine, See, Solver};

/// The `mural_of_deities` solver.
#[derive(Clone, Debug, Default)]
pub struct MuralOfDeities {
    /// The room's routine data.
    pub routine: Routine,
    /// Which command of the sequence comes next.
    pub step: usize,
}

impl MuralOfDeities {
    /// The kind word this solver answers to.
    pub const KIND: &'static str = "mural_of_deities";

    /// The recovered command sequence.
    pub const COMMANDS: &'static [&'static str] = &[
        "touch mural",
    ];

    /// The lines that belong to this routine.
    pub const KEEPS: &'static [&'static str] = &[
        "The face falls quiet and goes still.",
    ];

    pub fn new(routine: Routine) -> MuralOfDeities {
        MuralOfDeities {
            routine,
            step: 0,
        }
    }
}

impl Solver for MuralOfDeities {
    fn next(&mut self, _see: &See) -> Next {
        // The recovered body issues the sequence above in order; the
        // predicate that decides each step has to come from the room's
        // prose, which is not recoverable.
        match Self::COMMANDS.get(self.step) {
            Some(command) => {
                self.step += 1;
                Next::Say((*command).to_string())
            }
            None => Next::Done,
        }
    }

    fn keep(&mut self, line: &str) -> bool {
        Self::KEEPS.iter().any(|needle| line.contains(needle))
    }

    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_word() {
        let solver = MuralOfDeities::new(Routine::named(MuralOfDeities::KIND));
        assert_eq!(solver.kind(), "mural_of_deities");
    }

    #[test]
    fn issues_the_recovered_commands_in_order() {
        let mut solver = MuralOfDeities::new(Routine::named(MuralOfDeities::KIND));
        let mut issued = Vec::new();
        loop {
            match solver.next(&See::default()) {
                Next::Say(command) => issued.push(command),
                _ => break,
            }
        }
        assert_eq!(issued, MuralOfDeities::COMMANDS.to_vec());
    }

    #[test]
    fn keeps_its_own_lines() {
        let mut solver = MuralOfDeities::new(Routine::named(MuralOfDeities::KIND));
        for needle in MuralOfDeities::KEEPS {
            assert!(solver.keep(needle), "{needle}");
        }
        assert!(!solver.keep("Nothing to do with this room."));
    }
}

/// The twenty deities of the mural and their five-line verses.
///
/// **ESTABLISHED**, extracted verbatim from the static table at
/// `0x10443cf78`: twenty `(name: &str, verses: &[&str])` pairs at stride
/// `0x20`, whose verse arrays live in the contiguous `&str` block at
/// `0x10443c8b8`. Four of the verse arrays are longer than five lines —
/// `Jastev` and `Koar` have eight, `Ronan` and `Tonis` have six — which is
/// why the block holds 108 lines for twenty deities.
pub const DEITIES: [(&str, &[&str]); 20] = [
    ("Andelas", &[
        "A black cat crosses o'er a path of blood",
        "The crimson claw, the fang, the chase, the play",
        "The mice will play until the hunger bites",
        "There welcome lies, midst claws that spread with glee",
        "Yet one may smile, and be a predator",
    ]),
    ("Charl", &[
        "No mercy lies in sea's tempestuous rage",
        "One ship is saved, one lost in tempest's din",
        "Still waters gather, 'ware the eye of storms",
        "The blood of tyrants feeds the hurricane",
        "The sailor lives to praise the storm-wild sea",
    ]),
    ("Eonak", &[
        "Cold stone be given life by fiery forge",
        "Creation's soul, love's labor, earth and fire",
        "Let hammer strike the stone, the stone create",
        "The craftsman works, the iron becomes his hand",
        "The forger draws hard beauty from the ore",
    ]),
    ("Fash'lo'nae", &[
        "Cold yellow eye draws magic from a rune",
        "Dark flame of knowledge burns the midnight lamp",
        "The face of wisdom in the mirror crack'd",
        "The old one, so 'tis said, is he who seeks",
        "Thou feed'st the hearth of knowledge with a fire",
    ]),
    ("Gosaena", &[
        "In cold grey stone waits pale all-seeing eye",
        "No mortal knows the veil beyond the gate",
        "On silent wings, and peace beyond the dark",
        "Silence is silver, grey eternity",
        "The sickle falls and reaps the cycle's pause",
    ]),
    ("Imaera", &[
        "Dance on the wild green, child of the wood",
        "In autumn's chill the harvest golden reap'd",
        "The healing herb, the oak, the barley grow",
        "The silent doe reclines in dappled glade",
        "To nature's sylvan bounty raise a toast",
    ]),
    ("Ivas", &[
        "Beauty belies corruption to the core",
        "Behold temptation, bright deceiving lie",
        "Fine silk and velvet drape the dark decay",
        "The butterfly may twist into a wyrm",
        "Ye soft and bitter, sharp and demon-sweet",
    ]),
    ("Jastev", &[
        "Carry the weight of ages before time",
        "Death will yet come, still beauty lies in store",
        "Preserve the beauty of the dark and light",
        "Preserve the past for those who come anon",
        "Seer of sad shadows, painter of Mystery",
        "Sing prophecy of poets in the night",
        "Speak into silence, silence answers back",
        "The artist weeps, old tears becoming paint",
    ]),
    ("Jaston", &[
        "A snow-white feather dances on the air",
        "A sylvan zephyr rides the winds of light",
        "Above the green, white wings may soar afar",
        "Fly on the mistral wind, bright bird of song",
        "O love, bright spirit, rise on feathered wings",
    ]),
    ("Koar", &[
        "A golden crown shines bright in moonlight grey",
        "All wisdom at thy side, rule o'er the moon",
        "Great mountain rises, crowned in sunlight gold",
        "Keep alert watch unquieted by sleep",
        "O King, keep watch from far beyond the ice",
        "The bringer of tense peace to light and dark",
        "The wisdom of the Law the balance keep",
        "Thy word is law, great majesty of old",
    ]),
    ("Laethe", &[
        "A pang of sorrow calls a long-lost love",
        "Compassion doth within great sorrow lie",
        "Return, O love, and mend a longing heart",
        "The black rose grows among the purple graves",
        "Young love reflected in a pool of tears",
    ]),
    ("Lorminstra", &[
        "Give all to life, that ye may death defy",
        "Hold fast the golden key that binds the gate",
        "In winter's cold doth life begin anew",
        "The key, the gate, to be or not to be",
        "The touch of death melts like the winter snow",
    ]),
    ("Luukos", &[
        "Death's minions scream and wail in twisted dark",
        "Fear ye the serpent, mockery of death",
        "In promises of life twists falsehood black",
        "Serpent slithers among the fallen horde",
        "Unending torment feeds on souls of lies",
    ]),
    ("Marlu", &[
        "A demon's wings beat time in shadow vile",
        "Ancient destroyer, lord of what was not",
        "Black shadow moves in timeless pit of ruin",
        "Guard ye the veil, lest demons darkness free",
        "Still-living ichor drips from blackened stone",
    ]),
    ("Onar", &[
        "A dark blade's hiss disturbs the still night air",
        "All life has price, and death does not come cheap",
        "Cold death so swift, a whisper in the dark",
        "O sudden darkness, death a silent strike",
        "The white skull grins, the dark shape on the floor",
    ]),
    ("Ronan", &[
        "I shroud my prayers in darkest cloak of night",
        "In dreams may darkness find a peaceful sleep",
        "O silver blade, flash brightly in the night",
        "Protector of the night, keep safe the dark",
        "Walk brave in endless shadow, guard the door",
        "Watch o'er the night, so we might keep the day",
    ]),
    ("Sheru", &[
        "A nightmare drips black blood upon the stone",
        "Behold dark dreams of nightmare gallery",
        "False gold in dreams of blackness, crimson blood",
        "In moon-dark shadows howls the jackal wild",
        "Stark scream of terror pleads for break of day",
    ]),
    ("Tonis", &[
        "A breath of life, a trinket to a thief",
        "Across the sky, a streak of flaming gold",
        "Be swift of foot and mind, and quickly fly",
        "On golden wings, a blur of wind and haste",
        "On wings begat of freedom prayers take flight",
        "The hand so quick, the surest eye is lost",
    ]),
    ("Voaris", &[
        "A golden rose lies soft on sanguine field",
        "A king mayst not deny love pure and true",
        "Hold fast thy hands, and love shall find its way",
        "The love forbidden, fate shall not deny",
        "Young love is golden to the true of heart",
    ]),
    ("Zelia", &[
        "A pixie's laugh, a dark demented soul",
        "A silver crescent flickers in thine eye",
        "Beyond the moons, mad laughter calls to thee",
        "The gift of madness touches whom it may",
        "Wild freedom lies beneath chaotic moon",
    ]),
];

#[cfg(test)]
mod mural_tests {
    use super::*;

    #[test]
    fn the_mural_has_the_recovered_twenty_deities() {
        let names: Vec<&str> = DEITIES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names.len(), 20);
        assert_eq!(names[0], "Andelas");
        assert_eq!(names[19], "Zelia");
        assert!(names.contains(&"Fash'lo'nae"));
        assert_eq!(DEITIES.iter().map(|(_, v)| v.len()).sum::<usize>(), 108);
    }

    #[test]
    fn the_verses_are_the_recovered_lines() {
        let andelas = DEITIES[0].1;
        assert_eq!(andelas[0], "A black cat crosses o'er a path of blood");
        assert_eq!(andelas[4], "Yet one may smile, and be a predator");
        let zelia = DEITIES[19].1;
        assert_eq!(zelia[4], "Wild freedom lies beneath chaotic moon");
        assert_eq!(DEITIES[7].0, "Jastev");
        assert_eq!(DEITIES[7].1.len(), 8);
    }
}
