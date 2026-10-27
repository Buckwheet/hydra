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
and `hydra_table`.  This reconstruction is **std only**; every runtime type is
replaced by a seam recorded in §4.

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
| `Hosted::stop::{{closure}}` (local) | `0x10006c084` | shape ESTABLISHED, timer/session plumbing replaced | `async fn stop(self, …)` — see §1.8 |
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

### 1.8 `Hosted::stop` — shape ESTABLISHED, plumbing replaced

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
offsets and the `isize::MIN` niche encodings.

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

## 4. Departures from the original (tokio → std)

| original | port | why |
|---|---|---|
| `Host+0x00: Option<tokio::runtime::Handle>` (16 bytes, `CurrentThread`/`MultiThread` tags 0/1, `None` = tag 2) | `Host+0x00: Option<Handle>` where `Handle` is an `Arc<dyn Fn(Box<dyn FnOnce() + Send>) -> Task>`; `Handle::threads()` is the std stand-in. Still 16 bytes. | tokio is unavailable offline. `Host::add`'s branch is preserved exactly: `Some` ⇒ ask the handle, `None` ⇒ spawn ambiently. UNKNOWN: the two scheduler flavours have no std equivalent and are dropped. |
| `Hosted+0xa0: tokio JoinHandle<SupervisedEnd>` | `Task(Box<std::thread::JoinHandle<()>>)` | one thread per session. The `SupervisedEnd` the original handle yields belongs to `hydra_session` and is dropped. |
| `is_running` = `state & COMPLETE == 0` | `!JoinHandle::is_finished()` | Same semantics: a task is running from spawn until it returns *or* is cancelled (tokio sets `COMPLETE` in both cases). |
| `Hosted+0x48..+0xa0`: `mpsc::Sender<Inbox>`, `broadcast::Sender<Event>`, eight `Arc<_>`, `CancellationToken` | one opaque, correctly-sized slot (`Handles([usize; 11])`) | their payload types live in `hydra_session`/`hydra_board`, which *depend on this crate*; nothing in `hydra_host` reads any of them. |
| `Host::add(&mut self, who, session: SupervisedSession<LiveConnector>)` | `Host::add(&mut self, who: Who, body: F)` with `F: FnOnce() + Send + 'static` | `SupervisedSession`/`LiveConnector` are in `hydra_session`/`hydra_table` (circular for this crate). The guard, the id allocation, the spawn decision and the insert are the original's; the ten session handles are not reproduced. |
| `Hosted::stop`'s `SessionHandle::quit` step | caller-supplied `quit: FnOnce()` argument | the session's command channel is a `hydra_session` type. |
| `Hosted::stop`'s `tokio::time::timeout` + `AbortHandle::abort` | poll `Task::is_finished()` against `Instant::now() + STOP_TIMEOUT`, join on success | std timers need no runtime, but a `std::thread` **cannot be aborted**: a task that overruns is left detached instead of aborted. |

Nothing else departs: the port has no `unsafe` beyond the two `transmute_copy`
reads in the tests, and no dependencies (not even path dependencies — the guard
and the map operations need nothing from the workspace).

---

## 5. UNKNOWNs (all marked `// UNKNOWN:` in the source)

1. `SessionId`: alias (`type SessionId = u32`) or `#[repr(transparent)] struct`?
   Every recovered use is bare `u32` arithmetic, so the alias is used.
2. `Who`'s third `String`: never read by any recovered function. It is carried
   through, named `third`, and not interpreted.
3. `Host+0x00`'s `CurrentThread`/`MultiThread` distinction (tags 0/1) — dropped.
4. `Hosted`'s eight `Arc<_>` payloads and the two channel payloads — only their
   sizes and drop glue are recoverable.
5. `Host::may_add`'s receiver: `&self` (chosen) vs `&mut self` — indistinguishable
   in the disassembly.
6. `Host::add`'s error path drops `who`; whether the original `Where`/`Who`
   fields are `pub` is inferred from the absence of a constructor symbol.
7. Whether `Host::add`'s session parameter is consumed by value — yes: the error
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

`crates/hydra_host/src/tests.rs`, 17 tests, all asserting concrete values:

| test | asserts |
|---|---|
| `who_is_three_strings` | `size_of::<Who>() == 0x48`, field offsets 0x00/0x18/0x30 |
| `add_error_costs_no_tag` | `AddError` = 0x30, `Option<AddError>` = 0x30, `None` encodes `isize::MIN` in word 0 (as `may_add` writes at 0x1013b0714) |
| `result_of_add_is_thirty_bytes` | `Result<SessionId, AddError>` = 0x30, `Ok(7)` ⇒ word 0 = `isize::MIN`, word 1 = 7 (as `Host::add` writes at 0x1001c815c) |
| `hosted_layout` | `Hosted` = 0xa8, `who`@0x00, `task`@0xa0, `Option<Hosted>` = 0xa8, `Task` = 1 word |
| `host_layout` | `Host` = 0x30, `rt`@0x00, `sessions`@0x10, `next`@0x28, `Option<Handle>` = 0x10 |
| `on_starts_empty` | a fresh host answers `None`/empty everywhere and hands out id 0 then 1 |
| `get_and_take_by_id` | two entries under distinct u32 ids: names and accounts read back, `get(2)` is `None`, `take(0)` removes only id 0 and only once |
| `take_all_drains_in_id_order` | three entries come back in id order, the table is empty afterwards, ids continue at 3 |
| `may_add_rejects_a_running_duplicate_ignoring_ascii_case` | `"alice"`/`"ALICE"` are refused by a running `"Alice"`, `"bob"`/`""` are not, the error carries the *requested* spelling and the live account, and its `Display` is the recovered literal |
| `may_add_ignores_a_finished_session` | the same name is accepted once the task has ended (`add` ⇒ `Ok(1)`) |
| `may_add_answers_none_for_an_empty_table` | the `root == 0` early exit |
| `add_uses_the_supplied_runtime_handle` | `Host::on(Some(handle))` routes the spawn through the handle (counter == 1) |
| `is_running_until_the_sleeping_task_ends` | true while a thread is blocked on a channel, false once released |
| `is_running_is_false_once_the_task_has_finished` | false for a task that returned |
| `stop_quits_the_session_and_waits_for_the_task` | quit runs, the released task ends inside the timeout |
| `stop_skips_quit_when_the_task_is_already_gone` | a finished task never runs quit (`tbz w0, #0x0` @0x10006c134) |
| `stop_timeout_is_ten_seconds` | `STOP_TIMEOUT == 10 s` (the `w0 = 10, w1 = 0` immediate) |

Gate: `cargo check -p hydra_host` and `cargo test -p hydra_host` both pass in
`.work/host`; no other crate was touched.
