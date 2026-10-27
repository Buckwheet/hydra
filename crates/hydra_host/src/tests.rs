//! Tests for the recovered `hydra_host` API.
//!
//! Every expected value is either read out of the binary (layout, message text,
//! ordering) or follows from the recovered control flow.  The two "still
//! running" tests use a channel to hold a thread open, so they are
//! deterministic; every wait is bounded and asserts its bound.

use super::*;
use std::mem::{offset_of, size_of};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A task body that blocks until the returned `Sender` is dropped or written to.
fn blocked() -> (Sender<()>, impl FnOnce() + Send + 'static) {
    let (tx, rx) = channel::<()>();
    let body = move || {
        let _ = rx.recv();
    };
    (tx, body)
}

/// Spin (bounded) until `f` is true, returning whether it became true.
fn until(mut f: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    f()
}

// ---------------------------------------------------------------------------
// Layouts (read from the binary)
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

/// `Hosted` is 0xa8 bytes with the `Who` at +0x00, the session's handles at
/// +0x48 and the task at +0xa0 (`is_running` @0x1013b0978 loads `[self+0xa0]`,
/// `Host::take` copies 0xa8 bytes out of the map).
#[test]
fn hosted_layout() {
    assert_eq!(size_of::<Hosted>(), 0xa8);
    assert_eq!(offset_of!(Hosted, who), 0x00);
    assert_eq!(offset_of!(Hosted, task), 0xa0);
    // `take` answers `Option<Hosted>` in 0xa8 bytes, `None` = `isize::MIN`.
    assert_eq!(size_of::<Option<Hosted>>(), 0xa8);
    // The task is one word, as a tokio `JoinHandle` is.
    assert_eq!(size_of::<Task>(), 0x08);
}

/// `Host` is 0x30 bytes: the runtime at +0x00, the map at +0x10, `next` at +0x28
/// (`Host::on` writes `[x8+0x28]`, `[x8+0x8]`, `[x8+0x20]`, `[x8]`).
#[test]
fn host_layout() {
    assert_eq!(size_of::<Host>(), 0x30);
    assert_eq!(offset_of!(Host, rt), 0x00);
    assert_eq!(offset_of!(Host, sessions), 0x10);
    assert_eq!(offset_of!(Host, next), 0x28);
    // The original's `Option<tokio::runtime::Handle>` is 16 bytes (tag + Arc); a
    // fat `Arc<dyn Fn …>` with a niche gives the same 16.
    assert_eq!(size_of::<Option<Handle>>(), 0x10);
}

// ---------------------------------------------------------------------------
// Host::on — a host with no sessions and the next id at 0
// ---------------------------------------------------------------------------

#[test]
fn on_starts_empty() {
    let mut host = Host::on(None);
    assert!(host.get(0).is_none());
    assert!(host.take(0).is_none());
    assert!(host.take_all().is_empty());
    assert_eq!(host.may_add("alice"), None);

    // `next` starts at 0: the first session is id 0, the second id 1.
    assert_eq!(host.add(Who::new("alice", "MAIN", ""), || {}), Ok(0));
    assert_eq!(host.add(Who::new("bob", "MAIN", ""), || {}), Ok(1));
}

// ---------------------------------------------------------------------------
// get / take
// ---------------------------------------------------------------------------

#[test]
fn get_and_take_by_id() {
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

#[test]
fn take_all_drains_in_id_order() {
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
    assert_eq!(host.add(Who::new("dave", "MAIN", ""), || {}), Ok(3));
}

// ---------------------------------------------------------------------------
// may_add / add
// ---------------------------------------------------------------------------

#[test]
fn may_add_rejects_a_running_duplicate_ignoring_ascii_case() {
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
        .add(Who::new("ALICE", "ALT", ""), || {})
        .expect_err("duplicate refused");
    assert_eq!(err.name, "ALICE");
    assert_eq!(err.account, "MAIN");
    assert_eq!(host.add(Who::new("bob", "MAIN", ""), || {}), Ok(1));

    tx.send(()).unwrap();
}

#[test]
fn may_add_ignores_a_finished_session() {
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("Alice", "MAIN", ""), || {}), Ok(0));
    assert!(
        until(|| !host.get(0).unwrap().is_running()),
        "the empty body must finish"
    );

    assert!(host.may_add("alice").is_none());
    assert_eq!(host.add(Who::new("alice", "MAIN", ""), || {}), Ok(1));
}

#[test]
fn may_add_answers_none_for_an_empty_table() {
    let host = Host::on(None);
    assert_eq!(host.may_add("anything"), None);
}

#[test]
fn add_uses_the_supplied_runtime_handle() {
    let spawned = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&spawned);
    let handle = Handle::new(move |body: Box<dyn FnOnce() + Send>| {
        counter.fetch_add(1, Ordering::SeqCst);
        Task::spawn(move || body())
    });
    let mut host = Host::on(Some(handle));
    assert_eq!(host.add(Who::new("carol", "MAIN", ""), || {}), Ok(0));
    assert_eq!(spawned.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// Hosted::is_running
// ---------------------------------------------------------------------------

#[test]
fn is_running_until_the_sleeping_task_ends() {
    let (tx, body) = blocked();
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("alice", "MAIN", ""), body), Ok(0));
    assert!(host.get(0).unwrap().is_running());
    assert!(host.get(0).unwrap().is_running(), "still blocked");

    tx.send(()).unwrap();
    assert!(until(|| !host.get(0).unwrap().is_running()));
}

#[test]
fn is_running_is_false_once_the_task_has_finished() {
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("bob", "MAIN", ""), || {}), Ok(0));
    assert!(until(|| !host.get(0).unwrap().is_running()));
    assert!(!host.get(0).unwrap().is_running());
}

// ---------------------------------------------------------------------------
// Hosted::stop
// ---------------------------------------------------------------------------

#[test]
fn stop_quits_the_session_and_waits_for_the_task() {
    let (tx, body) = blocked();
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("dave", "MAIN", ""), body), Ok(0));

    let quit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit);
    let hosted = host.take(0).expect("id 0 is hosted");
    let ended = hosted.stop(move || {
        flag.store(true, Ordering::SeqCst);
        tx.send(()).unwrap();
    });
    assert!(quit.load(Ordering::SeqCst), "quit must reach the session");
    assert!(ended, "the released task ends well inside the timeout");
}

#[test]
fn stop_skips_quit_when_the_task_is_already_gone() {
    let mut host = Host::on(None);
    assert_eq!(host.add(Who::new("erin", "MAIN", ""), || {}), Ok(0));
    assert!(until(|| !host.get(0).unwrap().is_running()));

    let hosted = host.take(0).expect("id 0 is hosted");
    // `tbz w0, #0x0` @0x10006c134: a finished task never runs the quit step.
    let ended = hosted.stop(|| panic!("quit must not run for a finished task"));
    assert!(ended);
}

/// The timeout constant is the immediate `tokio::time::timeout` is given
/// (`mov w0, #0xa`, `mov w1, #0x0` @0x10006c1ac).
#[test]
fn stop_timeout_is_ten_seconds() {
    assert_eq!(STOP_TIMEOUT, Duration::from_secs(10));
    assert_eq!(STOP_TIMEOUT.as_secs(), 10);
    assert_eq!(STOP_TIMEOUT.subsec_nanos(), 0);
}
