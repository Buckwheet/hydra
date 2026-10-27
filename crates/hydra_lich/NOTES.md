# `hydra_lich` — reverse-engineering notes

Source: arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra`, symbols at
`symbols_demangled.txt`; addresses below are unslid VAs.  This crate is a
**std-only** reconstruction: the original linked `tokio`, `getrandom`,
`tokio_util::sync::CancellationToken` and `hydra_session::lich::LichDoor`.
No other Hydra crate is a dependency of this crate, so the recovered pieces
are expressed as owned data structures plus a synchronous relay.

Legend:
* **ESTABLISHED** — directly visible in the disassembly/strings.
* **INFERRED** — follows from the code shape or from the surrounding
  `hydra_session` API, but the original type/name is not in our crate.
* **UNKNOWN** — not recoverable from this binary; safest behaviour chosen.

---

## 1. Recovered functions

| function | address | recovered signature (source-level) |
| --- | --- | --- |
| `child::is_password` | `0x101683420` | `fn(&str) -> bool` |
| `child::command` (helper) | `0x101683638` | `fn(&Launch) -> std::process::Command` |
| `find_ruby` | `0x101689960` | `fn() -> Option<PathBuf>` |
| `new_token` | `0x101689d30` | `fn() -> Result<String, getrandom::Error>` |
| `relay::read_lines` | `0x10167dcc0` | `fn(OwnedReadHalf) -> mpsc::Receiver<String>` |
| `relay::keep_errors` | `0x10167dd7c` | `fn(ChildStderr) -> (Arc<Mutex<VecDeque<String>>>, JoinHandle<()>)` |
| `relay::never_given` | `0x10167de8c` | `fn(Vec<String>, <by-value String collection>) -> Vec<String>` |
| `relay::shared_symbol` | `0x10167decc` | `fn(char, char) -> Option<Notice>` |
| `relay::run` | `0x10167dfa0` | `async fn(&Launch, LichDoor, CancellationToken) -> …` |
| `relay::show` | `0x10167e060` | `fn(&ChildStderr-ish, &LichDoor) -> (handle, door)` |
| `relay::Ended::notice` | `0x10167e138` | `fn(&Ended) -> Option<Notice>` |
| `relay::Waits::new` | `0x10167e430` | `fn(&Shared, Arc<…>) -> Waits` |
| `relay::Waits::tell` | `0x10167e58c` | `fn(&mut Waits, &LichDoor)` |
| `relay::Waits::typed` | `0x10167e668` | `fn(&mut Waits, &Session, LichDoor, String)` |
| `relay::Waits::hand_held` | `0x10167e780` | `fn(&mut Waits, &Session, LichDoor)` |
| `relay::spawn` | `0x10167e860` | `async fn(&Launch, u16) -> Result<Child…, io::Error>` |

Rust ABI facts used while reading it:

* `Vec<T>` is laid out **cap @ +0, ptr @ +8, len @ +16** on this toolchain;
  `Vec::new()` stores `(0, align_of::<T>(), 0)`.
* `&str` / `Path` is `(ptr x0, len x1)`, `String`/`PathBuf` is the 24-byte
  `Vec` above, and 24-byte `String` slices have stride 24.
* `Option` niche values and enum tags are visible in the sret structs; the
  relay `Notice` is a 64-byte value whose first word is a discriminant and
  whose `+0x38`/`+0x39` bytes are flags.
* A 15-byte `&str` prefix comparison is inlined into `is_password`; the
  surrounding 23 KB is the inlined `eq_ignore_ascii_case` + UTF-8 boundary
  code, not extra semantics.

### 1.1 `child::is_password` — ESTABLISHED

```rust
name.len() >= 15
    && name.is_char_boundary(15)
    && name[..15].eq_ignore_ascii_case("hydra_password_")
```

It is called only from `child::command` (`0x1016836a8`), once per
`std::env::vars_os()` entry.  Nothing after byte 14 is inspected, so
`HYDRA_PASSWORD_` alone matches.  The `is_char_boundary(15)` guard is real:
a 14-byte ASCII prefix followed by a 2-byte UTF-8 character puts a
continuation byte at index 15, and the binary returns `false` instead of
panicking.  Tests cover positive, negative, mixed case and that exact
boundary case.

### 1.2 `find_ruby` — ESTABLISHED

1. `std::env::var_os("PATH")`; `env::split_paths` in order; first
   `dir.join("ruby")` for which `Path::is_file()` is true wins.
2. `read_dir(r"C:\Ruby4Lich5")` is attempted **unconditionally** (there is
   no `#[cfg(windows)]` in the binary; on Unix it just fails).  Each entry
   becomes `(Vec<u32> version, PathBuf)`, the list is sorted, the last entry
   is taken, then `"bin"` and `"ruby"` are joined:
   `C:\Ruby4Lich5\<newest>\bin\ruby`.
