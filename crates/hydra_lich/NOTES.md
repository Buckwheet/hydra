# `hydra_lich` — reverse-engineering notes

Source: arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra`, symbols at
`symbols_demangled.txt`; addresses below are unslid VAs.  The original linked
`tokio`, `getrandom`, `tokio_util::sync::CancellationToken` and
`hydra_session::lich::LichDoor`.

**Phase 1 status:** `tokio` (1.53.1) and `tokio_util::sync::CancellationToken`
(0.7.19) are reconnected for real; see §0.  Still stand-ins: `getrandom` and
`hydra_session::lich::LichDoor` (§4).

**Phase 3 status:** the Lich handshake is no longer guessed.  `docs/LICH_WIRE.md`
is the ground truth (one live capture of Hydra v0.1.0-beta.7 driving Lich
5.22.0), and §2/§3.2/§3.3 here record what it resolved and what it left open.
The wire constants, the two-line stdin handshake, the **server-side** keyed
accept, the `<c>`/bare command framing and the 0x1F stdout chunk prefix live in
`crate::wire`; `relay::start_keyed` runs the whole sequence.

Legend:
* **ESTABLISHED** — directly visible in the disassembly/strings.
* **OBSERVED** — captured from a live Hydra + Lich session
  (`docs/LICH_WIRE.md`); overrides an earlier guess.
* **INFERRED** — follows from the code shape or from the surrounding
  `hydra_session` API, but the original type/name is not in our crate.
* **UNKNOWN** — not recoverable from this binary (and not in the capture);
  safest behaviour chosen.

## 0. Phase 1 reconnection

Pins come from the root `Cargo.toml` `[workspace.dependencies]`; this crate
declares `x = { workspace = true }` and builds with `cargo --offline`.

| original | reconnected crate | pin | replaced | where |
| --- | --- | --- | --- | --- |
| `tokio::sync::mpsc::channel(64)` (stdout lines) | `tokio` | `=1.53.1` (`full`) | `std::sync::mpsc::sync_channel(64)` | `relay::read_lines_async`, `relay::bridge_to_std` |
| `tokio::io::BufReader` + `read_until` | `tokio` | `=1.53.1` | `std::io::BufReader` + `BufRead::read_until` | `relay::read_lines_async` |
| `tokio::sync::Mutex` (stderr ring, `Waits`) | `tokio` | `=1.53.1` | `std::sync::Mutex` | `relay::keep_errors_async`, `RelayAsync::waits` |
| `tokio::sync::watch::Receiver<bool>` (+0x160) | `tokio` | `=1.53.1` | plain `hand_held` bool | `RelayAsync::hand` |
| `tokio::process::Command` / `Child` | `tokio` | `=1.53.1` | `std::process::Command` / `Child` | `child::command_async`, `relay::build_command_async`, `relay::spawn_async`, `RelayAsync` |
| `tokio::select!` relay loop | `tokio` | `=1.53.1` | poll loop over a std channel | `RelayAsync::pump` |
| `tokio_util::sync::CancellationToken` | `tokio-util` | `=0.7.19` | `Relay::stop` / `Drop` kill flag | `Relay::stop`/`pump`/`Drop`, `RelayAsync::stop`, `relay::run_until` |
| `tokio::runtime` | `tokio` | `=1.53.1` | none (the binary's `main` owned it) | `crate::runtime` |

Two layers, one behaviour:

* **tokio-native (new)**: `read_lines_async`, `keep_errors_async`,
  `build_command_async`, `spawn_async`, `show_async`, `run_async`,
  `run_until`, `RelayAsync`, `Step`.
* **synchronous facade (kept, unchanged signatures)**: `read_lines`,
  `keep_errors`, `build_command`, `spawn`, `show`, `run`, `Relay`.  These now
  run the same tokio code behind a shared multi-thread runtime
  (`crate::runtime::{block_on, spawn}`); the only std hop that remains is the
  final `std::sync::mpsc` receiver the old API hands out, fed by one bridge
  thread and bounded at 64 so backpressure is unchanged.

`tokio-util` needs no extra feature flags at 0.7.19: `tokio_util::sync`
(and therefore `CancellationToken`) is compiled unconditionally.  No other
feature flags were added; `tokio` gets the workspace's `full`.

Not applicable here: this crate has no serde/TOML surface at all (the
recovered `hydra_lich` range contains no manifest, settings or JSON code), so
Phase 1 added no serde/toml round-trip tests.

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
  EOF.  **Reconnected** (`relay::read_lines_async`): real
  `tokio::io::BufReader` (its default capacity *is* 8192) over an
  `AsyncRead`, real `tokio::sync::mpsc::channel(64)`, real spawned task.  The
  binary strips the delimiter (`Lines`-style); our reader strips `\n` and a
  preceding `\r`, replaces invalid UTF-8 with U+FFFD, and disconnects the
  receiver at EOF.  `AsyncBufReadExt::lines` is deliberately *not* used:
  `lines()` rejects invalid UTF-8 with `InvalidData`, while the binary is
  lossy — so the split is `read_until(b'\n')` plus `from_utf8_lossy`.
* `keep_errors`: allocates `Arc<Mutex<VecDeque<String>>>` itself, reads
  `ChildStderr` lines, and **keeps the last 20** (`cmp x8, #0x14`; pop the
  front while full, then push back).  Returns `(Arc, JoinHandle)`.
  **Reconnected** (`relay::keep_errors_async`): `Arc<tokio::sync::Mutex<…>>`
  plus a tokio task over the same line reader.
