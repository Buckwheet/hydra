//! `hydra_host` — the table of hosted sessions.
//!
//! Reconstructed from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
//! (no DWARF; every claim below was read out of the instruction stream).  The
//! whole crate is one file — `crates/hydra-host/src/lib.rs` is the single source
//! path that survives in `__cstring` @0x103e215d1.
//!
//! Layouts recovered from the binary (arm64; `Vec`/`String` are
//! `cap@0, ptr@8, len@0x10` in this build):
//!
//! ```text
//! Host      (0x30)   +0x00 Option<tokio::runtime::Handle> (16: tag@0, Arc@8)
//!                    +0x10 BTreeMap<SessionId, Hosted>   (24: root@0, height@8, len@16)
//!                    +0x28 SessionId next                 (u32)
//! Who       (0x48)   3 × String
//! AddError  (0x30)   2 × String
//! Hosted    (0xa8)   Who@0x00, handles@0x48 (0x50), token@0x98, task@0xa0
//! ```
//!
//! Phase 1 measured the real types against those numbers and they agree: with
//! tokio 1.53.1 `Option<Handle>` is still 16 bytes and `JoinHandle<()>` /
//! `CancellationToken` are one word each, so `Host` is 0x30 and `Hosted` is 0xa8.
//!
//! The original linked `tokio` 1.53.1, `tokio_util`, `hydra_session`, `hydra_board`
//! and `hydra_table`.  **Phase 1** reconnects the runtime to the real crates: the
//! handle at `Host+0x00` is a `tokio::runtime::Handle`, the task at `Hosted+0xa0`
//! is a `tokio::task::JoinHandle`, the token at `Hosted+0x98` is a real
//! `tokio_util::sync::CancellationToken`, and `Hosted::stop` is the recovered
//! `async fn` over `tokio::time::timeout` + `AbortHandle::abort`.  What is still a
//! seam — the session body (`SupervisedSession`) and the ten session handles — is
//! marked `Departure:` / `UNKNOWN:` below and in `NOTES.md`.

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// The runtime handle at `Host+0x00`, re-exported so callers can name the type
/// they pass to [`Host::on`].
///
/// Recovered: `drop_in_place<hydra_host::Host>` @0x10126f764 names it
/// `core::option::Option<tokio::runtime::handle::Handle>`.  This is the real
/// `tokio` type (Phase 1), not a stand-in.
pub use tokio::runtime::Handle;

/// How long [`Hosted::stop`] waits for a session task to end.
///
/// Recovered: `Hosted::stop::{{closure}}` @0x10006c1ac sets `w0 = 10`, `w1 = 0`
/// and calls `tokio::time::timeout` — a `Duration` of 10 s + 0 ns.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// `hydra_host::SessionId` — the key of [`Host`]'s table.
///
/// Recovered: the `BTreeMap` key is four bytes wide.  `Host::get` @0x1013b0588
/// compares it with `cmp w1, w14` and loads keys with `ldr w14, [x11], #4`;
/// `Host::add` stores a fresh id with `str w25, [x19, #0x8]` and increments the
/// counter with `add w8, w25, #1`.
///
/// UNKNOWN: whether the original declared `type SessionId = u32` or a
/// `#[repr(transparent)] struct SessionId(u32)`.  Every recovered use is plain
/// `u32` arithmetic (`ldr w25, [x23, #0x28]` then `add w8, w25, #1`), so the
/// alias is the safest reading; nothing in the binary wraps or unwraps it.
pub type SessionId = u32;

/// One hosted session's task — the value at `Hosted+0xa0`.
///
/// Recovered: [`Hosted::is_running`] @0x1013b0978 loads `[self+0xa0]` and calls
/// `tokio::runtime::task::state::State::load`, and `drop_in_place<Hosted>`
/// @0x1013a0760 ends with `State::drop_join_handle_fast` /
/// `RawTask::drop_join_handle_slow`.  So `Hosted+0xa0` is a
/// `tokio::runtime::task::JoinHandle<T>`, one word wide.
///
/// The original's `T` is `hydra_session::supervisor::SupervisedEnd`.  That type
/// does not exist in this reconstruction (`hydra_session::supervisor` exposes
/// `jitter`/`backoff`/`answer_between` and no session type), and this crate must
/// not invent it, so the seam is `JoinHandle<()>`: [`Host::add`] spawns a future
/// whose output is `()`.  The handle is never awaited for its value here — only
/// `is_finished`, `abort_handle` and `timeout` are used, exactly like the
/// recovered code.
pub type Task = JoinHandle<()>;

