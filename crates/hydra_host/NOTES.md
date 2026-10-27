# `hydra_host` — reverse-engineering notes

Source: the arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra` (no DWARF).
All addresses below are unslid VAs and can be re-read with

```sh
/usr/bin/objdump -d --start-address=0xA --stop-address=0xB --no-show-raw-insn \
  /Users/cpmbpro/Code/hydra-inspect/hydra
```

The crate is one file: `crates/hydra-host/src/lib.rs` is the only source path this
crate contributes to `__cstring` (`0x103e215d1`).  Seven `T` symbols and two
local ones survive, and a fair amount of the crate is visible *inlined into
`hydra_table`*, which is where the guard and the spawn decision are read from.

The original linked `tokio` 1.53.1, `tokio_util`, `hydra_session`, `hydra_board`
and `hydra_table`.  **Phase 1** replaced the std stand-ins with the real crates:
`tokio` 1.53.1 and `tokio-util` 0.7.19 are now dependencies and the runtime seam
(handle, task, cancellation token, timeout/abort) *is* the recovered code.  What
is still a seam — the session body and the ten session handles — is recorded in
§4 and §4a.

Legend:

| tag | meaning |
|---|---|
| **ESTABLISHED** | read out of the instruction stream, the immediates or `__cstring`; not in doubt. |
| **INFERRED** | the shape is fixed by the code and by the surrounding API, but a source-level name or a visibility had to be chosen. |
| **UNKNOWN** | the binary does not pin it down; the safest behaviour is implemented and marked with `// UNKNOWN:` in the source. |

