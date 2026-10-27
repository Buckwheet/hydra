//! The startup timeline: `hydra::started`.
//!
//! `hydra::main`'s very first call is `hydra::started::begin` (`0x1001dd1f8`
//! … `0x1001dd2b4`, called from `0x10021d844`), i.e. *before* the argument
//! vector, the runtime and the data folder: the reference build wants a
//! timestamp for the whole launch.
//!
//! Recovered pieces:
//!
//! * `begin` (`0x1001dd1f8`): `std::time::Instant::now` (`0x1001dd208`), then
//!   fills the static `hydra::started::START` (`0x1044fd460`) — a
//!   `OnceLock<Mutex<Start>>` (`OnceLock::initialize` at `0x1001dd28c`,
//!   `Mutex<Start>` drop glue at `0x1001dd26c`) — from a 16-byte `__const`
//!   template at `0x10206ca40`.  Subsequent calls are no-ops (the `ldapr` /
//!   `cbnz` at `0x1001dd238` returns early).  **ESTABLISHED.**
//! * `stage` (`0x1001dd2b4`): returns immediately unless `START` was
//!   initialised; locks the start state, formats `Instant::elapsed` as
//!   seconds with a fractional part (`ucvtf` + `fdiv` by the double
//!   `0x41cdcd6500000000` = 1e9 at `0x1001dd390`), and hands the result to
//!   [`write`].  **ESTABLISHED shape, UNKNOWN wording** — the piece table was
//!   not decoded, so the text below is ours.
//! * `write` (`0x1001dd4d4`): `metadata` on the target file, `OpenOptions`
//!   open, `core::fmt::write` into it and `_eprint` (`0x1001dd4d4`'s `bl`s),
//!   i.e. the stage line goes to *both* a startup log and stderr.  The file
//!   path is built by the caller (`hydra::gui::run` calls `stage` four times,
//!   `0x1001103f4`); it is one of the data-folder files a later phase wires.
//! * `log_in` (`0x1001dd688`, 3076 bytes): the login timeline entries.
//!
//! Only [`begin`] runs in this phase (as in the reference `main`); [`stage`] is
//! kept so the GUI/`play` paths can call it when they are wired, and it is
//! silent unless [`begin`] ran.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The recorded start of the process (`hydra::started::START`).
static START: OnceLock<Mutex<Start>> = OnceLock::new();

/// What `begin` stores: the instant, and whether stage lines are wanted.
#[derive(Clone, Debug)]
struct Start {
    at: Instant,
}

/// Record the start instant.  Idempotent, like the recovered `OnceLock`.
pub fn begin() {
    let _ = START.set(Mutex::new(Start { at: Instant::now() }));
}

/// How long ago [`begin`] ran, or `None` when it never did.
pub fn elapsed() -> Option<Duration> {
    START
        .get()
        .map(|start| start.lock().unwrap_or_else(|e| e.into_inner()).at.elapsed())
}

/// True once [`begin`] has run.
pub fn started() -> bool {
    START.get().is_some()
}

/// Announce a startup stage: `[startup] {name} at {seconds:.3}s` on stderr.
///
/// `// UNKNOWN:` the recovered `write` also appends to a startup log file and
/// the exact wording lives in an undecoded piece table; only the shape (elapsed
/// seconds with a fractional part, stderr plus a file) is established.
pub fn stage(name: &str) {
    if let Some(elapsed) = elapsed() {
        eprintln!("[startup] {name} at {:.3}s", elapsed.as_secs_f64());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_records_an_instant_once() {
        // This test binary may already have started the clock through another
        // test, so assert the post-conditions rather than the transition.
        begin();
        assert!(started());
        let first = elapsed().expect("started");
        begin();
        assert!(started());
        assert!(elapsed().expect("started") >= first);
    }

    #[test]
    fn elapsed_is_a_plausible_duration() {
        begin();
        let elapsed = elapsed().expect("started");
        assert!(elapsed < Duration::from_secs(600), "{elapsed:?}");
    }
}
