//! What the caster needs to know about the game.
//!
//! The original crate read this from `hydra_model`.  Every query the recovered
//! code makes is declared as one method on [`World`] and answered, by default,
//! by [`ModelWorld`], the adapter over the real `hydra_model` spell table and
//! known-spell set.  [`install`] swaps in another model (the tests install a
//! fixture, the session may install a model that also knows about the room).
//!
//! Every method below is named after the `hydra_model` item the disassembly
//! called at the recorded address; see `NOTES.md`.
//!
//! ```text
//! 0x101bc54a0 hydra_model::spells::spell                -> spell(number) -> Option<&Spell>
//! 0x101bc53d0 hydra_model::spells::Spell::is            -> Spell::is(kind)
//! 0x101c0e1bc hydra_model::state::known_spells::KnownSpells::knows
//! 0x101bd5010 hydra_model::state::vitals::GameState::mana
//! 0x101bcc134 hydra_model::state::spell_time::GameState::spell_cost
//! 0x101bc5568 hydra_model::spells::extras::ProcLine::read
//! ```
//!
//! Phase 1: `hydra_model` is a real dependency, so [`ModelWorld`] answers
//! [`World::spell_name`], [`World::knows`], [`World::is_attack`] and
//! [`World::seconds`] from it.  The three queries the recovered model answers
//! from *live* state (mana, the room's targets) and the proc-line table stay
//! `None`/`false` — see the per-method notes and `NOTES.md` §5.

use std::sync::OnceLock;

use hydra_model::spells::{self, Role};
use hydra_model::state::known_spells::KnownSpells;

use crate::settings::Chosen;

/// A spell's kind as far as casting cares: `Spell::is` was called with `0` in
/// `plan::plan` (0x1019b0f24) and the result was and-ed with `options.safety`,
/// so kind 0 is the "attack" question.
///
/// ESTABLISHED: `hydra_model::spells::Role`'s discriminants put `attack` at
/// `0` (`Role::Attack.index()`), so kind 0 is [`Role::Attack`] — the adapter
/// tests the row's role instead of calling `Spell::is`, whose recovered body
/// compares the row's *id*.
pub const KIND_ATTACK: u8 = 0;

/// What the caster asks the game model.
///
/// All fallible queries return `None` when the model does not know the answer;
/// the recovered code treats "unknown" as "do not act" rather than guessing.
pub trait World: Send + Sync {
    /// `hydra_model::spells::spell(number)`: the spell's name, if the game has
    /// such a spell at all.
    fn spell_name(&self, number: u16) -> Option<&str>;

    /// `KnownSpells::knows(number)`: this character knows the spell.
    fn knows(&self, number: u16) -> bool;

    /// `Spell::is(number, KIND_ATTACK)`.
    fn is_attack(&self, number: u16) -> bool;

    /// `GameState::mana()`.  The recovered return is a small integer where `1`
    /// means "the mana figure is known" and `2`/`0` mean "do not check"
    /// (0x1019b0f94: `cmp w8, #2` / `tbz w8, #0` both skip the check).
    fn mana(&self) -> Option<i64> {
        None
    }

    /// `GameState::spell_cost(number, resource)`: the multiplier and the raw
    /// cost.  `resource` is `"mana"` (0x10206f4b0) or `"second"`
    /// (0x1039c4973).  The recovered code computes `multiplier * cost` and
    /// compares it to the mana it has.
    fn spell_cost(&self, number: u16, resource: &str) -> Option<(f64, i64)>;

    /// How long the cast takes: `spell_cost(number, "second")`, multiplied out.
    ///
    /// This is the product the `"second"` call feeds in `plan::plan`; it is a
    /// separate method because `hydra_model` keeps the duration as an `f64`
    /// on the spell row (the `+0x18` field `timer_change` reads) rather than as
    /// a multiplier and an integer cost.
    fn seconds(&self, number: u16) -> Option<f64> {
        self.spell_cost(number, "second")
            .map(|(multiplier, cost)| multiplier * cost as f64)
    }

    /// Whether there is a live target here with this name.  UNKNOWN: the
    /// original asked `GameState`; the accessor is not in the crate's symbols.
    fn has_live_target(&self, target: &str) -> bool;

    /// The spell's own verb, i.e. what `settings.verb` falls back to.  The
    /// recovered `CasterSettings::verb` returns "no verb of its own" when the
    /// settings have no entry; `plan` then uses `None`.  UNKNOWN: whether the
    /// model had a verb per spell.
    fn usual_verb(&self, _number: u16) -> Option<Chosen> {
        None
    }

    /// `ProcLine::read(number)`: the spell's "proc" line, when the spell works
    /// by itself.  UNKNOWN shape; only presence is used.
    fn proc_line(&self, _number: u16) -> Option<String> {
        None
    }
}

/// A model that knows nothing.
///
/// With this installed every spell number is refused by
/// [`crate::settings::spell`].  It is kept for callers that want the caster to
/// do nothing at all; the default model is [`ModelWorld`].
pub struct Nothing;

impl World for Nothing {
    fn spell_name(&self, _number: u16) -> Option<&str> {
        None
    }
    fn knows(&self, _number: u16) -> bool {
        false
    }
    fn is_attack(&self, _number: u16) -> bool {
        false
    }
    fn spell_cost(&self, _number: u16, _resource: &str) -> Option<(f64, i64)> {
        None
    }
    fn has_live_target(&self, _target: &str) -> bool {
        false
    }
}