3. Otherwise `None`.

The version parse is inferred from the 48-byte `(Vec<u32>, PathBuf)` tuple
and the stable sort call: dotted numeric components compare numerically
(`3.2.10 > 3.2.9`), non-numeric names are dropped.  **UNKNOWN:** the exact
filter closure was inlined; our `newest_version_dir` helper is the
documented equivalent.  Step 2 does *not* check `is_file`, matching the
recovered code.

### 1.3 `new_token` — ESTABLISHED

`getrandom` fills **32 bytes**, then each byte becomes two lowercase hex
digits; the `String` length is 64.  On RNG error the `getrandom::Error` is
formatted with `Display` into `Err`.

std-only equivalent: `File::open("/dev/urandom").read_exact(&mut [u8; 32])`
(Unix).  **UNKNOWN:** the binary has no fallback; if `/dev/urandom` is
unavailable we use a SplitMix64 stream seeded from clock + pid + stack
address so the relay still starts, and document that this is not
cryptographic.  The public error type is `child::TokenError` because
`getrandom::Error` cannot be named without the dependency.

### 1.4 `relay::Waits` — ESTABLISHED structure, INFERRED ownership

Recovered layout of `Waits` (offsets from `Waits::new`/`typed`/`tell`):

```
+0x00  Vec<String> pending   cap
+0x08                       ptr
+0x10                       len
+0x18  Instant deadline (secs)      = now + 10 s
+0x20  Instant deadline (nanos)
+0x28  pointer to shared session state (read under an RwLock<bool>)
+0x30  second handle (LichDoor-ish)
+0x38  bool hand_held
+0x39  bool tell/told
```

`Waits::new` takes a read lock on a `std::sync::RwLock<bool>` at
`shared+0x150`, reads the byte at `+0x159` into `hand_held`, peeks at a
`tokio::sync::watch::Receiver<bool>` at `+0x160`, and stores
`Instant::now() + Duration::from_secs(10)`.

* `typed` (**ESTABLISHED**): if `hand_held` is set, call `hand_typed`
  immediately (no queue, no notice).  Else push; **only if the queue was
  empty before the push**, emit `Notice::line(2, "Lich is still logging in;
  what you typed follows once it knows the character.")` and say it through
  the door.
* `tell` (**ESTABLISHED**): set `told = true`; if the queue is non-empty,
  format `"Lich has not said it knows the character yet: {n} line(s) wait,
  and go as soon as it does."` and `LichDoor::say` it.
* `hand_held` (**ESTABLISHED**): if the flag is set, drain the whole vector
  (FIFO) and call `hand_typed` for each; the flag is not cleared.
* `never_given` (**ESTABLISHED shape**): builds
  `Chain<vec::IntoIter<String>, vec::IntoIter<String>>` and collects it into
  a new `Vec<String>` (the monomorphised
  `SpecFromIterNested::from_iter` at `0x101681fc8`).  Observable behaviour:
  concatenate the pending vector with the recovered vector, preserving both
  orders.  **UNKNOWN:** the caller's second value comes from a task result
  whose type alias prints as `Vec<Vec<u8>>`, yet the iterator element stride
  is 24 and the element type is `String`; we model the second argument as
  `Vec<String>`.
* `shared_symbol` (**ESTABLISHED**): compares two `char`s (`w0`/`w1`).  If
  different, returns the no-notice discriminant.  If equal, formats the
  0x103982ad0 template:
  `Hydra's commands start with '{s}', as Lich's do, so a line typed with
  '{s}' is Hydra's and Lich's commands can't be reached. Give Hydra another
  command symbol, such as ., on the settings menu's General page.`
* `Ended::notice` (**ESTABLISHED text**): a match over an `Ended` enum
  produces exactly these notices (the 0x103e6d… templates):

```
Lich stopped.                                                        level 2
Lich exited (<status>).                                              level 1
Lich did not connect within <n> seconds, and was stopped.            level 0  (binary n = 60)
Lich fell <n> chunks behind the game, and was stopped: a stream with
  a hole in it would mislead its scripts.                            level 1
Lich already runs for this character.                                level 0
Lich could not start: <error>                                        level 0
(none)                                                               kind 4 / None
```

**UNKNOWN:** the exact `Ended` discriminant numbering.  The branch decode
uses negative tags plus a non-negative default; the string-to-cause mapping
above is certain, the enum spelling is ours.

### 1.5 `read_lines` / `keep_errors` — ESTABLISHED

