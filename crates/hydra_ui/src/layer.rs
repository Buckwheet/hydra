//! Layered settings.
//!
//! Every theme type in the binary implements the same three operations:
//!
//! * `over(&self, under: &Self) -> Self`      — put `self` on top of `under`
//!   (the field is taken from `self` when it is set, otherwise from `under`);
//! * `differing(&self, base: &Self) -> Self`  — only the fields that differ
//!   from `base`, which is what a theme file stores so it can be read over a
//!   built-in theme;
//! * `is_empty(&self) -> bool`                — nothing set at all.
//!
//! The original expressed this with `serde` + `Option` fields (each `settings`
//! struct in the string pool ends in the shared field list).  This module gives
//! the same three operations a name so the per-type code stays short.

use crate::toml::Value;

/// A settings layer that can be stacked over another one.
pub trait Over: Sized {
    /// `self` on top of `under`.
    fn over(&self, under: &Self) -> Self;
    /// Only the fields that differ from `base`.
    fn differing(&self, base: &Self) -> Self;
    /// Nothing is set.
    fn is_empty(&self) -> bool;
    /// Read one TOML value.
    fn from_value(v: &Value) -> Option<Self>;
    /// Write this layer back to TOML.
    fn to_value(&self) -> Value;

    /// Helper for `Option<T>` fields.
    fn take<T: Clone>(a: &Option<T>, b: &Option<T>) -> Option<T> {
        a.clone().or_else(|| b.clone())
    }
}
