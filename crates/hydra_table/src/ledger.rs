//! The process-wide ledger handle.
//!
//! Recovered from `hydra_table::ledger` (`0x101225424`, `0x10122549c`).
//!
//! Both functions read one `OnceLock`-style static at `0x104586be0`: the state
//! word at `0x104586c00` is tested with `ldapr` and passed to
//! `OnceLock::initialize` (`0x101f33c74`), and the stored value is an `Option`
//! whose `None` is `isize::MIN` in the first word — `open` compares
//! `[0x104586be0]` with `isize::MIN` (`0x101225444`) exactly the way
//! `Option<String>`/`Option<Vec<_>>` encode `None` on this toolchain.
//!
//! * [`shared`] hands out `Option<&'static …>` — `csel x0, xzr, x8, eq`
//!   (`0x1012254c0`) returns the static's address, or null when the value is
//!   `None`, and never initialises by itself.
//! * [`open`] initialises if needed and then bumps two atomic counters
//!   (`ldadd` on `[inner]` and `[inner + 0x200]` at `0x101225484`) before
//!   returning a reference-counted handle.  `store_claim::run` and
//!   `reward_claim::run` accept the result with `cbz x0` and keep the second
//!   register next to it (`0x10006c788`, `0x10006ee70`), then call
//!   `hydra_ledger::Ledger::reader(&self)` (`0x10168e934`) with the first.
//!
//! The ledger itself lives in `hydra_ledger`, which is **not** part of this
//! reconstruction, so [`Ledger`] is an opaque stand-in and the initialiser
//! answers `None`.  Callers treat that as "no ledger": the reward and store
//! claim runners simply skip their ledger query.  See `NOTES.md`.

use std::sync::{Arc, OnceLock};

/// The ledger handle shared by every claim runner.
///
/// **UNKNOWN**: `hydra_ledger::Ledger` is not reconstructed, so only the fact
/// that a single shared handle exists is modelled.  The binary shows the stored
/// `Option` with a `String`/`Vec`-shaped first field (that is where the
/// `isize::MIN` niche lives) and a reference-counted inner object at `+0x18`.
#[derive(Debug, Default)]
pub struct Ledger {
    /// Where the ledger's database lives, once opened.
    pub path: Option<std::path::PathBuf>,
}

impl Ledger {
    /// Open the ledger.
    ///
    /// **UNKNOWN**: the original's initialiser (`OnceLock::initialize` at
    /// `0x101f33c74`) calls into `hydra_ledger` with the data directory; none of
    /// that crate survives here, so the safe answer is "no ledger".
    fn connect() -> Option<Ledger> {
        None
    }
}

/// The shared ledger, if it is open.
///
/// **ESTABLISHED** (`0x10122549c`): a plain read of the static — no
/// initialisation — returning `None` while the value is not set or is `None`.
pub fn shared() -> Option<&'static Ledger> {
    init().as_ref()
}

/// The shared ledger, opening it if this is the first call.
///
/// **ESTABLISHED** (`0x101225424`): initialises the static, then returns a
/// reference-counted handle to the value (the two `ldadd` counter bumps before
/// the return) or `None` when the stored value is `None`.
pub fn open() -> Option<Arc<Ledger>> {
    init().as_ref().map(|_| Arc::new(Ledger::default()))
}

fn init() -> &'static Option<Ledger> {
    static SHARED: OnceLock<Option<Ledger>> = OnceLock::new();
    SHARED.get_or_init(Ledger::connect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_ledger_both_entry_points_answer_none() {
        // `hydra_ledger` is not part of this reconstruction, so `connect` fails
        // and every caller takes its "no ledger" path.
        assert!(shared().is_none());
        assert!(open().is_none());
        assert!(shared().is_none(), "opening does not invent a ledger");
    }
}
