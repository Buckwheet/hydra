//! Tests for the recovered `hydra_host` API.
//!
//! Every expected value is either read out of the binary (layout, message text,
//! ordering) or follows from the recovered control flow.  Phase 1 runs them on a
//! real `tokio` runtime: the "still running" tests hold a session future open on
//! a `oneshot` channel (so no runtime thread is ever blocked), and every wait is
//! bounded by [`until`], which yields to the runtime instead of sleeping.

use super::*;
use std::mem::{offset_of, size_of};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::{Builder, RuntimeFlavor};
use tokio::sync::oneshot;

/// A session future that stays pending until the returned `Sender` fires (or is
/// dropped, which also releases the waiter).
fn blocked() -> (
    oneshot::Sender<()>,
    impl Future<Output = ()> + Send + 'static,
) {
    let (tx, rx) = oneshot::channel::<()>();
    let body = async move {
        let _ = rx.await;
    };
    (tx, body)
}

/// Yield to the runtime (bounded) until `f` is true, returning whether it became
/// true.  The helper never blocks the runtime thread it runs on, so the task it
/// is waiting for can still be polled; the loop gives up after 5 000 yields and
/// answers `false` instead of hanging.
async fn until(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..5_000 {
        if f() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    f()
}

// ---------------------------------------------------------------------------
// Layouts (read from the binary, then measured on the real types)
// ---------------------------------------------------------------------------

/// `Who` is three `String`s: `drop_in_place<Who>` @0x100012dc0 frees the buffers
/// at +0x00, +0x18 and +0x30, and `Host::add` reads `[who+0x40]`.
#[test]
fn who_is_three_strings() {
    assert_eq!(size_of::<String>(), 0x18);
    assert_eq!(size_of::<Who>(), 0x48);
    assert_eq!(offset_of!(Who, name), 0x00);
    assert_eq!(offset_of!(Who, account), 0x18);
    assert_eq!(offset_of!(Who, third), 0x30);
}

/// `AddError` is two `String`s (`drop_in_place<AddError>` @0x1000130d0).  The
/// `Option` wrapping it costs no tag: `Host::may_add` signals `None` by writing
/// `isize::MIN` into the first word (@0x1013b0714).
#[test]
fn add_error_costs_no_tag() {
    assert_eq!(size_of::<AddError>(), 0x30);
    assert_eq!(size_of::<Option<AddError>>(), 0x30);

    let none = None::<AddError>;
    // SAFETY: `Option<AddError>` is 0x30 bytes and all bit patterns are readable
    // memory; this only *reads* the niche that `may_add` writes.
    let bytes: [u64; 6] = unsafe { std::mem::transmute_copy(&none) };
    assert_eq!(bytes[0] as i64, isize::MIN as i64);

    // A `Some` payload never has that capacity (it is not a capacity at all).
    let some = Some(AddError {
        name: "x".to_owned(),
        account: String::new(),
    });
    let bytes: [u64; 6] = unsafe { std::mem::transmute_copy(&some) };
    assert_ne!(bytes[0] as i64, isize::MIN as i64);
}

/// `Host::add` answers `Result<SessionId, AddError>` in 0x30 bytes: the `Ok`
/// variant stores `isize::MIN` in the first word and the id at +0x8
/// (@0x1001c815c), which is exactly what `hydra_table` tests.
#[test]
fn result_of_add_is_thirty_bytes() {
    assert_eq!(size_of::<Result<SessionId, AddError>>(), 0x30);

    let ok: Result<SessionId, AddError> = Ok(7);
    // SAFETY: as above — reading the 0x30-byte painted value.
    let bytes: [u64; 6] = unsafe { std::mem::transmute_copy(&ok) };
    assert_eq!(bytes[0] as i64, isize::MIN as i64);
    assert_eq!(bytes[1] as u32, 7);
}

/// `Hosted` is 0xa8 bytes with the `Who` at +0x00, the ten session handles at
/// +0x48, the `CancellationToken` at +0x98 and the task at +0xa0
/// (`is_running` @0x1013b0978 loads `[self+0xa0]`, `Host::take` copies 0xa8 bytes
/// out of the map).
///
/// Phase 1 keeps every recovered offset because the three real fields that
/// replace the stand-ins are all one word: `JoinHandle<()>` (`NonNull<RawTask>`)
/// and `CancellationToken` (`Arc<TreeNode>`).
#[test]
fn hosted_layout() {
    assert_eq!(size_of::<Hosted>(), 0xa8);
    assert_eq!(offset_of!(Hosted, who), 0x00);
    assert_eq!(offset_of!(Hosted, handles), 0x48);
    assert_eq!(offset_of!(Hosted, token), 0x98);
    assert_eq!(offset_of!(Hosted, task), 0xa0);
    // The recovered slot widths: 0x50 of session handles, 8 + 8 for the token and
    // the task.
    assert_eq!(size_of::<Handles>(), 0x50);
    assert_eq!(size_of::<CancellationToken>(), 0x08);
    // `take` answers `Option<Hosted>` in 0xa8 bytes, `None` = `isize::MIN`.
    assert_eq!(size_of::<Option<Hosted>>(), 0xa8);
    let none = None::<Hosted>;
    // SAFETY: `Option<Hosted>` is 0xa8 bytes of readable memory; this only reads
    // the niche `Host::take` writes at 0x1013b0674.
    let bytes: [u64; 0x15] = unsafe { std::mem::transmute_copy(&none) };
    assert_eq!(bytes[0] as i64, isize::MIN as i64);
    // The task is one word, as a tokio `JoinHandle` is.
    assert_eq!(size_of::<Task>(), 0x08);
    assert_eq!(size_of::<Option<Task>>(), 0x08);
}

/// `Host` is 0x30 bytes: the runtime at +0x00, the map at +0x10, `next` at +0x28
/// (`Host::on` writes `[x8+0x28]`, `[x8+0x8]`, `[x8+0x20]`, `[x8]`).
///
/// The recovered 16-byte `Option<Handle>` is measured, not assumed: tokio
/// 1.53.1's `Handle` wraps a two-variant `scheduler::Handle` enum, and the
/// `Option` around it gets no niche (the enum's tag word is a full word), so the
/// real type matches the original's build exactly.  See `NOTES.md` §4a.
#[test]
fn host_layout() {
    assert_eq!(size_of::<Option<Handle>>(), 0x10);
    assert_eq!(size_of::<Handle>(), 0x10);
    assert_eq!(size_of::<BTreeMap<SessionId, Hosted>>(), 0x18);
    assert_eq!(offset_of!(Host, rt), 0x00);
    assert_eq!(offset_of!(Host, sessions), 0x10);
    assert_eq!(offset_of!(Host, next), 0x28);
    assert_eq!(size_of::<Host>(), 0x30);
}

// ---------------------------------------------------------------------------
// Host::on — a host with no sessions and the next id at 0
// ---------------------------------------------------------------------------

#[tokio::test]
async fn on_starts_empty() {
    let mut host = Host::on(None);
    assert!(host.get(0).is_none());
    assert!(host.take(0).is_none());
    assert!(host.take_all().is_empty());
    assert_eq!(host.may_add("alice"), None);

    // `next` starts at 0: the first session is id 0, the second id 1.
    assert_eq!(host.add(Who::new("alice", "MAIN", ""), async {}), Ok(0));
    assert_eq!(host.add(Who::new("bob", "MAIN", ""), async {}), Ok(1));
}

// ---------------------------------------------------------------------------
// get / take
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_and_take_by_id() {
    let (_a_tx, a) = blocked();
    let (_b_tx, b) = blocked();
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("alice", "MAIN", ""), a), Ok(0));
    assert_eq!(host.add(Who::new("bob", "ALT", ""), b), Ok(1));

    assert_eq!(host.get(0).unwrap().who.name, "alice");
    assert_eq!(host.get(0).unwrap().who.account, "MAIN");
    assert_eq!(host.get(1).unwrap().who.name, "bob");
    assert_eq!(host.get(1).unwrap().who.account, "ALT");
    assert!(host.get(2).is_none());

    let taken = host.take(0).expect("id 0 is hosted");
    assert_eq!(taken.who.name, "alice");
    assert!(host.get(0).is_none());
    assert_eq!(host.get(1).unwrap().who.name, "bob");
    // Taking twice is not a hit.
    assert!(host.take(0).is_none());
}