* Synchronous facade: `read_lines` / `keep_errors` keep their old signatures.
  They adapt the blocking `std::io::BufRead` with `relay::BlockingReader`
  (a reader thread plus a 2-chunk bounded tokio channel, so blocking I/O never
  lands on a runtime worker) and hand the result back through
  `bridge_to_std` (one thread, bounded at 64).  The recovered ring rule lives
  in one helper, `push_kept`, shared by both ring flavours.


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
* Lich also speaks a local TCP protocol.  **Observed**
  (`docs/LICH_WIRE.md` §1-2): Hydra **listens** on `127.0.0.1:<ephemeral>`
  and `-g 127.0.0.1:<port>` tells Lich to connect back; Hydra plays the game
  server for Lich.  The 64-char `new_token` value is the key for that
  handshake.  The old note here had the direction backwards (Hydra
  connecting to a Lich listener) — that was a guess from the `--pipe`
  spelling and is wrong.

  Handshake, two redundant channels carrying the same two lines
  (`crate::wire`):

  ```text
  Hydra -> Lich stdin, once, right after the spawn:
      <token>\n
      /FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n
  Lich -> Hydra, first two lines of the accepted socket:
      <token>\n                                      (the key)
      /FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n  (client identification)
  ```

  [`crate::wire::accept_keyed`] is the server side: first line must equal the
  token, second line must be the client-identification line, anything else is
  an error and the stream is dropped.  [`crate::relay::start_keyed`] runs the
  whole sequence (bind, spawn, stdin handshake, accept with the recovered
  60-second deadline) and returns the relay plus the accepted socket.

* Streaming after the handshake (`docs/LICH_WIRE.md` §3-4):
  * Lich -> Hydra: newline-delimited commands.  Lich-originated commands carry
    the Stormfront `<c>` prefix (`<c>exp`, `<c>locker info`, `<c>open #123456`);
    text the player typed and Lich forwarded is the bare line (`look`); some
    bare protocol lines exist (`_injury 2`).  [`crate::wire::LichFrame`] is
    the typed parser and [`crate::wire::command_frame`] builds `<c>` frames.
  * Hydra -> Lich: the game stream on the socket; typed input on stdin as a
    bare line (the existing `Relay::send` path).
  * Lich stdout: game XML it forwards is prefixed **per chunk** with the byte
    0x1F; Lich's own lines (banner, `<output class="mono"/>…`) are not.
    [`read_lines`]/[`read_lines_async`] strip that byte so it never reaches a
    protocol parser, [`read_lich_out`]/[`read_lich_out_async`] yield
    [`crate::wire::LichOut`] when the game/Lich distinction matters.

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

Our `run`/`show` implement the same *shape* (spawn → wire readers → pump lines
until exit → map to `Ended`).  Phase 1 gave that shape two forms:

