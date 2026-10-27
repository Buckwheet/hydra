#![allow(dead_code, unused)]
//! `gs_base` — small foundations shared by the Hydra front end.
//!
//! Recovered from the arm64 Hydra binary; see `NOTES.md` for the evidence behind
//! every function, constant and the `established` / `INFERRED` / `UNKNOWN`
//! legend.
//!
//! ## Dependencies (Phase 1)
//!
//! The original crate leaned on `anyhow`, `toml` + `serde`, `zip` (with its
//! vendored inflater) and `dirs`, plus an internal `vellum_light` helper crate.
//! Phase 1 reconnected the registry crates at the pinned workspace versions:
//! `anyhow` ([`Error`] / [`Result`]), `toml` + `serde` ([`packs`]) and `zip` +
//! `flate2` ([`packs`]). `dirs` is not pinned, so [`data_dirs::home_dir`] stays a
//! std stand-in; the SHA-256 in [`packs`] stays hand-written because the original
//! had no `sha2` dependency (see `PLAN.md` and `NOTES.md`).

pub mod body;
pub mod data_dirs;
pub mod elanthian_time;
pub mod packs;
pub mod png_meta;
pub mod sky;
pub mod sky_light;
pub mod status;

/// The error type the original crate used: `anyhow::Error`.
///
/// The binary carries `anyhow::Error` payloads (8-byte non-null pointers built by
/// `anyhow::__private::format_err`) and calls `anyhow::Context::with_context` in
/// `data_dirs` and `packs`, so the real crate is reconnected directly rather
/// than imitated. Every recovered message is a plain `format!` string produced
/// verbatim by the call sites, so the observable text is unchanged.
pub use anyhow::Error;

/// `anyhow::Result<T>`.
///
/// `anyhow::Error` is neither `Clone` nor `PartialEq`, so the recovered
/// assertions compare `Error::to_string()` instead of a hand-rolled
/// `Error::message()`; the expected strings are unchanged. Use `{:?}` (or
/// `{:#}`) to see the whole context chain.
pub type Result<T> = anyhow::Result<T>;

#[cfg(test)]
mod tests {
    use super::*;

    /// [`Error`] and [`Result`] really are the crate's types.
    #[test]
    fn error_is_anyhow_error() {
        fn takes_anyhow(_: &anyhow::Error) {}

        let e: Error = anyhow::anyhow!("boom");
        takes_anyhow(&e);
        assert_eq!(e.to_string(), "boom");

        // `?` on any std error lands in `Result<T>` with no conversion code, and
        // the original error stays downcastable — the two properties the
        // hand-rolled `Error(String)` could not offer.
        fn fails() -> Result<()> {
            let _: u8 = "not a number".parse()?;
            Ok(())
        }
        let err = fails().unwrap_err();
        assert!(
            err.downcast_ref::<std::num::ParseIntError>().is_some(),
            "{err:?}"
        );
        assert_eq!(err.to_string(), "invalid digit found in string");
    }
}