* `read_lines`: `BufReader` (8192-byte allocation), reads until `\n`,
  `String::from_utf8_lossy`, bounded `tokio::sync::mpsc` with
  `Semaphore::new(0x40)` = **capacity 64**; detached task; sender closes at
  EOF.  std equivalent: `std::sync::mpsc::sync_channel(64)` plus a thread
  (backpressure identical).  The binary strips the delimiter (`Lines`-style);
  our reader strips `\n` and a preceding `\r`, replaces invalid UTF-8 with
  U+FFFD, and disconnects the receiver at EOF.
* `keep_errors`: allocates `Arc<Mutex<VecDeque<String>>>` itself, reads
  `ChildStderr` lines, and **keeps the last 20** (`cmp x8, #0x14`; pop the
  front while full, then push back).  Returns `(Arc, JoinHandle)`.
  std equivalent: same ring under `Arc<Mutex<…>>` with a reader thread.

---

## 2. IPC and lifecycle

### 2.1 Process command line — ESTABLISHED

`spawn` first resolves the configured `lich.rbw` to an absolute path
(`std::sys::path::absolute`), calls `child::command` (Ruby program + strip
`HYDRA_PASSWORD_*` from the inherited environment), then appends, in this
order:

```
-e "trace_var(:$frontend) { |name| $frontend = \"stormfront\" if name == \"unknown\" }"
-e "$0 = ARGV.shift; load $0"
<absolute path to lich.rbw>
--pipe
--stormfront
--sentinel
-g
127.0.0.1:<port>          # template 0x103e6df18, port is a u16
<launch.args...>          # game/character arguments (Vec<String> at +0x38)
```

Then every `(key, value)` pair from `launch.env` is set with
`CommandEnv::set`, stdin/stdout/stderr are set to `Stdio::piped()`, and the
working directory is set to `Path::parent(lich.rbw)` when it is non-empty.
`Command::spawn` is called; an `io::Error` becomes the `CouldNotStart`
outcome.

```
parent (Hydra session)                         child (ruby lich.rbw)
   typed line / game command  ───────────────▶  stdin
   child stdout ──▶ BufReader ──▶ bounded(64) ──▶ line reader ──▶ frame parser
                                                        │
                                                        └─ hydra_protocol::parser
                                                           (character stream)
   child stderr ──▶ last-20 ring (Arc<Mutex<VecDeque<String>>>)
   child exit   ──▶ Child::wait ──▶ Ended::notice ──▶ LichDoor::say
```

* **stdin** carries game commands and typed lines.  The recovered
  `hand_typed` path writes to the child stdin through the session handle;
  `Waits` only holds lines until Lich says it knows the character.
* **stdout** is *not* parsed in `hydra_lich`: `read_lines` yields `String`s;
  the relay loop feeds them to `hydra_session`/`hydra_protocol` (the frame
  parser lives in `hydra_protocol`).  Our crate exposes the receiver and
  records lines in `Relay::output`; it cannot depend on `hydra_protocol`
  (std-only rule).
* **stderr** is never shown directly; it is kept so a failed Lich can be
  diagnosed (`Errors::tail` in the original).  Only the last 20 lines are
  retained.
* Lich also speaks a local TCP protocol: the command line tells it to
  `--pipe` on `127.0.0.1:<port>`, and the relay connects back and performs
  the keyed accept (`relay::accept_keyed` closure).  The 64-char
  `new_token` value is the key for that local handshake (see §3.2).

### 2.2 Lifecycle — ESTABLISHED events, INFERRED orchestration

The relay loop (`hydra_lich::relay::run::{{closure}}` at `0x10112ec94`, plus
`show::{{closure}}`, `feed`, `reap`, `exited`, `finish`, `let_go`,
`accept_keyed`, `Errors::tail`) is a `tokio::select!` over:

1. `LichDoor` command events (`claim`, `say`, `symbol`, `lich_symbol`,
   `session_over`, `hand_to_lich`, …);
2. a `watch::Receiver<bool>` (visibility / hand-held changes);
3. `Waits` deadline (`sleep`);
4. the bounded stdout receiver (`read_lines`);
5. a `Typing::next` stream of typed lines;
6. `Child::wait` (process exit);
7. a `CancellationToken`.

Recovered transitions:

* start: `show` → reader + writer + stderr tasks → `Waits::new`; if the
  symbol returned by `LichDoor::symbol` equals `lich_symbol`, immediately
  say the shared-symbol warning.
* character known: `Waits::hand_held` drains queued typed lines to Lich.
* Lich not yet acknowledging: `tell` emits the "not said it knows the
  character yet" notice at most once, then the pending lines go when the
  hold is released.
* child exit / read failure / cancellation: `Ended::notice` maps the reason,
  `LichDoor::say` delivers it, stderr ring is available, child is reaped.
* "fell behind" (`LichDoor::fell_behind` / `Wire::fell_behind`): the stream
  had a hole; Lich is stopped rather than desynchronising scripts.
* "already running": `LichDoor::show` reports a second relay for the same
  character; no child is spawned.
