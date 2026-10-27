#![allow(dead_code, unused)]
//! `gs_base` — small, dependency-free foundations shared by the Hydra front end.
//!
//! Recovered from the arm64 Hydra binary; see `NOTES.md` for the evidence behind
//! every function, constant and the `established` / `INFERRED` / `UNKNOWN`
//! legend. Only `std` is used: the original crate leaned on `anyhow`, `toml`,
//! `zip`, `sha2`, `dirs` and an internal `vellum_light` helper crate; each of
//! those has a hand-written std equivalent here.

pub mod body;
pub mod data_dirs;
pub mod elanthian_time;
pub mod packs;
pub mod png_meta;
pub mod sky;
pub mod sky_light;
pub mod status;

use std::fmt;

/// Error type standing in for the `anyhow::Error` the original crate used.
///
/// The binary carries `anyhow::Error` payloads (8-byte non-null pointers built by
/// `anyhow::__private::format_err`), and every recovered message is a plain
/// `format!` string, so a boxed message reproduces the observable behaviour
/// without the dependency.  **std departure.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn msg(m: impl Into<String>) -> Self {
        Error(m.into())
    }
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

/// `anyhow::Result` stand-in.
pub type Result<T> = std::result::Result<T, Error>;