// ---------------------------------------------------------------------------
// Who
// ---------------------------------------------------------------------------

/// The identity a session is hosted under: the three names `Host::add` copies
/// into the [`Hosted`] it inserts.
///
/// Recovered: `hydra_host::Who` is `0x48` bytes of three `String`s —
/// `drop_in_place<Who>` @0x100012dc0 frees the buffers at +0x00, +0x18 and
/// +0x30, and `Host::add` copies `[who+0x40]` (the third `String`'s length) into
/// `Hosted+0x40`, i.e. the whole `Who` is memcpy'd into the front of the
/// `Hosted`.  There is no `Who` constructor symbol, so the struct is built with
/// literal syntax elsewhere in the workspace and its fields are public.
// `repr(C)`: the recovered field offsets (+0x00, +0x18, +0x30) are kept.
#[repr(C)]
pub struct Who {
    /// The character name.
    ///
    /// `Host::may_add` compares this field, case-insensitively, with the name it
    /// is asked about (`Hosted+0x08`/`+0x10` are read in @0x1013b07fc).
    pub name: String,
    /// The account the character is on.
    ///
    /// The live entry's account is cloned into [`AddError`] (`add x0, x27,
    /// #0x18; bl String::clone` in `Host::may_add` @0x1013b08a0) and printed as
    /// `"… on account …"`.
    pub account: String,
    /// UNKNOWN: the third name.  No recovered function reads it: `may_add` reads
    /// `name` and `account`, `is_running` reads the task, and `Host::add` only
    /// copies the field across.  It exists because the type is three `String`s
    /// wide; the field is carried through unchanged and named `third` rather
    /// than guessed at.
    pub third: String,
}

impl Who {
    /// Build a `Who` from its three names.
    pub fn new(
        name: impl Into<String>,
        account: impl Into<String>,
        third: impl Into<String>,
    ) -> Who {
        Who {
            name: name.into(),
            account: account.into(),
            third: third.into(),
        }
    }
}

impl fmt::Debug for Who {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Who")
            .field("name", &self.name)
            .field("account", &self.account)
            .field("third", &self.third)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// AddError
// ---------------------------------------------------------------------------

/// Why a session could not be added: one is already logged in.
///
/// Recovered: `hydra_host::AddError` is two `String`s —
/// `drop_in_place<AddError>` @0x1000130d0 frees exactly the buffers at +0x00 and
/// +0x18, and `<AddError as Display>::fmt` @0x1013b0bc8 formats one literal with
/// both.  The literal, `__cstring` @0x103e696a7, is
///
/// ```text
/// {} is already logged in on account {}; the game allows one character per account at a time
/// ```
///
/// The two `String`s are built by [`Host::may_add`] from the requested name and
/// from the running entry's `Who::account`.
// `repr(C)`: `name` at +0x00, `account` at +0x18, 0x30 bytes in total.
#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct AddError {
    /// The name that was asked for, exactly as it was passed in.
    pub name: String,
    /// The account the clashing session is logged in on.
    pub account: String,
}

impl fmt::Display for AddError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is already logged in on account {}; the game allows one character per account at a time",
            self.name, self.account
        )
    }
}

impl std::error::Error for AddError {}

// ---------------------------------------------------------------------------
// Hosted
// ---------------------------------------------------------------------------

/// The session handles the host keeps, `Hosted+0x48..+0x98`.
///
/// Recovered from `drop_in_place<Hosted>` @0x1013a0594, which drops them in
/// order:
///
/// ```text
/// +0x48  tokio::sync::mpsc::Sender<_>          (Tx::close + AtomicWaker::wake)
/// +0x50  tokio::sync::broadcast::Sender<hydra_session::event::Event>
/// +0x58  Arc<_>   +0x60 Arc<_>   +0x68 Arc<_>   +0x70 Arc<_>
/// +0x78  Arc<_>   +0x80 Arc<_>   +0x88 Arc<_>   +0x90 Arc<_>
/// ```
///
/// (eight independent `Arc::drop_slow` instantiations, so eight distinct
/// payloads — the `hydra_board::sessions::Sessions` pair plus the session's own
/// state.)
///
/// Departure (**still a seam after Phase 1**): those ten payload types live in
/// `hydra_session`/`hydra_board`, and neither crate exposes them — `hydra_session`
/// has no `SupervisedSession`/`SupervisedEnd`, so the handles cannot be named,
/// cloned or read here.  No function in `hydra_host` reads any of them, so the
/// port keeps one opaque, correctly-sized slot of ten words (0x50 bytes): the
/// `Hosted` offsets stay as recovered.  The recovered *token* at `+0x98` is no
/// longer inside this array — it is the real `CancellationToken` field below.
#[derive(Default)]
// Dead weight by design: the slot only holds the recovered offsets, and keeping it
// is what makes `Hosted` 0xa8 bytes with the task at +0xa0.
#[allow(dead_code)]
struct Handles([usize; 10]);

