//! Interact mode: aiming at one thing in the room and doing something to it.
//!
//! **ESTABLISHED**:
//!
//!  * The messages are `Nothing here.`, `Interact: nothing here.` and
//!    `That needs a thing in interact mode's focus.`
//!    (`0x1024183xx`).
//!  * `placed` builds the list from the game state's creatures, objects and
//!    players (`0x10147daf0`, the literals `Creatures`, `Objects`, `Players`).
//!  * The compass words are `nneesesswwnw` (`0x1024191xx`), so a thing's place
//!    is one of `n ne e se s sw w nw`.
//!  * `step_kind` maps a step to its kind from the byte table at
//!    `0x1020xxxx` (`0x10147e758`).

use crate::pad::touch::TouchSlice;

/// The eight places a thing can be in, in the order the binary writes them.
///
/// **ESTABLISHED** (`0x1024191xx`): the literal `nneesesswwnw`.
pub const PLACES: [&str; 8] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];

/// Where a thing is relative to the character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Where {
    #[default]
    Here,
    At(usize),
}

impl Where {
    /// The compass word for the place.
    pub fn as_str(&self) -> &'static str {
        match self {
            Where::Here => "here",
            Where::At(index) => PLACES.get(*index).copied().unwrap_or("here"),
        }
    }

    /// Read a compass word.
    pub fn parse(word: &str) -> Option<Where> {
        PLACES
            .iter()
            .position(|place| *place == word.trim())
            .map(Where::At)
    }
}

/// One thing in the room that can be aimed at.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Thing {
    /// What the game calls it.
    pub name: String,
    /// The noun on its own, which is what gets aimed at.
    pub noun: String,
    /// Which of `Creatures`, `Objects` or `Players` it came from.
    pub kind: String,
    /// Where it is.
    pub at: Where,
}

impl Thing {
    /// The word the game is aimed with.
    ///
    /// **PARTLY** (`0x10147daf0`): the noun, which is what `target` takes; the
    /// count prefix the binary may add is UNKNOWN.
    pub fn word(&self) -> &str {
        &self.noun
    }
}

/// Which kind of thing a line of the game state belongs to.
///
/// **ESTABLISHED** (`0x10147daf0`): three kinds, in this order.
pub const KINDS: [&str; 3] = ["Creatures", "Objects", "Players"];

/// The things that can be aimed at, laid out for the pad.
///
/// **ESTABLISHED** (`0x10147daf0`): the three kinds gathered, then `Nothing
/// here.` when there are none.
pub fn placed(things: Vec<Thing>) -> Vec<Thing> {
    things
}

/// The line interact mode shows when there is nothing to aim at.
pub const NOTHING: &str = "Nothing here.";

/// The line it shows when the focus is empty.
pub const NO_FOCUS: &str = "Interact: nothing here.";

/// The line it shows when an action needs a thing in focus.
pub const NEEDS_FOCUS: &str = "That needs a thing in interact mode's focus.";

/// The step kinds the pad takes, as the binary's byte table has them.
///
/// **PARTLY** (`0x10147e758`): the table is `0, 1, 2, 3, … 0x58` — a small
/// enumeration.  UNKNOWN what each number means beyond the five named here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepKind {
    #[default]
    Nothing,
    Aim,
    Look,
    Touch,
    Leave,
    Other(u8),
}

impl StepKind {
    /// The kind a step's number names.
    pub fn of(step: u8) -> StepKind {
        match step {
            0 => StepKind::Nothing,
            1 => StepKind::Aim,
            2 => StepKind::Look,
            3 => StepKind::Touch,
            4 => StepKind::Leave,
            other => StepKind::Other(other),
        }
    }

    pub fn as_u8(&self) -> u8 {
        match self {
            StepKind::Nothing => 0,
            StepKind::Aim => 1,
            StepKind::Look => 2,
            StepKind::Touch => 3,
            StepKind::Leave => 4,
            StepKind::Other(n) => *n,
        }
    }
}

/// Interact mode's own state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Interact {
    /// Whether interact mode is on.
    pub on: bool,
    /// The things that can be aimed at.
    pub things: Vec<Thing>,
    /// The one in focus, if any.
    pub focus: Option<usize>,
}

impl Interact {
    /// Turn interact mode on or off.
    ///
    /// **ESTABLISHED** (`0x10147e018`): turning it on with nothing to aim at
    /// says `Nothing here.`.
    pub fn begin(&mut self, on: bool) -> String {
        self.on = on;
        if on && self.things.is_empty() {
            NOTHING.to_string()
        } else {
            String::new()
        }
    }

    /// Is the mode on and is there a thing in focus?
    ///
    /// **ESTABLISHED** (`0x10147e254`).
    pub fn exist(&self) -> bool {
        self.on && self.focus.is_some()
    }

