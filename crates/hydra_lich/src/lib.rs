//! `hydra_lich` — the Lich relay and child-process integration.
//!
//! The binary's addresses are recorded on each item; see `NOTES.md` for the
//! recovered IPC protocol, the established/UNKNOWN split and the Phase 1
//! reconnection table.
//!
//! Phase 1 reconnected the real `tokio` (1.53.1) and
//! `tokio_util::sync::CancellationToken` (0.7.19): the async entry points
//! ([`relay::read_lines_async`], [`relay::show_async`], [`relay::run_async`])
//! are tokio-native, and the original synchronous API is kept as a facade
//! driven by a shared runtime ([`runtime`]).
//!
//! Still stand-ins (see `NOTES.md` §4):
//! * `getrandom` — not in the workspace pins, so [`child::new_token`] reads
//!   `/dev/urandom`;
//! * `hydra_session::lich::LichDoor` — a sibling reconstruction with a
//!   different `Notice`/`Tap` shape, so the local [`relay::Relay`],
//!   [`relay::Notice`] and [`relay::Typed`] equivalents stay.

#![allow(dead_code, unused)]

pub mod child;
pub mod relay;
pub mod runtime;

pub use child::{
    command, command_async, filter_password_env, filter_password_env_tokio, find_ruby, find_ruby_in,
    hex_lower, is_password, new_token, newest_version_dir, TokenError, PASSWORD_PREFIX,
    RUBY4LICH5_ROOT,
};
pub use relay::{
    build_command, build_command_async, keep_errors, keep_errors_async, never_given, read_lines,
    read_lines_async, run, run_async, run_until, shared_symbol, show, show_async, spawn,
    spawn_async, BlockingReader, Ended, Launch, Notice, Relay, RelayAsync, Spawned, SpawnedAsync,
    Step, Typed, Waits, HAND_HOLD_GRACE, KEPT_ERRORS, LICH_ARGS, LINE_CHANNEL_CAPACITY,
    RUBY_LOAD_LICH, RUBY_PRELUDE,
};
