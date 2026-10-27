//! The effects/timers widget.
//!
//! `hydra_gui::widget::effects::{OwnRows, EffectRows, TimersLook, EveryEffects}`
//! are named by the unresolved-symbol list; none of them was stubbed.

/// Which rows the effects widget shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectRows {
    pub own: bool,
    pub every: bool,
}