#[tokio::test]
async fn take_all_drains_in_id_order() {
    let (_a_tx, a) = blocked();
    let (_b_tx, b) = blocked();
    let (_c_tx, c) = blocked();
    let mut host = Host::on(None);
    for (who, body) in [
        (Who::new("alice", "MAIN", ""), a),
        (Who::new("bob", "MAIN", ""), b),
        (Who::new("carol", "ALT", ""), c),
    ] {
        assert!(host.add(who, body).is_ok());
    }

    let all = host.take_all();
    let names: Vec<&str> = all.iter().map(|h| h.who.name.as_str()).collect();
    assert_eq!(names, ["alice", "bob", "carol"]);
    assert_eq!(all[2].who.account, "ALT");
    // The map is empty again, and ids restart where they left off.
    assert!(host.get(0).is_none() && host.get(1).is_none() && host.get(2).is_none());
    assert!(host.take_all().is_empty());
    assert_eq!(host.add(Who::new("dave", "MAIN", ""), async {}), Ok(3));
}

// ---------------------------------------------------------------------------
// may_add / add
// ---------------------------------------------------------------------------

#[tokio::test]
async fn may_add_rejects_a_running_duplicate_ignoring_ascii_case() {
    let (tx, body) = blocked();
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("Alice", "MAIN", ""), body), Ok(0));
    assert!(host.get(0).unwrap().is_running());

    let err = host
        .may_add("alice")
        .expect("a running 'Alice' refuses 'alice'");
    assert_eq!(err.name, "alice");
    assert_eq!(err.account, "MAIN");
    assert_eq!(
        err.to_string(),
        "alice is already logged in on account MAIN; the game allows one character per account at a time"
    );

    // Same bytes, different ASCII case, both directions.
    assert_eq!(host.may_add("ALICE").unwrap().name, "ALICE");
    assert!(host.may_add("bob").is_none());
    assert!(host.may_add("").is_none());

    // `add` runs the same guard and spawns nothing on a refusal.
    let err = host
        .add(Who::new("ALICE", "ALT", ""), async {})
        .expect_err("duplicate refused");
    assert_eq!(err.name, "ALICE");
    assert_eq!(err.account, "MAIN");
    assert_eq!(host.add(Who::new("bob", "MAIN", ""), async {}), Ok(1));

    tx.send(()).unwrap();
}