* `RelayAsync::pump` is the real `tokio::select!`, with the three arms this
  crate owns: the bounded stdout receiver, `Child::wait` and the
  `CancellationToken` (`relay::run_async` / `relay::run_until`).
* The synchronous `Relay::pump` polls the bridged channel with the same order
  of checks and consults its `CancellationToken` first.

The other recovered arms (door commands, the hand-held `watch`, the `Waits`
deadline, `fell_behind`) still live outside this crate: they need
`hydra_session::lich::LichDoor`'s command stream, which is not a dependency
here (see §4.2).  The keyed-accept race is now implemented directly in
[`crate::wire::accept_keyed_listener`] / [`crate::relay::start_keyed`] with the
recovered 60-second deadline (see §3.2); it no longer needs the door.
**UNKNOWN:** exact cancellation ordering between the token and the child kill
(the capture shows the child killed on teardown, not the arm order).


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

### 3.2 Token flow — OBSERVED (`docs/LICH_WIRE.md` §2)

**ESTABLISHED (binary):** `new_token` is called exactly twice:

* `hydra::instance::claim` (`0x1001145ec`) — a 64-char token for the local
  Hydra instance/claim protocol;
* the Lich relay loop (`0x10112ee04`) — the token is stored as a `String`
  at `+0x2f8` in the relay state, then handed to the `accept_keyed` future
  together with a 60-second deadline.

**OBSERVED (capture), which replaces the earlier guess:** Hydra listens;
Lich connects.  The token is a key *Lich* presents, on two channels:

1. Hydra writes `<token>\n<client line>\n` to Lich's **stdin** once, right
   after the spawn.
2. Lich connects to `127.0.0.1:<port>` and sends the same two lines as the
   first bytes of the socket stream.

The earlier note in this file claimed Lich "listens on `--pipe` and Hydra
connects and sends the token" with an unknown encoding (newline/JSON/length
prefix).  Both halves of that were wrong: the capture shows no Lich listener at
all, `-g <host:port>` is Lich's *game-server* address (Hydra is the server), and
the encoding is plain `\n`-terminated ASCII.  `--pipe` is what makes Lich speak
a socket rather than a terminal.

Code: `crate::wire::{handshake_bytes, write_lich_handshake, accept_keyed,
accept_keyed_listener, keyed_listener, listen_loopback}` and
`crate::relay::{start_keyed, start_keyed_minted, KeyedRelay}`.  The old
sending helper survives as `hydra_session::lich::client_handshake` (Lich's
half, for fake Lich/tests) — Hydra itself accepts.

### 3.3 Resolved by the capture, and what is still unknown

Resolved (guesses removed from this file and from the source):

* the direction of the local TCP channel: Hydra listens, Lich connects;
* the handshake encoding: two `\n`-terminated lines, no CR, on *both* the
  stdin channel and the socket;
* the client-identification literal `/FE:WRAYTH /VERSION:1.0.1.28
  /P:WIN_UNKNOWN /XML` (the same on both channels);
* the `<c>` prefix on Lich-originated commands versus bare forwarded lines
  and bare protocol lines;
* the 0x1F per-chunk prefix on game XML in Lich's stdout, and that Lich's
  own wrapped lines (`<output class="mono"/>…`, `\r\r\n`) are unprefixed;
* the recovered 60-second accept deadline is the keyed-accept race.

Still **UNKNOWN** (the single live capture cannot answer these):

* whether Hydra re-sends the two stdin lines when Lich restarts;
* what Hydra says to the player on a token mismatch (the message is not in
  the capture; `accept_keyed` returns `PermissionDenied` and the relay kills
  the child, and no notice text is invented here);
* non-Windows builds (the captured Hydra was the Windows build; Ruby
  discovery, the `C:\Ruby4Lich5` scan and path separators differ);
* other game instances (DragonRealms, Platinum) — only one GemStone IV Prime
  session was captured;
* what Hydra does when Lich is disabled mid-session;
* whether scripts' own commands are also `<c>`-prefixed (the capture shows
  both `<c>` commands and bare typed lines, but does not identify which of
  the `<c>` ones came from a script);