/// One session the host is keeping: the map value, `0xa8` bytes.
///
/// Recovered: `Hosted+0x00` is the [`Who`] (0x48 bytes), `+0x48..+0x98` are the
/// session's handles, `+0x98` is the `CancellationToken`, and `+0xa0` is the task
/// that [`Hosted::is_running`] and [`Hosted::stop`] follow.
// `repr(C)`: keeps `who` at +0x00, the handles at +0x48, the token at +0x98 and
// the task at +0xa0.
#[repr(C)]
pub struct Hosted {
    /// The identity the session was added under.
    pub who: Who,
    /// The session's channels (see `Handles`).
    handles: Handles,
    /// The session's cancellation token; `+0x98` in the original.
    ///
    /// Recovered: `drop_in_place<Hosted>` calls `CancellationToken::drop` at
    /// `+0x98`, `Host::add` takes a `CancellationToken::clone` out of the session
    /// (`ldadd x8, x8, [x21, #0x15d0]`) for it, and `Hosted::stop` cancels it
    /// @0x10006c180.  Phase 1: the real `tokio_util::sync::CancellationToken`.
    ///
    /// Departure: the original *cloned the session's own* token, so the session
    /// kept a second clone and observed the cancel.  Here the host creates the
    /// token (it has no session type to ask) and lends it out through
    /// [`Hosted::stop_token`]; the seam is documented in `NOTES.md` §4a.
    token: CancellationToken,
    /// The spawned task; `+0xa0` in the original.
    task: Task,
}

impl Hosted {
    /// Is the session's task still running?
    ///
    /// Recovered verbatim from @0x1013b0970:
    ///
    /// ```text
    /// ldr  x0, [x0, #0xa0]      ; the JoinHandle
    /// bl   tokio…State::load    ; the task state word
    /// tst  x0, #0x2             ; the COMPLETE bit
    /// cset w0, eq               ; running == !complete
    /// ```
    ///
    /// i.e. `!JoinHandle::is_finished()`: a task counts as running from spawn
    /// until it returns *or* is cancelled.
    pub fn is_running(&self) -> bool {
        !self.task.is_finished()
    }

    /// A clone of the session's cancellation token, the recovered `Hosted+0x98`.
    ///
    /// **Phase 1 addition** (not a recovered function): the original handed the
    /// same token to the session, the relay and the player log, so those holders
    /// could wait for shutdown; here [`Hosted::stop`] is the only writer, so
    /// readers need this accessor.  Waiters use `token.cancelled()`.
    pub fn stop_token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Ask the session to stop and wait up to [`STOP_TIMEOUT`] for its task.
    /// Answers whether the task ended in time.
    ///
    /// Recovered from `hydra_host::Hosted::stop::{{closure}}` @0x10006c084:
    ///
    /// 1. `Hosted::is_running` @0x10006c130; when it is false the `quit` step is
    ///    skipped entirely (`tbz w0, #0x0` → straight to the cancel of step 2).
    /// 2. `hydra_session::command::SessionHandle::quit` @0x10006c158, then
    ///    `CancellationToken::cancel` on `Hosted+0x98` @0x10006c180.
    /// 3. `tokio::time::timeout(Duration::from_secs(10), self.task)` @0x10006c1b4
    ///    around `JoinHandle::poll`; when it elapses the task is aborted through
    ///    `AbortHandle::abort` @0x10006c2ec.
    ///
    /// Phase 1: steps 2 and 3 are the real ones — the token is cancelled and the
    /// timeout/abort pair is `tokio::time::timeout` over `&mut JoinHandle` with
    /// `JoinHandle::abort_handle`.  Departure (**still a seam**):
    /// `hydra_session::command::SessionHandle` has no recovered `quit` (it offers
    /// `stop(reason)`, which trips its own token — a different operation), so the
    /// caller passes the quit step.  See `NOTES.md` §4a.
    pub async fn stop<Q>(self, quit: Q) -> bool
    where
        Q: FnOnce(),
    {
        if self.is_running() {
            quit();
        }
        self.token.cancel();
        // The abort handle has to be taken before the join handle is borrowed by
        // `timeout`, exactly as the original kept it across the poll.
        let abort = self.task.abort_handle();
        let mut task = self.task;
        match tokio::time::timeout(STOP_TIMEOUT, &mut task).await {
            Ok(_) => true,
            Err(_) => {
                abort.abort();
                false
            }
        }
    }
}