#[tokio::test]
async fn may_add_ignores_a_finished_session() {
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("Alice", "MAIN", ""), async {}), Ok(0));
    assert!(
        until(|| !host.get(0).unwrap().is_running()).await,
        "the empty body must finish"
    );

    assert!(host.may_add("alice").is_none());
    assert_eq!(host.add(Who::new("alice", "MAIN", ""), async {}), Ok(1));
}

#[test]
fn may_add_answers_none_for_an_empty_table() {
    let host = Host::on(None);
    assert_eq!(host.may_add("anything"), None);
}

/// `Host::add`'s `Some(handle)` arm (`Handle::spawn` @0x1001c7f78) spawns on the
/// handle it was given, not on the ambient runtime.
///
/// The test runtime is `current_thread` and the supplied one is `multi_thread`,
/// so the session body can tell the two apart through
/// `Handle::current().runtime_flavor()`.
#[tokio::test]
async fn add_uses_the_supplied_runtime_handle() {
    let rt = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a multi-thread runtime");
    let (tx, rx) = oneshot::channel::<RuntimeFlavor>();
    let mut host = Host::on(Some(rt.handle().clone()));

    let spawned = host.add(Who::new("carol", "MAIN", ""), async move {
        let _ = tx.send(Handle::current().runtime_flavor());
    });
    assert_eq!(spawned, Ok(0));
    assert_eq!(
        Handle::current().runtime_flavor(),
        RuntimeFlavor::CurrentThread,
        "the test's own runtime is the current_thread one"
    );
    assert_eq!(
        rx.await.expect("the body runs"),
        RuntimeFlavor::MultiThread,
        "the session task ran on the supplied handle"
    );

    // Dropping a `Runtime` inside another runtime's context panics, so hand the
    // workers back without blocking.
    rt.shutdown_background();
}

/// `Host::add`'s `None` arm (`tokio::task::spawn::spawn` @0x1001c7ea8) uses the
/// ambient runtime.
#[tokio::test]
async fn add_without_a_handle_spawns_on_the_ambient_runtime() {
    let (tx, rx) = oneshot::channel::<RuntimeFlavor>();
    let mut host = Host::on(None);
    assert_eq!(
        host.add(Who::new("dave", "MAIN", ""), async move {
            let _ = tx.send(Handle::current().runtime_flavor());
        }),
        Ok(0)
    );
    assert_eq!(
        rx.await.expect("the body runs"),
        RuntimeFlavor::CurrentThread,
        "the ambient (this test's) runtime spawned it"
    );
    assert!(until(|| !host.get(0).unwrap().is_running()).await);
}

/// With no runtime at all the ambient arm panics, as the real `tokio::spawn`
/// does — the port does not hide that behind a std thread any more.
#[test]
fn add_without_a_runtime_panics() {
    let mut host = Host::on(None);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.add(Who::new("erin", "MAIN", ""), async {})
    }));
    let panic = outcome.expect_err("tokio::spawn needs an ambient runtime");
    let text = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        text.contains("no reactor running"),
        "tokio's own message, not a port-specific one: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// Hosted::is_running
// ---------------------------------------------------------------------------