* the exact `Ended` branch Hydra uses for a mismatch, and the cancellation
  ordering on teardown.

---

## 4. Departures still in place after Phase 1

The `tokio` / `tokio_util` rows are **gone** (reconnected, §0).  What remains:

| original | stand-in still used | exact reason |
| --- | --- | --- |
| `getrandom` | `/dev/urandom` + documented SplitMix64 fallback (`child::new_token`) | **not in the workspace pins** (root `Cargo.toml` `[workspace.dependencies]` has no `getrandom`), and Phase 1 may not add a version of its own.  `getrandom::Error` also cannot be named, hence the crate-local `child::TokenError`.  Behaviour is unchanged: 32 bytes -> 64 lowercase hex characters; the fallback is marked `// UNKNOWN:` because the binary has none. |
| `hydra_session::lich::LichDoor` | local `relay::Relay` / `Notice` / `Typed` / `Waits` | see §4.2 — a sibling *reconstruction*, not a pinned external crate; API shape and merge risk, not a hard blocker. |
| `hydra_protocol` frame parsing | caller consumes `String` lines (`Relay::output` / `RelayAsync::lines`) | unchanged, and out of scope for this layer: `hydra_protocol` is another workspace crate whose Phase 1 branch is separate; the recovered `hydra_lich` code never parses frames itself (`show` only forwards lines to the door). |
| `tokio::sync::RwLock` (the shared session flag read in `Waits::new`) | `Waits` keeps its own `hand_held` flag; `RelayAsync::hand` is the real `watch::Receiver<bool>` | the `RwLock` in the binary belongs to `hydra_session`'s shared slot (`shared+0x150`), not to this crate; without the door there is no shared value to lock. |

Everything marked `// UNKNOWN:` in the source expands the points above.

### 4.1 What "reconnected for real" means here

`tokio 1.53.1` and `tokio-util 0.7.19` are real dependencies (workspace pins,
`cargo --offline`), used through the primitive types the binary used: bounded
`mpsc`, `BufReader`, `Mutex`, `watch`, `process`, `select!`,
`CancellationToken`, and a runtime.  The synchronous API is a facade over
them, not a parallel std implementation — the only std pieces left on that
path are the blocking-reader adapter (`BlockingReader`, needed because the
public `read_lines`/`keep_errors` accept `std::io::BufRead`) and the final
`std::sync::mpsc::Receiver` the old signature returns.

### 4.2 Why `hydra_session::lich::LichDoor` is still a local equivalent

Checked against the current snapshot (`crates/hydra_session/src/lich.rs`,
`NOTES.md` §3 of that crate):

* **No cycle exists**, so a `path` dependency is *technically* possible:
  `hydra_session` does not depend on `hydra_lich` (nothing in the workspace
  does; this crate has no consumers).
