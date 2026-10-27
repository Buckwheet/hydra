//! The one tokio runtime behind this crate's synchronous facade.
//!
//! The recovered relay is a `tokio` task tree driven by a `select!` loop
//! (`relay::run::{{closure}}` @ `0x10112ec94`).  The public API of this crate
//! is still synchronous — sibling crates and every existing test call
//! `show`/`run`/`read_lines` from ordinary threads — so the crate owns a
//! lazily started multi-thread runtime and drives the async machinery through
//! [`block_on`]:
//!
//! ```text
//! sync caller ──▶ runtime::block_on(fut) ──▶ tokio worker threads
//! sync caller ◀── std::sync::mpsc bridge ◀── tokio::sync::mpsc
//! ```
//!
//! The async entry points (`relay::read_lines_async`, `relay::show_async`,
//! `relay::run_async`) are the tokio-native ones; they are meant to be awaited
//! inside a runtime — either this one (`block_on`) or the caller's.
//!
//! **Reconnected in Phase 1**: `tokio` 1.53.1 (workspace pin).  The original
//! did not own a runtime — `hydra` called `#[tokio::main]`-style — so the
//! singleton here is our integration seam, not a recovered item.

use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinHandle;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The shared runtime, started on first use.
///
/// UNKNOWN: the original's runtime flavour (`hydra`'s `main` chooses it, and
/// the choice is not visible in `hydra_lich`'s address range).  A multi-thread
/// runtime is the safe equivalent: the blocking std readers behind
/// [`crate::relay::BlockingReader`] never share a worker with the relay loop.
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("hydra-lich")
            .build()
            .expect("could not start the hydra_lich tokio runtime")
    })
}

/// A cloneable handle to [`runtime`].
pub fn handle() -> Handle {
    runtime().handle().clone()
}

/// Spawn a task: on the caller's runtime when there is one, otherwise on
/// [`runtime`].  Mirroring the recovered code, the relay never joins these
/// tasks; they end when their channel, child or token does.
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    match Handle::try_current() {
        Ok(handle) => handle.spawn(future),
        Err(_) => self::handle().spawn(future),
    }
}

/// Run `future` to completion from synchronous code.
///
/// When the caller is already inside a runtime (`#[tokio::test]`, another
/// Hydra crate), blocking that worker thread on [`runtime`] would panic, so the
/// future is handed to a scoped thread instead: the future still runs on the
/// shared runtime, and the calling worker simply waits for it.
pub fn block_on<F>(future: F) -> F::Output
where
    F: Future + Send,
    F::Output: Send,
{
    if Handle::try_current().is_ok() {
        std::thread::scope(|scope| {
            let handle = scope.spawn(move || runtime().block_on(future));
            match handle.join() {
                Ok(output) => output,
                Err(panic) => std::panic::resume_unwind(panic),
            }
        })
    } else {
        runtime().block_on(future)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn the_runtime_is_a_singleton() {
        let a: *const Runtime = runtime();
        let b: *const Runtime = runtime();
        assert_eq!(a, b);
    }

    #[test]
    fn block_on_runs_a_tokio_future_from_sync_code() {
        let value = block_on(async { 6 * 7 });
        assert_eq!(value, 42);
    }

    #[test]
    fn block_on_is_safe_inside_a_runtime() {
        // A nested `block_on` on the same runtime would panic; the scoped
        // thread fallback must not.
        let inner = block_on(async {
            block_on(async { "nested" })
        });
        assert_eq!(inner, "nested");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn block_on_and_spawn_work_from_inside_a_foreign_runtime() {
        let value = block_on(async { 1 + 1 });
        assert_eq!(value, 2);

        let ran = Arc::new(AtomicUsize::new(0));
        let flag = Arc::clone(&ran);
        let handle = spawn(async move {
            flag.fetch_add(1, Ordering::SeqCst);
        });
        handle.await.unwrap();
        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn block_on_propagates_panics() {
        let result = std::panic::catch_unwind(|| block_on(async { panic!("boom") }));
        assert!(result.is_err());
    }
}