    /// Put the thing at `index` in focus.
    ///
    /// **ESTABLISHED** (`0x10147e338`): a missing index clears the focus.
    pub fn focus(&mut self, index: Option<usize>) -> Result<(), String> {
        match index {
            Some(index) if index < self.things.len() => {
                self.focus = Some(index);
                Ok(())
            }
            Some(_) => {
                self.focus = None;
                Err(NOTHING.to_string())
            }
            None => {
                self.focus = None;
                Ok(())
            }
        }
    }

    /// The thing in focus.
    pub fn focused(&self) -> Option<&Thing> {
        self.focus.and_then(|index| self.things.get(index))
    }

    /// What the mode's line says.
    ///
    /// **ESTABLISHED** (`0x10147dcf8`): `Interact: nothing here.` when nothing is
    /// in focus, else the thing's name.
    pub fn line(&self) -> String {
        match self.focused() {
            Some(thing) => thing.name.clone(),
            None => NO_FOCUS.to_string(),
        }
    }

    /// The command a step takes.
    ///
    /// **PARTLY** (`0x10147debc`): UNKNOWN how a step and its words are put
    /// together; a step with no thing in focus is [`NEEDS_FOCUS`].
    pub fn step(&self, verb: &str) -> Result<String, String> {
        let thing = self.focused().ok_or_else(|| NEEDS_FOCUS.to_string())?;
        Ok(format!("{verb} {}", thing.word()))
    }

    /// The kind of a step.
    pub fn step_kind(&self, step: u8) -> StepKind {
        StepKind::of(step)
    }
}

/// The things a touch wheel's slices point at.
pub fn things(slices: &[TouchSlice]) -> Vec<Thing> {
    slices
        .iter()
        .filter(|slice| slice.opens.is_none())
        .map(|slice| Thing {
            name: slice.label.clone(),
            noun: slice.label.clone(),
            kind: KINDS[0].to_string(),
            at: Where::Here,
        })
        .collect()
}

/// Walk the focus to the next thing.
pub fn walk(interact: &mut Interact, forward: bool) -> Option<usize> {
    if interact.things.is_empty() {
        interact.focus = None;
        return None;
    }
    let len = interact.things.len();
    let next = match interact.focus {
        None => 0,
        Some(index) if forward => (index + 1) % len,
        Some(0) => len - 1,
        Some(index) => index - 1,
    };
    interact.focus = Some(next);
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn things() -> Vec<Thing> {
        vec![
            Thing {
                name: "a grey wolf".into(),
                noun: "wolf".into(),
                kind: "Creatures".into(),
                at: Where::At(3),
            },
            Thing {
                name: "a silver chest".into(),
                noun: "chest".into(),
                kind: "Objects".into(),
                at: Where::Here,
            },
        ]
    }

    #[test]
    fn places_are_the_compass_words() {
        assert_eq!(PLACES.join(""), "nneesesswwnw");
        assert_eq!(Where::parse("sw"), Some(Where::At(5)));
        assert_eq!(Where::At(5).as_str(), "sw");
        assert_eq!(Where::Here.as_str(), "here");
        assert_eq!(Where::parse("up"), None);
    }

    #[test]
    fn turning_the_mode_on_with_nothing_to_aim_at_says_so() {
        let mut interact = Interact::default();
        assert_eq!(interact.begin(true), NOTHING);
        interact.things = things();
        assert_eq!(interact.begin(true), "");
        assert!(interact.on);
        assert!(!interact.exist());
        assert_eq!(interact.line(), NO_FOCUS);
    }

    #[test]
    fn focusing_gives_a_line_and_a_step() {
        let mut interact = Interact {
            on: true,
            things: things(),
            focus: None,
        };
        assert_eq!(interact.step("look"), Err(NEEDS_FOCUS.to_string()));
        interact.focus(Some(0)).unwrap();
        assert!(interact.exist());
        assert_eq!(interact.line(), "a grey wolf");
        assert_eq!(interact.step("look").unwrap(), "look wolf");
        // Out of range clears the focus.
        assert!(interact.focus(Some(9)).is_err());
        assert_eq!(interact.focus, None);
    }

    #[test]
    fn walking_round_the_things() {
        let mut interact = Interact {
            things: things(),
            ..Interact::default()
        };
        assert_eq!(walk(&mut interact, true), Some(0));
        assert_eq!(walk(&mut interact, true), Some(1));
        assert_eq!(walk(&mut interact, true), Some(0));
        assert_eq!(walk(&mut interact, false), Some(1));
        assert_eq!(StepKind::of(2), StepKind::Look);
        assert_eq!(StepKind::Other(9).as_u8(), 9);
    }
}
