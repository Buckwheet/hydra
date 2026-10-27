//! Seam: what the caster needs to know about the game.
//!
//! The original crate read this from `hydra_model` (and, for the command sink,
//! from `hydra_session`).  Neither may be depended on here yet, so every query
//! the recovered code makes is declared as one method on [`World`] and installed
//! once at start-up with [`install`].
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

use std::sync::OnceLock;

use crate::settings::Chosen;

/// A spell's kind as far as casting cares: `Spell::is` was called with `0` in
/// `plan::plan` (0x1019b0f24) and the result was and-ed with `options.safety`,
/// so kind 0 is the "attack" question.
///
/// UNKNOWN: the rest of `Spell::is`'s kinds.
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
/// This is the default so that nothing is invented: with no [`World`]
/// installed every spell number is refused by [`crate::settings::spell`].
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

static INSTALLED: OnceLock<&'static dyn World> = OnceLock::new();

/// Install the game model.  Returns `false` if one was already installed.
pub fn install(world: &'static dyn World) -> bool {
    INSTALLED.set(world).is_ok()
}

/// The installed game model, if any.
pub fn world() -> Option<&'static dyn World> {
    INSTALLED.get().copied()
}
