//! `hydra_lich` — the Lich relay and child-process integration.
//!
//! This is a std-only reconstruction of the original crate (which used
//! `tokio`, `getrandom` and `hydra_session::lich::LichDoor`).  The binary's
//! addresses are recorded on each item; see `NOTES.md` for the recovered
//! IPC protocol and the established/UNKNOWN split.

#![allow(dead_code, unused)]

pub mod child;
pub mod relay;

pub use child::{
    find_ruby, find_ruby_in, hex_lower, is_password, new_token, newest_version_dir, TokenError,
    PASSWORD_PREFIX, RUBY4LICH5_ROOT,
};
