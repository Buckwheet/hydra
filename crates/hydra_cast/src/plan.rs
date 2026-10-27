//! Turning one cast into the lines the game wants.
//!
//! `hydra_cast::plan::plan` (0x1019b0ecc, 6084 bytes) is a validation pipeline
//! followed by a command builder.  The order of the checks and every message
//! were read out of the disassembly:
//!
//! ```text
//! 0x1019b0f0c  hydra_model::spells::spell(number)          null -> Err
//! 0x1019b0f40  `lsr w8, w20, #3; cmp w8, #0x465`           number < 9000 gate
//! 0x1019b0f50  KnownSpells::knows(number)                  false -> Err
//! 0x1019b0f80  options.conserve  (byte at settings+0x61)
//! 0x1019b0f8c  GameState::mana()
//! 0x1019b104c  GameState::spell_cost(number, "mana")
//! 0x1019b10c0  options.safety    (byte at settings+0x62)
//! 0x1019b1514  options.channel   (byte at settings+0x60)
//! 0x1019b15a8  CasterSettings::verb(number, ..)
//! 0x1019b1720  spells::extras::ProcLine::read
//! ```
//!
//! Messages, with the literals as they are stored:
//!
//! ```text
//! 0x103e7b536  "{} is not a spell."
//! 0x103e7b521  "You do not know {}."
//! 0x103e7b4c4  "Not enough mana to cast {}."
//! 0x103e7b4fd  "No live target here to cast {} at."
//! 0x103e7b4e1  "No live target here is {}."
//! 0x103e7b549  "{} asks nothing to be done: it works by itself."
//! 0x103e7b579  "{} is not cast by a command."
//! 0x103e6c6d3  "prepare {}"     0x103e6c6dd  "cast {}"
//! 0x103e7b596  "cast{}"         0x103e7b59d  "cast {}{}"
//! 0x103e7b5a6  "target {}"      0x103e7b5b0  "incant {}{}"
//! 0x103e7b5bb  "incant {} {}{}"
//! 0x1039c497a/0x1039c4981/0x1039c4986  ["channel", "evoke", "channel evoke"]
//! 0x10206f4b0  "mana"           0x1039c4973  "second"
//! 0x1039c4965  "offensive"
//! ```
//!
//! UNKNOWN: the exact interleaving of the `prepare`/`target`/`cast` lines (the
//! builder is one long chain of `format!` calls whose arguments could not all
//! be followed), and the flag that decides between `evoke` and `channel evoke`.

use crate::game::World;
use crate::settings::{CasterSettings, Chosen};

/// The spells above this number are not in the game's own spell list, so the
/// "do you know it" question is not asked (0x1019b0f40: `number / 8 < 0x465`).
pub const LAST_KNOWN_SPELL: u16 = 9000;

/// The stance aimed spells are cast from when `options.stance` is on.  The
/// literal is at 0x1039c4965.
pub const AIMED_STANCE: &str = "offensive";

/// The verb suffixes the builder appends to an `incant` line (the three-entry
/// table at 0x10449c628 / 0x1039c4998).
pub const VERB_SUFFIXES: [&str; 3] = ["channel", "evoke", "channel evoke"];

/// What a cast is for.
///
/// ESTABLISHED: the spell number is a `u16` at offset 0x38 of the argument
/// (`ldrh w20, [x0, #0x38]` at 0x1019b0f00).  UNKNOWN: the rest of the
/// original's fields; the target is carried here because the builder formats
/// `target {}` and `cast{}` with one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cast {
    /// The spell number.
    pub spell: u16,
    /// The word after the cast command, if any.
    pub target: Option<String>,
}

impl Cast {
    /// A cast with no target.
    pub fn new(spell: u16) -> Cast {
        Cast {
            spell,
            target: None,
        }
    }

    /// A cast at a target.
    pub fn at(spell: u16, target: &str) -> Cast {
        Cast {
            spell,
            target: Some(target.to_string()),
        }
    }
}