/// The `hydra_model`-backed model: what [`world_model`] answers while nothing
/// else is installed.
///
/// * the spell table is `hydra_model::spells::spell` (0x101bc54a0), which
///   `hydra_model::spells::set_spells` fills — the table itself is a generated
///   data file and is not embedded in the crate;
/// * "do you know it" is `hydra_model::state::known_spells::KnownSpells::knows`
///   (0x101c0e1bc) over the set handed to [`install_model_with`], if any;
/// * "is it an attack" is the row's [`Role`] (see [`KIND_ATTACK`]);
/// * the cast duration is the row's `seconds` (the `+0x18` `f64`).
///
/// The three answers that need the character's live `GameState` are not
/// available to a `&'static` adapter and are reported as unknown:
/// [`World::mana`] (`None`), [`World::has_live_target`] (`false`) and
/// [`World::spell_cost`] (`None`).  `hydra_model`'s
/// `GameState::spell_cost`/`mana` are methods on the state, and
/// `spell_cost` takes a spellsong *expression*, not `(number, resource)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelWorld {
    /// The character's known spells, when the caller has them.
    known: Option<&'static KnownSpells>,
}

impl ModelWorld {
    /// An adapter over the spell table, with (or without) a known-spell set.
    pub const fn new(known: Option<&'static KnownSpells>) -> ModelWorld {
        ModelWorld { known }
    }

    /// The known-spell set this adapter consults, if any.
    pub fn known(&self) -> Option<&'static KnownSpells> {
        self.known
    }
}

/// The default model: the spell table, no known-spell set.
static MODEL: ModelWorld = ModelWorld::new(None);

/// The `hydra_model`-backed default model.
pub fn model() -> &'static ModelWorld {
    &MODEL
}

impl World for ModelWorld {
    fn spell_name(&self, number: u16) -> Option<&str> {
        // 0x101bc54a0 `spells::spell(number)`; the row owns its name.
        spells::spell(number).map(|spell| spell.name.as_str())
    }

    fn knows(&self, number: u16) -> bool {
        // 0x101c0e1bc `KnownSpells::knows`; the recovered model keeps the set
        // by name, so the row's name is the key.
        match (self.known, self.spell_name(number)) {
            (Some(known), Some(name)) => known.knows(name),
            _ => false,
        }
    }

    fn is_attack(&self, number: u16) -> bool {
        spells::spell(number)
            .map(|spell| spell.role == Some(Role::Attack))
            .unwrap_or(false)
    }

    fn spell_cost(&self, _number: u16, _resource: &str) -> Option<(f64, i64)> {
        // `hydra_model::state::spell_time::GameState::spell_cost` (0x101bcc134)
        // is a method on the character's state and takes the spellsong
        // expression.  A `&'static` adapter has neither.
        None
    }

    fn seconds(&self, number: u16) -> Option<f64> {
        spells::spell(number).map(|spell| spell.seconds)
    }

    fn has_live_target(&self, _target: &str) -> bool {
        // Needs `GameState`'s room; see the type docs.
        false
    }
}

static INSTALLED: OnceLock<&'static dyn World> = OnceLock::new();

/// Install the game model.  Returns `false` if one was already installed.
pub fn install(world: &'static dyn World) -> bool {
    INSTALLED.set(world).is_ok()
}

/// The installed game model, if the caller installed one.
pub fn world() -> Option<&'static dyn World> {
    INSTALLED.get().copied()
}

/// The model the caster uses: the installed one, or [`model`].
///
/// This is what replaces the std stand-in's "no model at all" default: the
/// caster is always backed by the real `hydra_model` spell table, and empty
/// table or not, nothing is invented.
pub fn world_model() -> &'static dyn World {
    match INSTALLED.get() {
        Some(world) => *world,
        None => model(),
    }
}

/// Install [`ModelWorld`] over the spell table alone.
///
/// Returns `false` if a model was already installed.
pub fn install_model() -> bool {
    install(model())
}

/// Install [`ModelWorld`] with the character's known spells.
///
/// The adapter is leaked so that it can be handed out as `&'static`; it is a
/// single three-word value for the life of the process.
pub fn install_model_with(known: &'static KnownSpells) -> bool {
    let world: &'static ModelWorld = Box::leak(Box::new(ModelWorld::new(Some(known))));
    install(world)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_with_no_known_spells_knows_nothing() {
        let model = ModelWorld::new(None);
        assert!(model.known().is_none());
        // The empty spell table refuses everything, so both sides agree.
        assert_eq!(model.spell_name(901), None);
        assert!(!model.knows(901));
        assert!(!model.is_attack(901));
        assert_eq!(model.mana(), None);
        assert_eq!(model.seconds(901), None);
        assert!(!model.has_live_target("kobold"));
        // `world_model` never answers `None`: the adapter is the default.
        assert!(world_model().spell_name(901).is_none());
    }

    #[test]
    fn a_known_spell_set_is_what_knows_asks() {
        let known: &'static KnownSpells = Box::leak(Box::new(KnownSpells::default()));
        let model = ModelWorld::new(Some(known));
        assert_eq!(model.known(), Some(known));
        // No spell table in this test binary, so no name to look up.
        assert!(!model.knows(901));
    }
}