impl fmt::Debug for Hosted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hosted")
            .field("who", &self.who)
            .field("is_running", &self.is_running())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Host
// ---------------------------------------------------------------------------

/// The table of hosted sessions: `0x30` bytes.
///
/// Recovered offsets: `+0x00` `Option<tokio::runtime::Handle>` (16 bytes; the
/// real type is also 16 in tokio 1.53.1 — measured in
/// `tests::host_layout`), `+0x10` the `BTreeMap<SessionId, Hosted>`
/// (`root@+0x10`, `height@+0x18`, `len@+0x20` — `Host::on` zeroes only root and
/// length, because a null root makes the height dead), `+0x28` the next
/// [`SessionId`] (u32).
// `repr(C)`: keeps `rt` at +0x00, the map at +0x10 and `next` at +0x28 (0x30
// bytes in total, as recovered).
#[repr(C)]
pub struct Host {
    /// Where new session tasks are spawned; `+0x00`.
    rt: Option<Handle>,
    /// The hosted sessions; `+0x10`.
    sessions: BTreeMap<SessionId, Hosted>,
    /// The id the next [`Host::add`] hands out; `+0x28`.
    next: SessionId,
}

impl Host {
    /// A host with no sessions, spawning its tasks on `rt` (or on the ambient
    /// runtime when `rt` is `None`).
    ///
    /// Recovered verbatim from @0x1013b054c (20 bytes, the whole function):
    ///
    /// ```text
    /// str  wzr, [x8, #0x28]      ; next = 0
    /// stp  x1,  xzr, [x8, #0x8]  ; rt.1 = x1, map.root = 0
    /// str  xzr, [x8, #0x20]      ; map.len = 0
    /// str  x0,  [x8]             ; rt.0 = x0
    /// ```
    ///
    /// i.e. `Host { rt, sessions: BTreeMap::new(), next: 0 }` — with the real
    /// tokio types the same expression still builds a `0x30`-byte struct with
    /// exactly those offsets.
    pub fn on(rt: Option<Handle>) -> Host {
        Host {
            rt,
            sessions: BTreeMap::new(),
            next: 0,
        }
    }

    /// The session hosted under `id`.
    ///
    /// Recovered from @0x1013b0560: walk the `BTreeMap` from `+0x10`/`+0x18`,
    /// comparing the u32 key, and return a bare pointer (null = absent) to the
    /// value — i.e. `self.sessions.get(&id)`.
    pub fn get(&self, id: SessionId) -> Option<&Hosted> {
        self.sessions.get(&id)
    }

    /// Remove and return the session hosted under `id`.
    ///
    /// Recovered from @0x1013b05d4: the same search, then
    /// `OccupiedEntry::remove_kv` (@0x1013af794) into a temporary from which
    /// only the value is copied out (0xa8 bytes; the u32 key is discarded) — i.e.
    /// `self.sessions.remove(&id)`.
    pub fn take(&mut self, id: SessionId) -> Option<Hosted> {
        self.sessions.remove(&id)
    }

    /// Remove every session and return the values in `SessionId` order.
    ///
    /// Recovered from @0x1013b0920: the map's three words are moved out
    /// (`ldp x9, x10, [x0, #0x10]`, `ldr x11, [x0, #0x20]`), the root and the
    /// length are zeroed, the moved map is turned into its `IntoIter`
    /// (length at `+0x40`) and collected by
    /// `Vec<hydra_host::Hosted>::from_iter` (@0x1013a9410, element stride 0xa8) —
    /// i.e. `mem::take(&mut self.sessions).into_values().collect()`.
    pub fn take_all(&mut self) -> Vec<Hosted> {
        std::mem::take(&mut self.sessions).into_values().collect()
    }