* "did not connect within 60 seconds": the accept-keyed race times out.
* "could not start": `Command::spawn` failed.

Our std-only `run`/`show` implement the same *shape* (spawn → wire readers →
pump lines until exit → map to `Ended`) without the tokio select, session
crate or frame parser.  **UNKNOWN:** exact cancellation ordering and the
Lich TCP wire framing of the token.

### 2.3 Password / environment flow

**ESTABLISHED:** `child::command` iterates `std::env::vars_os()` and calls
`CommandEnv::remove` for every name matched by `is_password`.  This happens
before any arg/env additions, so *no* `HYDRA_PASSWORD_*` variable reaches
`ruby lich.rbw`.  The recovered prefix literal is `HYDRA_PASSWORD_`
(`0x103e613d1`), and the 15-byte check ignores the suffix, so one variable
per password (e.g. `HYDRA_PASSWORD_GEMSTONE`, `HYDRA_PASSWORD_<account>`)
can be carried by Hydra's own environment.

Why remove them?  They **carry the Lich password**, and the child runs
user-supplied Ruby (`lich.rbw` plus scripts/macros), so the password must
not be readable from the child's environment.  Hydra keeps the value in its
own process; when Lich prints a password prompt on its stdout, Hydra
recognises it and writes the password to the child's stdin (the same
parent→child stdin channel used for game commands).  The Lich password is
therefore never in argv either.

**INFERRED:** the prompt recognition itself is in `hydra_session`/the relay
loop, not in these 15 functions; no prompt literal appears inside the
`hydra_lich` address range.  The exact prompt text/prefix is **UNKNOWN**.
Our crate exposes the stdin writer (`Relay::send`) and leaves prompt
recognition to the caller; it does document that the password channel is
the removed `HYDRA_PASSWORD_*` environment, not the command line.

### 3.2 Token flow

**ESTABLISHED:** `new_token` is called exactly twice in the binary:

* `hydra::instance::claim` (`0x1001145ec`) — a 64-char token for the local
  Hydra instance/claim protocol;
* the Lich relay loop (`0x10112ee04`) — the token is stored as a `String`
  at `+0x2f8` in the relay state, then handed to the `accept_keyed` future
  together with the TCP connect to `127.0.0.1:<port>` and a 60-second
  timeout.

**INFERRED:** Lich (started with `--pipe --stormfront --sentinel -g`) listens
on the given port; Hydra connects and proves it is the parent by sending the
token as the keyed first line.  **UNKNOWN:** the exact wire encoding
(newline/JSON/length prefix) of that keyed accept, because `accept_keyed`'s
body is inlined into the relay closure.

---

## 4. std-only departures (all marked in code)

| original | std-only reconstruction | why |
| --- | --- | --- |
| `tokio::sync::mpsc::channel(64)` | `std::sync::mpsc::sync_channel(64)` + thread | no tokio |
| `tokio::io::BufReader`/`Lines` | `std::io::BufReader` + `read_until` | no tokio |
| `tokio::sync::Mutex`/`RwLock`/`watch` | `std::sync::Mutex`, plain flags | no tokio |
| `tokio::process::Command/Child` | `std::process::Command/Child` + threads | no tokio |
| `getrandom` | `/dev/urandom`, documented fallback | no dependency |
| `hydra_session::lich::LichDoor` | `Relay` + `Notice` + `Typed` | separate crate not a dependency |
| `hydra_protocol` frame parsing | caller consumes `String` lines | std-only rule |
| `CancellationToken` | `Relay::stop` / `Drop` kill+wait | no tokio_util |

Everything marked `// UNKNOWN:` in the source expands the points above.

## 5. Tests

`cargo test -p hydra_lich` covers:

* `is_password`: positive/negative/mixed case, 14-byte non-match, suffix
  ignored, UTF-8 boundary at byte 15.
* `new_token`: exactly 64 lowercase hex chars, two calls differ, known hex
  vectors.
* `find_ruby`: first PATH hit, PATH-before-version-dir, numeric version
  ordering (`3.2.10 > 3.2.9`), missing root.
* `filter_password_env`/`command`: password vars removed, 14-byte and
  wrong-prefix names kept.
* `Waits`: 10-second deadline, queue order, first-line notice text, tell
  flags/message with N=2, FIFO drain only when armed, immediate-hand branch.
* `never_given` concatenation order; `shared_symbol` exact warning.
* `Ended::notice` exact text/levels for all seven variants.
* `read_lines`: order, CRLF stripping, empty lines, lossy UTF-8, EOF.
* `keep_errors`: last-20 ring and order.
* `build_command`: exact recovered argv, env pairs, cwd, port 65535.
* end-to-end `show`/`run` with a fake Ruby shell script: stdout lines in
  order, stdin round-trip, stderr ring, exit status; missing Ruby -> error.