/// What to send, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// The stance to take before casting, when `options.stance` is on and the
    /// spell is aimed: the recovered literal is [`AIMED_STANCE`].
    pub stance: Option<&'static str>,
    /// The lines to send, in order.
    pub lines: Vec<String>,
    /// `spell_cost(number, "second")`: how long the cast takes.  `0.0` when the
    /// model does not know.
    pub seconds: f64,
}

/// Build the plan for one cast.
///
/// UNKNOWN: the original returns a niche-tagged enum by value; it is modelled
/// as `Result<Plan, String>` because every `format!` it builds becomes either
/// a line of the plan or an error message.
pub fn plan(cast: &Cast, settings: &CasterSettings, world: &dyn World) -> Result<Plan, String> {
    let number = cast.spell;

    // 1. The game must have the spell at all.
    let name = match world.spell_name(number) {
        Some(name) => name,
        // 0x103e7b536 "{} is not a spell."
        None => return Err(format!("{} is not a spell.", number)),
    };

    // 2. Below LAST_KNOWN_SPELL the character must know it.
    if number < LAST_KNOWN_SPELL && !world.knows(number) {
        // 0x103e7b521 "You do not know {}."
        return Err(format!("You do not know {}.", name));
    }

    // 3. `conserve`: enough mana, before starting.
    if settings.options.conserve {
        if let (Some(have), Some((multiplier, cost))) = (
            world.mana(),
            world.spell_cost(number, "mana"),
        ) {
            let cost = if cost == 0 { 1.0 } else { cost as f64 };
            if cost * multiplier > have as f64 {
                // 0x103e7b4c4 "Not enough mana to cast {}."
                return Err(format!("Not enough mana to cast {}.", name));
            }
        }
    }

    // 4. `safety`: no attack spell without a live target here.
    if settings.options.safety && world.is_attack(number) {
        let live = cast
            .target
            .as_deref()
            .map(|target| world.has_live_target(target))
            .unwrap_or(false);
        if !live {
            return Err(match cast.target {
                // 0x103e7b4fd "No live target here to cast {} at."
                Some(_) => format!("No live target here to cast {} at.", name),
                // 0x103e7b4e1 "No live target here is {}."
                None => format!("No live target here is {}.", name),
            });
        }
    }

    // 5. A spell that works by itself, or that has no command, cannot be cast.
    if world.proc_line(number).is_some() {
        // 0x103e7b549 "{} asks nothing to be done: it works by itself."
        return Err(format!("{} asks nothing to be done: it works by itself.", name));
    }

    // 6. The verb: the settings first, then the spell's own.
    let chosen = settings
        .verb(number, None)
        .or_else(|| world.usual_verb(number));

    let mut plan = Plan {
        stance: if settings.options.stance && world.is_attack(number) {
            Some(AIMED_STANCE)
        } else {
            None
        },
        lines: Vec::new(),
        // `spell_cost(number, "second")`, multiplied out; the real model
        // answers it from the spell row's own duration.
        seconds: world.seconds(number).unwrap_or(0.0),
    };

    // 7. The prepare line.
    //
    // UNKNOWN: the separated `incant {}{}` literal has no space between the
    // name and the suffix, so the recovered command text is built exactly that
    // way; a game that needs a separator would have to put it in the name.
    // UNKNOWN: 0x1019b1514 reads `options.channel` and that byte feeds the
    // suffix selection as well, so with the option on a spell with no verb of
    // its own may be channelled instead of prepared.  That predicate was not
    // recovered; only the verb decides here.
    match chosen {
        Some(Chosen::Channel) => plan
            .lines
            .push(format!("incant {}{}", name, VERB_SUFFIXES[0])),
        Some(Chosen::Evoke) => plan
            .lines
            .push(format!("incant {}{}", name, VERB_SUFFIXES[1])),
        // 0x103e6c6d3 "prepare {}" — cast, or no verb of its own.
        Some(Chosen::Cast) | None => plan.lines.push(format!("prepare {}", name)),
    }

    // 8. The target line, then the cast line.
    if let Some(target) = &cast.target {
        plan.lines.push(format!("target {}", target));
        plan.lines.push(format!("cast {}", target));
    } else {
        // "cast{}" with an empty suffix is just the verb.
        plan.lines.push("cast".to_string());
    }

    Ok(plan)
}
