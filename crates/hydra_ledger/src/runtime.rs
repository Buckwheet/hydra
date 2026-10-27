//! The one tokio runtime behind this crate.
//!
//! **Reconnected in Phase 1** (`tokio` 1.53.1). The original writer is
//! `tokio`-native: `Ledger::writer` (`0x10168e9e0`) builds a
//! `tokio::sync::mpsc::channel(1024)`, `Writer::{record, flush}` are `async fn`s
//! and the flush ack is a `tokio::sync::oneshot`. The public API of this crate
//! is synchronous — every sibling and every existing test calls
//! `try_record`/`record`/`flush` from an ordinary thread — so the crate owns a
//! lazily started runtime and drives the async pieces through [`block_on`]:
//!
//! ```text
//! sync caller ──▶ runtime::block_on(fut) ──▶ tokio workers
//! writer thread ──▶ runtime::runtime().block_on(writer::run_async(..))
//! ```
//!
//! The original did not own a runtime (`hydra`'s `main` did), so the singleton
//! here is our integration seam, not a recovered item — the same choice
//! `hydra_lich`'s `runtime` module makes.
//!
//! The runtime is **multi-threaded on purpose**: the writer's `run_async` never
//! finishes, and a caller's [`block_on`] (`record`, `flush`) must be able to
//! drive its own future while that task is suspended. A current-thread runtime
//! would serialise the two and deadlock.

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The shared runtime, started on first use.
///
/// UNKNOWN: the original's flavour (`hydra`'s `main` picked it), so this is the
/// safe equivalent rather than a recovered value.
pub(crate) fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("hydra-ledger-rt")
            .build()
            .expect("could not start the hydra_ledger tokio runtime")
    })
}

/// Run `future` to completion from synchronous code.
///
/// When the caller is already inside a runtime (`#[tokio::test]`, another
/// Hydra crate), blocking that worker on [`runtime`] would panic or deadlock,
/// so the future is handed to a scoped thread: it still runs on the shared
/// runtime and the calling worker simply waits for it.
pub(crate) fn block_on<F>(future: F) -> F::Output
where
    F: Future + Send,
    F::Output: Send,
{
    if Handle::try_current().is_ok() {
        std::thread::scope(|scope| match scope.spawn(|| runtime().block_on(future)).join() {
            Ok(output) => output,
            Err(panic) => std::panic::resume_unwind(panic),
        })
    } else {
        runtime().block_on(future)
    }
}