    /// Would adding a session called `name` be refused?
    ///
    /// Recovered from @0x1013b06d4:
    ///
    /// * no map root or no entries → `None` (`cmp x8, #0x0` / `ccmp x22, #0x0`
    ///   @0x1013b0708).
    /// * for each entry, in key order: skip it unless its task is *running*
    ///   (`State::load` @0x1013b07d8, `tbz w0, #0x1` — bit 1 is `COMPLETE`, so
    ///   bit 1 clear ⇒ running).
    /// * compare `Hosted+0x10` (the name's length) with `name.len()`, then the
    ///   bytes of `Hosted+0x08` with `name`'s, ASCII-lowercasing both
    ///   (`sub w11, w10, #0x41; cmp w11, #0x1a; csel w11, w24(=0x20), wzr, lo`)
    ///   — i.e. `eq_ignore_ascii_case`.
    /// * on a match: `name.to_string()` (exact-capacity allocation,
    ///   `bl __rust_alloc` @0x1013b087c + `memcpy` @0x1013b0890) and a clone of the live
    ///   entry's `Hosted+0x18` (its account) become the [`AddError`].
    ///
    /// The result is `Option<AddError>`: the failure paths store `isize::MIN` in
    /// the first word of the 0x30-byte result (@0x1013b0714), which is the
    /// `String`-capacity niche of the two-`String` payload — the same encoding
    /// `Host::add` tests at @0x1001c7d84.
    pub fn may_add(&self, name: &str) -> Option<AddError> {
        for hosted in self.sessions.values() {
            if !hosted.is_running() {
                continue;
            }
            if hosted.who.name.eq_ignore_ascii_case(name) {
                return Some(AddError {
                    name: name.to_owned(),
                    account: hosted.who.account.clone(),
                });
            }
        }
        None
    }

    /// Host a session, unless one with the same name is still running.
    ///
    /// Recovered from `hydra_host::Host::add` @0x1001c7d28:
    ///
    /// 1. `may_add(&who.name)` (`ldp x22, x2, [x1, #0x8]` reads `who`'s first
    ///    `String`); `Some` is answered as the error without spawning anything
    ///    (@0x1001c7eb0).
    /// 2. `let id = self.next; self.next += 1;` (@0x1001c7d90).
    /// 3. spawn the session future: on `Some(handle)` `Handle::spawn`
    ///    @0x1001c7f78, on `None` the ambient `tokio::task::spawn::spawn`
    ///    @0x1001c7ea8.
    /// 4. insert `Hosted { who, …session handles…, token, task }` under `id`
    ///    (`VacantEntry::insert_entry` @0x1001c8158) and answer `Ok(id)`
    ///    (`str w25, [x19, #0x8]`, `str isize::MIN, [x19]` @0x1001c815c).
    ///
    /// The original signature is
    /// `fn add(&mut self, who: Who, session: SupervisedSession<LiveConnector>) -> Result<SessionId, AddError>`;
    /// `SupervisedSession`/`LiveConnector` do not exist in this reconstruction
    /// (`hydra_session`/`hydra_table` do not expose that shape), so the port
    /// takes the session's future instead.  Phase 1: that future is spawned by
    /// the real tokio (`Handle::spawn` / `tokio::spawn`), so `Hosted+0xa0` is a
    /// real `JoinHandle`; only the ten session handles (`Handles`) are not
    /// reproduced.
    pub fn add<F>(&mut self, who: Who, body: F) -> Result<SessionId, AddError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Some(err) = self.may_add(&who.name) {
            return Err(err);
        }
        let id = self.next;
        self.next += 1;
        let task = match &self.rt {
            Some(rt) => rt.spawn(body),
            None => tokio::spawn(body),
        };
        self.sessions.insert(
            id,
            Hosted {
                who,
                handles: Handles::default(),
                token: CancellationToken::new(),
                task,
            },
        );
        Ok(id)
    }
}

impl fmt::Debug for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Host")
            .field("sessions", &self.sessions)
            .field("next", &self.next)
            .finish()
    }
}

impl Default for Host {
    fn default() -> Host {
        Host::on(None)
    }
}

#[cfg(test)]
mod tests;