ABI facts used throughout: `&str` is `(ptr, len)`; `String`/`Vec` are
`cap@0, ptr@8, len@0x10` in this build (proved by
`drop_in_place<alloc::string::String>` @0x10126f774, which passes `[x0+0]` as
`dealloc`'s *size* and `[x0+8]` as its pointer); `sret` is x8; `Option<X>` and
`Result<T, X>` use a niche, and for a payload that starts with a `String` that
niche is `isize::MIN` in the capacity word — the port reproduces this and asserts
it in `tests::add_error_costs_no_tag`.

---

## 1. Recovered functions

| symbol | address | status | recovered signature |
|---|---|---|---|
| `Host::on` | `0x1013b054c` | ESTABLISHED | `fn on(rt: Option<Handle>) -> Host` |
| `Host::get` | `0x1013b0560` | ESTABLISHED | `fn get(&self, id: SessionId) -> Option<&Hosted>` |
| `Host::take` | `0x1013b05d4` | ESTABLISHED | `fn take(&mut self, id: SessionId) -> Option<Hosted>` |
| `Host::may_add` | `0x1013b06d4` | ESTABLISHED | `fn may_add(&self, name: &str) -> Option<AddError>` |
| `Host::take_all` | `0x1013b0920` | ESTABLISHED | `fn take_all(&mut self) -> Vec<Hosted>` |
| `Hosted::is_running` | `0x1013b0970` | ESTABLISHED | `fn is_running(&self) -> bool` |
| `Host::add` (local symbol) | `0x1001c7d28` | ESTABLISHED except the session parameter | `fn add(&mut self, who: Who, session: SupervisedSession<LiveConnector>) -> Result<SessionId, AddError>` |
| `Hosted::stop::{{closure}}` (local) | `0x10006c084` | ESTABLISHED (timer/cancel real in Phase 1; `quit` still caller-supplied) | `async fn stop(self, quit: impl FnOnce()) -> bool` — see §1.8 |
| `<AddError as Display>::fmt` | `0x1013b0bc8` | ESTABLISHED | `fn fmt(&self, f: &mut Formatter) -> fmt::Result` |
| `hydra_host::stop_all` | — | **not recovered**, see §6 | free `async fn`; only its drop glue survives |

The crate skeleton carried six stubs (`get`, `may_add`, `on`, `take`,
`take_all`, `is_running`); all six are implemented.  Two further crate members
(`Host::add`, `Hosted::stop`) were recovered from local symbols and are
implemented as well.

Rust-visible types this crate owns, as the binary names them:

* `hydra_host::SessionId` (map key; `BTreeMap<hydra_host::SessionId, hydra_host::Hosted>`,
  `BTreeMap<hydra_host::SessionId, hydra_table::table::Started>`),
* `hydra_host::Who`, `hydra_host::Hosted`, `hydra_host::Host`, `hydra_host::AddError`,
* `hydra_host::stop_all`.

### 1.1 `Host::on` — ESTABLISHED

The whole function is 20 bytes:

```text
1013b054c  str  wzr, [x8, #0x28]      ; out+0x28 = 0
1013b0550  stp  x1,  xzr, [x8, #0x8]  ; out+0x08 = x1, out+0x10 = 0
1013b0554  str  xzr, [x8, #0x20]      ; out+0x20 = 0
1013b0558  str  x0,  [x8]             ; out+0x00 = x0
1013b055c  ret
```

One 16-byte argument (two registers) and a 0x30-byte result.  That it is
`Option<tokio::runtime::Handle>` comes from three places:

* `drop_in_place<hydra_host::Host>` @0x10126f764 names it:
  `drop_in_place<core::option::Option<tokio::runtime::handle::Handle>>` on
  `self + 0`, and the same function drops the `Arc` at `+0x08` through one of two
  different `Arc::drop_slow` instantiations depending on the discriminant at `+0`
  (`cmp x8, #2; b.eq done` — 2 is the `None` niche).
* `Host::add` @0x1001c7e90 reads it back: `ldr x8, [x23]; cmp x8, #0x2` and takes
  the ambient `tokio::task::spawn::spawn` path when it *is* 2.
* The only call site, `hydra::gui::run` → `Table::with_game_runtime`
  @0x101293390, passes `x3 = (a discriminant == 1) as usize` and `x4 = <Arc>`.

`Host::on` writes only the map's root and length, not its height: a null root
makes `BTreeMap`'s height field dead, so the store is elided.

### 1.2 `Host::get` — ESTABLISHED

`(self = x0, key = w1) -> x0`.  Walks the map: `ldr x8, [x0, #0x10]` (root, null
⇒ `None`), `ldr x9, [x0, #0x18]` (height, decremented per level,
`ldr x8, [x8, x10, lsl #3]` + `#0x770` per edge), keys at `node+0x740` with stride
4 (`ldr w14, [x11], #4`, `cmp w1, w14`), values at `node+8` with stride `0xa8`
(`sub x0, x8, #0xa0` then `add x0, x0, #0xa8` per key, so the answer is
`node + 8 + i*0xa8`).  The result is a bare pointer: null is `None`.  Hence
`Option<&Hosted>` and `self.sessions.get(&id)`.

### 1.3 `Host::take` — ESTABLISHED

Same search, then

```text
1013b0648  stp x9, x10, [x29, #-0x30]   ; (node, height)
1013b0650  stp x9', x8, [x29, #-0x20]   ; (index, &self.sessions)
1013b0660  bl  OccupiedEntry::remove_kv
```

`remove_kv` (alloc's own symbol @0x1013af794) leaves `(K, V)` at `sp`: the u32 key
at `sp+0`, the value at `sp+8` (4 bytes of padding).  Only the value is copied to
the sret, 0xa8 bytes, and `None` is `mov x8, #isize::MIN; str x8, [x19]`
(0x1013b0674) — i.e. `self.sessions.remove(&id)` and `Option<Hosted>` with a niche
in `Hosted`'s first `String` capacity.

### 1.4 `Host::take_all` — ESTABLISHED

```text
1013b092c  ldp x9, x10, [x0, #0x10]   ; root, height
1013b0930  ldr x11, [x0, #0x20]       ; length
1013b0934  str xzr, [x0, #0x10]
1013b0938  str xzr, [x0, #0x20]       ; the map is left empty
...
1013b0960  bl  Vec<hydra_host::Hosted>::from_iter   (0x1013a9410)
```

The moved map becomes a 0x48-byte `IntoIter` whose length sits at `+0x40`
(`ldr x23, [x0, #0x40]` in `from_iter`) and `from_iter` multiplies by the element
size `0xa8` — so the answer is `Vec<Hosted>`, the keys are dropped, and the order
is the map's (`SessionId` ascending).  Hence
`mem::take(&mut self.sessions).into_values().collect()`.

### 1.5 `Host::may_add` — ESTABLISHED

`(self = x0, name = (x1 ptr, x2 len)) -> sret x8`, a 0x30-byte
`Option<AddError>`:

```text
1013b06f8  ldr  x8, [x0, #0x10]        ; root
1013b0700  cset w9, ne                 ; root != 0
1013b0704  ldr  x22, [x0, #0x20]       ; length
1013b070c  ccmp x22, #0x0, #0x4, ne
1013b0710  b.ne body                   ; root != 0 && length != 0
1013b0714  mov  x8, #isize::MIN; str x8, [x20]     ; None
```

body, for every entry in key order (the `Iter` construction inlines
`btree::navigate` — that is where the `unwrap_failed` literal at `0x103e695e6`
(`/rustc/31fca3ad…/library/alloc/src/collections/btree/navigate.rs`) comes
from):

```text
1013b07d4  ldr  x0, [x27, #0xa8]       ; entry + 0xa0 = the task handle
1013b07d8  bl   State::load
1013b07dc  tbz  w0, #0x1, candidate    ; bit 1 clear ⇒ still running
```

(`State::transition_to_complete` @0x101e69c58 sets bits 0 **and** 1 and asserts
bit 1 was clear beforehand, so bit 1 is tokio's `COMPLETE`.)

```text
1013b07fc  ldr  x8, [x27, #0x18]       ; entry + 0x10 = name length
1013b0800  cmp  x8, x19                ; == requested length?
1013b0810  ldr  x9, [x27, #0x8]        ; entry + 0x08 = name pointer
…          ldrb/ldrb, ASCII-lowercase both sides, compare byte-wise
1013b0854  …                           ; equal ⇒ build the error
```

The error is `name.to_string()` (bit-63 capacity check, `__rust_alloc` of exactly
`len` bytes @0x1013b087c, `memcpy` from the caller's pointer
@0x1013b0890) followed by
`String::clone` of the live entry's `+0x18` (its account), laid out at
`+0x00` and `+0x18` of the sret.  So:

```rust
fn may_add(&self, name: &str) -> Option<AddError> {
    for hosted in self.sessions.values() {
        if !hosted.is_running() { continue; }
        if hosted.who.name.eq_ignore_ascii_case(name) {
            return Some(AddError { name: name.to_owned(), account: hosted.who.account.clone() });
        }
    }
    None
}
```

Two details worth keeping: the comparison is `eq_ignore_ascii_case` on **bytes**
(`sub w11, w10, #0x41; cmp w11, #0x1a; csel w11, w24(=0x20), wzr, lo; orr`), and
the returned `name` is the caller's spelling, not the stored one.

**INFERRED:** `&self` rather than `&mut self` — the function only reads, but the
register evidence cannot tell the two apart.

### 1.6 `Hosted::is_running` — ESTABLISHED

```text
1013b0978  ldr  x0, [x0, #0xa0]   ; the JoinHandle's raw task
1013b097c  bl   tokio…State::load
1013b0980  tst  x0, #0x2
1013b0984  cset w0, eq            ; running == !complete
```

So it is `!task.is_finished()`: a task counts as running from spawn until it
returns *or* is cancelled.

### 1.7 `Host::add` — ESTABLISHED

Local symbol `hydra_host::Host::add` @0x1001c7d28.  `(self = x0, who = x1 (a
0x48-byte by-value `Who`), session = x2 (a by-value `SupervisedSession`),
sret = x8)`:

```text
1001c7d6c  ldp  x22, x2, [x1, #0x8]    ; who.name's ptr/len
1001c7d7c  bl   may_add
1001c7d84  cmp  [sret], isize::MIN     ; Some ⇒ return the error, drop who+session
1001c7d90  ldr  w25, [x23, #0x28]      ; id = self.next
1001c7d94  add  w8, w25, #0x1
1001c7d98  str  w8, [x23, #0x28]       ; self.next += 1
1001c7e90  ldr  x8, [x23]; cmp x8, #2  ; rt is None?  → ambient spawn
1001c7f78  bl   tokio…Handle::spawn    ; otherwise spawn on the given handle
1001c8158  bl   VacantEntry::insert_entry
1001c815c  str  w25, [x19, #0x8]       ; Ok(id)
1001c8160  str  isize::MIN, [x19]
```

The inserted `Hosted` is assembled from `who` (three `String`s memcpy'd to
`+0x00..0x48`) plus ten handles lifted out of the session
(`ldadd x8, x8, [x21+0x15d8 … +0x1620]` — ten `Arc::clone`s — and the
`CancellationToken::clone` at `+0x15d0`), with the spawn result at `+0xa0`.

### 1.8 `Hosted::stop` — ESTABLISHED (Phase 1: the real plumbing)

`hydra_host::Hosted::stop::{{closure}}` @0x10006c084 is the async body (it is
also polled by `tokio::runtime::task::core::Core::poll`, @0x1000a3c38 and
@0x1000a41e0, because `Table::take_off` spawns it):

```text
10006c130  bl  Hosted::is_running
10006c134  tbz w0, #0x0, skip_quit
10006c158  bl  hydra_session…SessionHandle::quit
10006c180  bl  CancellationToken::cancel        ; Hosted+0x98
10006c1ac  mov w0, #0xa; mov w1, #0x0           ; Duration::from_secs(10)
10006c1b4  bl  tokio::time::timeout             ; over the JoinHandle
10006c2ec  bl  AbortHandle::abort               ; on Elapsed
```

Its first poll copies the whole 0xa8-byte `Hosted` (and its `JoinHandle`) into the
generator, so `stop` takes `self` by value.

Only `skip_quit` is conditional: the cancel of step 2 and the timeout of step 3
run in both paths, which is why the port is

```rust
pub async fn stop<Q: FnOnce()>(self, quit: Q) -> bool {
    if self.is_running() { quit(); }
    self.token.cancel();                       // the real CancellationToken
    let abort = self.task.abort_handle();      // taken before `timeout` borrows it
    let mut task = self.task;
    match tokio::time::timeout(STOP_TIMEOUT, &mut task).await {
        Ok(_) => true,
        Err(_) => { abort.abort(); false }
    }
}
```

`quit` is the one remaining stand-in — see §4a.

---

## 2. Recovered layouts

`String` = `cap@0, ptr@8, len@0x10`, size 0x18.

```text
Host        0x30
  +0x00  Option<tokio::runtime::Handle>   16 bytes: discriminant@0 (2 = None), Arc@8
  +0x10  BTreeMap<SessionId, Hosted>      24 bytes: root@0, height@8, length@16
  +0x28  next: SessionId (u32)            + 4 bytes of tail padding

Who         0x48     three Strings: name@0x00, account@0x18, third@0x30
AddError    0x30     two Strings:   name@0x00, account@0x18

Hosted      0xa8
  +0x00  Who                              0x48  (name, account, third)
  +0x48  tokio::sync::mpsc::Sender<_>     8     (drop: Tx::close + AtomicWaker::wake)
  +0x50  tokio::sync::broadcast::Sender<hydra_session::event::Event>
  +0x58  Arc<_>   +0x60 Arc<_>   +0x68 Arc<_>   +0x70 Arc<_>
  +0x78  Arc<_>   +0x80 Arc<_>   +0x88 Arc<_>   +0x90 Arc<_>
  +0x98  tokio_util::sync::CancellationToken
  +0xa0  tokio::runtime::task::JoinHandle<hydra_session::supervisor::SupervisedEnd>
```

`Hosted`'s tail is read out of `drop_in_place<hydra_host::Hosted>` @0x1013a0594:
three `String` deallocations at +0x00/+0x18/+0x30, `mpsc`'s `Tx::close` +
`AtomicWaker::wake`, `broadcast::Sender::drop`, eight *distinct*
`Arc::drop_slow` instantiations at +0x58…+0x90, `CancellationToken::drop` at
+0x98 and `State::drop_join_handle_fast` / `RawTask::drop_join_handle_slow` for
the handle at +0xa0 (0xa8 bytes in total, matching the `stride 0xa8` of the map's
values).  `SessionId` is four bytes wide (`cmp w1, w14` on keys, `str w25,
[x19, #0x8]` for the answer of `add`).

The port keeps those offsets exactly: the four types are `#[repr(C)]` and
`tests::{host_layout, hosted_layout, who_is_three_strings,
add_error_costs_no_tag, result_of_add_is_thirty_bytes}` assert the sizes,
offsets and the `isize::MIN` niche encodings.  **Phase 1 measured the real
tokio types against this table and every number still holds** — `Host` 0x30,
`Hosted` 0xa8 (see §4a for the measurements).

---

## 3. `AddError`'s message

`<AddError as Display>::fmt` @0x1013b0bc8 (76 bytes) formats one literal with the
two `String`s.  The literal is `__cstring` @0x103e696a7:

```text
{} is already logged in on account {}; the game allows one character per account at a time
```

`hydra_table`'s caller wraps it once more: `__cstring` @0x103e21675 is
`"{} not started: {}"`, and the format argument list built at 0x1001367b4 uses
`<AddError as Display>::fmt` (0x1013b0bc8) as the second argument's vtable, then
`drop_in_place<hydra_host::AddError>` — independent confirmation that `may_add`'s
`Some` payload is an `AddError`.

---

## 4. Departures from the original

Phase 1 struck every stand-in that the real `tokio`/`tokio-util` could take over.
What is left is the session body and the ten session handles.

| original | port | why |
|---|---|---|
| ~~`Host+0x00: Option<tokio::runtime::Handle>` (16 bytes, `CurrentThread`/`MultiThread` tags 0/1, `None` = tag 2)~~ | ~~a std closure handle, `Handle::threads()`~~ → **reconnected**: `Option<tokio::runtime::Handle>`, unchanged shape | tokio 1.53.1 is in the workspace pins.  `Host::add`'s branch is the recovered one: `Some` ⇒ `Handle::spawn`, `None` ⇒ ambient `tokio::spawn`.  The two scheduler flavours are back for free (§4a). |
| ~~`Hosted+0xa0: tokio JoinHandle<SupervisedEnd>`~~ | ~~`Task(Box<std::thread::JoinHandle<()>>)`~~ → **reconnected**: a real `tokio::task::JoinHandle<()>`, one word at +0xa0 | the `SupervisedEnd` output type belongs to `hydra_session` and does not exist here, so the output is `()`.  The three handle operations (`is_finished`, `abort_handle`, `timeout`) are the original's. |
| ~~`is_running` = `state & COMPLETE == 0`~~ | ~~`!JoinHandle::is_finished()` on a thread~~ → **reconnected**: `!JoinHandle::is_finished()` on the tokio handle | same semantics, now literally the same `State::load` test: a task is running from spawn until it returns *or* is cancelled. |
| ~~`Hosted::stop`'s `tokio::time::timeout` + `AbortHandle::abort`~~ | ~~poll `is_finished` against `Instant::now() + STOP_TIMEOUT`~~ → **reconnected**: `async fn stop` = `tokio::time::timeout(STOP_TIMEOUT, &mut task)` and `abort_handle().abort()` on `Elapsed` | a `std::thread` cannot be aborted; a `JoinHandle` can, and is. |
| ~~`Hosted+0x98: CancellationToken`~~ | ~~inside the opaque `Handles` slot~~ → **reconnected**: a real `tokio_util::sync::CancellationToken` field at +0x98, cancelled by `stop` | it is one word, so the recovered offsets are unaffected. |
| `Hosted+0x48..+0x98`: `mpsc::Sender<Inbox>`, `broadcast::Sender<Event>`, eight `Arc<_>` | one opaque, correctly-sized slot (`Handles([usize; 10])`, 0x50 bytes) | **still a seam**: their payload types live in `hydra_session`/`hydra_board` and are not exposed (no `SupervisedSession`, no `hydra_session::event` module), so they cannot be named here; nothing in `hydra_host` reads any of them. |
| `Host::add(&mut self, who, session: SupervisedSession<LiveConnector>)` | `Host::add(&mut self, who: Who, body: F)` with `F: Future<Output = ()> + Send + 'static` | **still a seam**: `SupervisedSession`/`LiveConnector`/`SupervisedEnd` do not exist in this reconstruction.  The guard, the id allocation, the spawn decision and the insert are the original's, and the spawn is real tokio; the ten session handles are not reproduced. |
| `Hosted::stop`'s `SessionHandle::quit` step | caller-supplied `quit: FnOnce()` argument | **still a seam**: `hydra_session::command::SessionHandle` has no `quit` (see §4a). |

Nothing else departs: the port has no `unsafe` beyond the `transmute_copy` reads
in the tests, and its only dependencies are `tokio` and `tokio-util` (no path
dependencies — the guard and the map operations need nothing from the
workspace).

### 4a. Phase 1 reconnection (tokio 1.53.1, tokio-util 0.7.19)

Versions are the workspace pins (`crates/hydra_host/Cargo.toml` uses
`tokio = { workspace = true }`, `tokio-util = { workspace = true }`); everything
was built with `cargo --offline`.  Nothing in the root manifest changed.

Layout, measured by `tests::{host_layout, hosted_layout}` (not assumed):

| type | size | note |
|---|---|---|
| `Option<tokio::runtime::Handle>` | **0x10** | matches the original's 16-byte slot: tokio's `Handle` wraps a two-variant `scheduler::Handle` enum, which takes a tag word plus the `Arc` instead of a single niche pointer, so the `Option` is `tag@0, Arc@8` with `None` = tag 2 — exactly what `Host::on`/`Host::add` (`cmp x8, #2`) show |
| `Host` | **0x30** | `rt`@0x00, `sessions`@0x10, `next`@0x28 — the recovered offsets, unchanged |
| `tokio::task::JoinHandle<()>` | **0x08** | `NonNull<RawTask>` + `PhantomData`; `Option<JoinHandle>` is also 8 |
| `tokio_util::sync::CancellationToken` | **0x08** | `Arc<TreeNode>` |
| `Handles([usize; 10])` | **0x50** | the opaque stand-in for `+0x48..+0x98` |
| `Hosted` | **0xa8** | `who`@0x00, `handles`@0x48, `token`@0x98, `task`@0xa0 — recovered offsets and the recovered size |
| `Option<Hosted>` | **0xa8** | `None` is still `isize::MIN` in the leading `String` capacity word, as `Host::take` writes it |

So the two recovered sizes (`Host` 0x30, `Hosted` 0xa8) survive the reconnection
and every `isize::MIN` niche test still holds; no ABI change had to be recorded.

What is **connected** now:

* `Host::on(Option<tokio::runtime::Handle>)` — the recovered signature, and
  `Host::add` spawns with `Handle::spawn` when it is `Some` and with the ambient
  `tokio::spawn` when it is `None` (`0x1001c7f78` / `0x1001c7ea8`).  With no
  runtime at all the ambient arm now panics with tokio's own message; the std
  port silently started a thread instead.
* `Hosted`'s task is a real `JoinHandle`, so `is_running` is literally the
  `COMPLETE`-bit test of `0x1013b0978`.
* `Hosted::stop` is an `async fn` (`0x10006c084` polled by the runtime) that
  cancels the token (`0x10006c180`), wraps `tokio::time::timeout` over the
  join handle (`0x10006c1b4`) and aborts on `Elapsed` (`0x10006c2ec`).
  `STOP_TIMEOUT` is still 10 s, the recovered immediate.
* The `+0x98` slot is the real `CancellationToken`; it is created by
  `Host::add` and lent to readers by `Hosted::stop_token` (a Phase 1 addition —
  the original instead cloned the session's own token at `+0x15d0`, so the
  session held a second clone and observed the cancel).

What is **still a seam**, and why:

* **`SupervisedSession` / `SupervisedEnd` / `LiveConnector`.**  Verified absent:
  `hydra_session::supervisor` exposes only `jitter`, `backoff`,
  `answer_between` (+ `answer_between_async`) and has no session type,
  and `hydra_table` has no `LiveConnector`.  `hydra_session::command::Event`
  exists but there is no `hydra_session::event` module.  So `Host::add` keeps a
  generic body parameter (`F: Future<Output = ()> + Send + 'static`, spawned by
  real tokio) and `Hosted+0xa0` is `JoinHandle<()>`.  A future is the faithful
  shape — the original spawned the supervisor's future, not a closure — and it
  is what makes abort observable.
* **The ten session handles** (`+0x48..+0x98`).  They cannot be named without
  those types, so they stay an opaque `[usize; 10]`.
* **`SessionHandle::quit`.**  No sibling dependency was taken for it: nothing
  depends on `hydra_host` (so there is no cycle to worry about), but
  `hydra_session::command::SessionHandle` has **no** `quit` — it has
  `stop(reason)` (trips its own token, emits `Event::Over`) and `stop_token()`.
  Using `stop` in `quit`'s place would either need a sibling edit or would
  silently change what the recovered call does, so the caller keeps supplying
  the quit step.  When the real session type appears, `stop` should take the
  session instead and clone its token into `+0x98`.

Conflict with the original dependency graph: none material.  The original linked
`hydra_session`/`hydra_board`/`hydra_table`; this crate links neither because the
types it would use do not exist yet, and no sibling currently links
`hydra_host`, so adding that edge later is a one-line change on either side.

Test-only departure: `tokio`'s `test-util` feature is enabled as a
**dev-dependency** (`[dev-dependencies] tokio = { workspace = true, features =
["test-util"] }`) so the abort test can run on tokio's paused clock instead of
sleeping 10 real seconds.  The workspace pin (`features = ["full"]`) does not
include it and the root manifest must not be edited.

---

## 5. UNKNOWNs (all marked `// UNKNOWN:` in the source)

1. `SessionId`: alias (`type SessionId = u32`) or `#[repr(transparent)] struct`?
   Every recovered use is bare `u32` arithmetic, so the alias is used.
2. `Who`'s third `String`: never read by any recovered function. It is carried
   through, named `third`, and not interpreted.
3. `Hosted`'s eight `Arc<_>` payloads and the two channel payloads — only their
   sizes and drop glue are recoverable.
4. `Host::may_add`'s receiver: `&self` (chosen) vs `&mut self` — indistinguishable
   in the disassembly.
5. `Host::add`'s error path drops `who`; whether the original `Where`/`Who`
   fields are `pub` is inferred from the absence of a constructor symbol.
6. Whether `Host::add`'s session parameter is consumed by value — yes: the error
   path calls `drop_in_place<SupervisedSession<LiveConnector>>` on it.

---

## 6. Not recovered

* `hydra_host::stop_all` — a free `async fn` whose own symbol is absent; only its
  drop glue survives (`core::ptr::drop_in_place<hydra_host::stop_all::{{closure}}>`,
  `Stage<stop_all::{{closure}}::{{closure}}>`,
  `Cell<stop_all::{{closure}}::{{closure}}, Arc<current_thread::Handle>>`,
  `Cell<…, Arc<multi_thread::handle::Handle>>`, and
  `Map<vec::IntoIter<(usize, Option<hydra_session::supervisor::SupervisedEnd>)>,
  stop_all::{{closure}}::{{closure}}>`).  That is enough to say it is a spawned
  async task that consumes a `Vec<(usize, Option<SupervisedEnd>)>`, and nothing
  more: no signature, no ordering, no timeout.  It is **not** implemented rather
  than invented.
* `Host`'s public surface beyond the functions above (e.g. an iterator over the
  hosted sessions): `hydra_table` walks the host only through
  `get`/`take`/`take_all`/`may_add`/`add` at every recovered call site.

---

## 7. Tests

`crates/hydra_host/src/tests.rs`, 21 tests, all asserting concrete values.  Phase
1 runs them on a real tokio runtime (`#[tokio::test]`, `current_thread` where the
scheduling does not matter); the "still running" sessions hold a `oneshot` open,
so no runtime thread is ever blocked, and `until` yields to the runtime instead of
sleeping.  Every pre-Phase-1 assertion is kept verbatim.

| test | asserts |
|---|---|
| `who_is_three_strings` | `size_of::<Who>() == 0x48`, field offsets 0x00/0x18/0x30 |
| `add_error_costs_no_tag` | `AddError` = 0x30, `Option<AddError>` = 0x30, `None` encodes `isize::MIN` in word 0 (as `may_add` writes at 0x1013b0714) |
| `result_of_add_is_thirty_bytes` | `Result<SessionId, AddError>` = 0x30, `Ok(7)` ⇒ word 0 = `isize::MIN`, word 1 = 7 (as `Host::add` writes at 0x1001c815c) |
| `hosted_layout` | `Hosted` = 0xa8, `who`@0x00, `handles`@0x48 (`Handles` = 0x50), `token`@0x98 (`CancellationToken` = 8), `task`@0xa0 (`JoinHandle<()>` = 8), `Option<Hosted>` = 0xa8 with `None` = `isize::MIN` |
| `host_layout` | `Host` = 0x30, `rt`@0x00, `sessions`@0x10, `next`@0x28, `Option<Handle>` = `Handle` = 0x10 |
| `on_starts_empty` | a fresh host answers `None`/empty everywhere and hands out id 0 then 1 |
| `get_and_take_by_id` | two entries under distinct u32 ids: names and accounts read back, `get(2)` is `None`, `take(0)` removes only id 0 and only once |
| `take_all_drains_in_id_order` | three entries come back in id order, the table is empty afterwards, ids continue at 3 |
| `may_add_rejects_a_running_duplicate_ignoring_ascii_case` | `"alice"`/`"ALICE"` are refused by a running `"Alice"`, `"bob"`/`""` are not, the error carries the *requested* spelling and the live account, and its `Display` is the recovered literal |
| `may_add_ignores_a_finished_session` | the same name is accepted once the task has ended (`add` ⇒ `Ok(1)`) |
| `may_add_answers_none_for_an_empty_table` | the `root == 0` early exit |
| `add_uses_the_supplied_runtime_handle` | **Phase 1**: `Host::on(Some(multi_thread handle))` really spawns there — the body reports `Handle::current().runtime_flavor() == MultiThread` while the test itself runs on a `current_thread` runtime |
| `add_without_a_handle_spawns_on_the_ambient_runtime` | **Phase 1**: `Host::on(None)` spawns on the ambient runtime (`RuntimeFlavor::CurrentThread` from inside the body), and the task then finishes |
| `add_without_a_runtime_panics` | **Phase 1**: outside any runtime the ambient arm panics with tokio's `"there is no reactor running…"` |
| `is_running_until_the_sleeping_task_ends` | true while a session future is parked on a `oneshot`, false once released |
| `is_running_is_false_once_the_task_has_finished` | false for a task that returned |
| `stop_quits_the_session_and_waits_for_the_task` | quit runs, the released task ends inside the timeout |
| `stop_skips_quit_when_the_task_is_already_gone` | a finished task never runs quit (`tbz w0, #0x0` @0x10006c134) |
| `stop_aborts_the_task_after_the_timeout` | **Phase 1**: a task that never ends outlives `STOP_TIMEOUT` ⇒ `stop` answers `false` and `AbortHandle::abort` (@0x10006c2ec) drops its future (checked through a `Drop` guard); the ten recovered seconds are tokio's *paused* clock, so the test is instant |
| `stop_trips_the_cancellation_token` | **Phase 1**: the token at `+0x98` is untripped when `quit` runs (@0x10006c158) and tripped afterwards (@0x10006c180); `token.cancelled()` waiters resolve |
| `stop_timeout_is_ten_seconds` | `STOP_TIMEOUT == 10 s` (the `w0 = 10, w1 = 0` immediate) |

Gate: `cargo check --offline -p hydra_host` and `cargo test --offline -p hydra_host`
both pass in `.work/p1-hydra_host` (21/21), and `cargo check --offline
--workspace --all-targets` is green with the new dependencies.  No other crate was
touched; the only file outside `crates/hydra_host` that changed is the root
`Cargo.lock` (it records the two new edges).