#[tokio::test]
async fn is_running_until_the_sleeping_task_ends() {
    let (tx, body) = blocked();
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("alice", "MAIN", ""), body), Ok(0));
    assert!(host.get(0).unwrap().is_running());
    assert!(host.get(0).unwrap().is_running(), "still blocked");

    tx.send(()).unwrap();
    assert!(until(|| !host.get(0).unwrap().is_running()).await);
}

#[tokio::test]
async fn is_running_is_false_once_the_task_has_finished() {
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("bob", "MAIN", ""), async {}), Ok(0));
    assert!(until(|| !host.get(0).unwrap().is_running()).await);
    assert!(!host.get(0).unwrap().is_running());
}

// ---------------------------------------------------------------------------
// Hosted::stop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stop_quits_the_session_and_waits_for_the_task() {
    let (tx, body) = blocked();
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("dave", "MAIN", ""), body), Ok(0));

    let quit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit);
    let hosted = host.take(0).expect("id 0 is hosted");
    let ended = hosted
        .stop(move || {
            flag.store(true, Ordering::SeqCst);
            tx.send(()).unwrap();
        })
        .await;
    assert!(quit.load(Ordering::SeqCst), "quit must reach the session");
    assert!(ended, "the released task ends well inside the timeout");
}

#[tokio::test]
async fn stop_skips_quit_when_the_task_is_already_gone() {
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("erin", "MAIN", ""), async {}), Ok(0));
    assert!(until(|| !host.get(0).unwrap().is_running()).await);

    let hosted = host.take(0).expect("id 0 is hosted");
    // `tbz w0, #0x0` @0x10006c134: a finished task never runs the quit step.
    let ended = hosted
        .stop(|| panic!("quit must not run for a finished task"))
        .await;
    assert!(ended);
}

/// The timeout path: `tokio::time::timeout` @0x10006c1b4 elapses and
/// `AbortHandle::abort` @0x10006c2ec drops the task's future.
///
/// The runtime is paused, so the recovered ten seconds are tokio's virtual ones
/// and the test does not wait for them; the timeout itself is the real one.
#[tokio::test(start_paused = true)]
async fn stop_aborts_the_task_after_the_timeout() {
    /// Records that the future it lives in was dropped — which is what an abort
    /// does to a task that never returns.
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Arc::new(AtomicBool::new(false));
    let guard = Dropped(Arc::clone(&dropped));
    let mut host = Host::on(None);
    assert_eq!(
        host.add(Who::new("frank", "MAIN", ""), async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        }),
        Ok(0)
    );

    let quit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit);
    let hosted = host.take(0).expect("id 0 is hosted");
    let ended = hosted
        .stop(move || flag.store(true, Ordering::SeqCst))
        .await;
    assert!(
        quit.load(Ordering::SeqCst),
        "the task was running, so quit ran"
    );
    assert!(!ended, "a task that never ends outlives the timeout");
    assert!(
        until(|| dropped.load(Ordering::SeqCst)).await,
        "the aborted task's future is dropped"
    );
}

/// The token at `Hosted+0x98` (`CancellationToken::cancel` @0x10006c180) is
/// tripped, and it is tripped *after* `quit` (@0x10006c158), so the session still
/// sees an untripped token when it is asked to stop.
#[tokio::test]
async fn stop_trips_the_cancellation_token() {
    let (tx, body) = blocked();
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("gina", "MAIN", ""), body), Ok(0));

    let token = host.get(0).expect("id 0 is hosted").stop_token();
    assert!(!token.is_cancelled());

    let seen = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&seen);
    let watched = token.clone();
    let hosted = host.take(0).expect("id 0 is hosted");
    let ended = hosted
        .stop(move || {
            flag.store(watched.is_cancelled(), Ordering::SeqCst);
            tx.send(()).unwrap();
        })
        .await;
    assert!(ended, "the released task ends inside the timeout");
    assert!(
        !seen.load(Ordering::SeqCst),
        "quit runs before the token is cancelled"
    );
    assert!(token.is_cancelled(), "stop trips the token after quit");

    // A waiter on the token is woken by the cancel.
    let waiter = token.clone();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), waiter.cancelled())
            .await
            .is_ok(),
        "the token's waiters resolve once it is cancelled"
    );
}

/// The timeout constant is the immediate `tokio::time::timeout` is given
/// (`mov w0, #0xa`, `mov w1, #0x0` @0x10006c1ac).
#[test]
fn stop_timeout_is_ten_seconds() {
    assert_eq!(STOP_TIMEOUT, Duration::from_secs(10));
    assert_eq!(STOP_TIMEOUT.as_secs(), 10);
    assert_eq!(STOP_TIMEOUT.subsec_nanos(), 0);
}