* **The API does not fit without changing our public surface.**
  `hydra_session`'s `Notice` is `{ kind: Kind /* enum */, level: u8, lines,
  quiet: bool }` built by `Notice::line(level, text)`; ours is `{ kind: u32,
  lines, logged, level }` built by `Notice::line(kind, text)` and used exactly
  that way by the existing tests and by `Ended::notice`
  (`Notice::line(1, …).with_level(2)`).  Adopting the door's `Notice` would
  change `Relay`, `Typed`, `Waits::typed/tell`, `shared_symbol` and
  `Ended::notice` — i.e. the public API this task must keep — and it would
  move the recovered level/kind numbering into another crate's guesswork.
* **The door in the snapshot is itself a reconstruction, not a pinned crate.**
  Its `claim()` returns `Claim { lines: std::sync::mpsc::Sender<String>, typed:
  std::sync::mpsc::Receiver<String>, wire }`, i.e. *std* channels this crate
  would have to bridge back into tokio, and its `lich` module is being
  rewritten on the parallel `p1/hydra_session` branch.  A path dependency
  would couple the two Phase-1 branches (both must land together or neither
  builds) and pull `hydra_session`'s growing dependency graph
  (`hydra_model`/`hydra_net`/`hydra_protocol`, tokio, serde, …) into the relay
  crate for no behaviour we can test today.
* **Consequence:** the local equivalents stay, and the parent decides the
  integration.  For that merge, the mapping is mechanical:
  * `relay::Notice` -> `hydra_session::notice::Notice::{line,lines}` with
    `kind: u32` folded onto `Kind::Text` and `level` onto the same `0..=2`
    scale (`logged` -> `quiet` inverted);
  * `Relay::send` / `RelayAsync::send` -> `SessionHandle::to_lich` /
    `hand_to_lich` (the door owns the child's stdin);
  * `RelayAsync::hand` (`watch::Receiver<bool>`) -> `LichDoor::claim`'s typed
    tap plus `SessionHandle::lich_takes_typing`;
  * `Ended` -> `LichDoor::say`, and `LichDoor::fell_behind` -> `Ended::FellBehind`;
  * `hydra_session::lich::{accept_keyed, connect_keyed}` already exist there
    for the token handshake this crate mints with `new_token`.


## 5. Tests

`cargo test -p hydra_lich` covers:

* `is_password`: positive/negative/mixed case, 14-byte non-match, suffix
  ignored, UTF-8 boundary at byte 15.
* `new_token`: exactly 64 lowercase hex chars, two calls differ, known hex
  vectors.
* `find_ruby`: first PATH hit, PATH-before-version-dir, numeric version
  ordering (`3.2.10 > 3.2.9`), missing root.
* `filter_password_env`/`command`: password vars removed, 14-byte and
  wrong-prefix names kept — for the `std` **and** the `tokio::process`
  builders.
* `Waits`: 10-second deadline, queue order, first-line notice text, tell
  flags/message with N=2, FIFO drain only when armed, immediate-hand branch.
* `never_given` concatenation order; `shared_symbol` exact warning.
* `Ended::notice` exact text/levels for all seven variants.
* `read_lines`: order, CRLF stripping, empty lines, lossy UTF-8, EOF
  (through the synchronous facade).
* `keep_errors`: last-20 ring and order (sync ring, plus the
  `tokio::sync::Mutex` ring of `keep_errors_async`).
* `build_command`: exact recovered argv, env pairs, cwd, port 65535;
  `build_command_async` must produce the identical argv/program/cwd.
* end-to-end `show`/`run` with a fake Ruby shell script: stdout lines in
  order, stdin round-trip, stderr ring, exit status; missing Ruby -> error.
* `runtime`: singleton, `block_on` from plain threads, nested `block_on`
  inside a runtime, `spawn` from a foreign runtime, panic propagation.
* **Phase 1 async layer**: `read_lines_async` order / lossy UTF-8 / EOF;
  bounded backpressure (a 200-line writer stalls while 64 lines are buffered
  and resumes when drained); `keep_errors_async` ring; `show_async` over a
  fake Ruby (queued typed line + notice, `watch` hand flag flipped by
  `release_hold`, stdin round-trip, stdout order, stderr ring, exit status);
  `run_async` exit status -> `Ended`; `run_until` cancellation ->
  `Ended::Stopped` ("Lich stopped.", level 2) with the child killed; a
  pre-cancelled token; the synchronous `Relay`'s `CancellationToken`
  cancelling `run_to_end` from another thread; `BlockingReader`.
* **Phase 3 wire protocol** (`crate::wire`): the exact handshake bytes and the
  captured client-identification literal; `accept_keyed` accepting a good
  token + client line (CRLF tolerated, the following game bytes left in the
  stream), and refusing a wrong token (`PermissionDenied`, no token in the
  message), a wrong second line, a truncated handshake and an over-long line;
  the loopback listener helper and `accept_keyed_listener` accepting a good
  handshake and closing a wrong-token socket (a real client observes EOF/RST,
  not an open connection); `<c>`/bare framing and `command_frame`; the 0x1F
  prefix recognition/stripping.  The stdout readers strip 0x1F
  (`read_lines`/`read_lines_async`) and classify
  (`read_lich_out`/`read_lich_out_async`).  `start_keyed` is exercised
  end-to-end against a fake Lich written as a **bash** script that reads the
  two stdin lines and speaks the handshake over `/dev/tcp` (no Ruby, no game
  text): good handshake streams both ways, and a wrong token or a wrong
  client line is refused with the child killed and reaped (checked with
  `kill -0`).

