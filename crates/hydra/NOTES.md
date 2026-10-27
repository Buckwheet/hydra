# `hydra` — the binary crate, recovered and running headless

Source: the arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra` (no DWARF).
Every address below is an **unslid** virtual address and every function extent is
taken from the *complete* local symbol table — `nm -a … | rustfilt` — not from
`symbols_demangled.txt`, which only lists global symbols and therefore overruns
into the next function (`hydra::main`'s real size is 3364 bytes, not the ~24 KB a
global-only boundary suggests).

Tooling used: `/Users/cpmbpro/Code/hydra-recon/.work/recon.py` (`str`, `bytes`,
`findstr`), `objdump -d --start-address/--stop-address --no-show-raw-insn`, and a
small driver that annotates `adrp`/`add` literal loads and `bl` targets with the
demangled names from the same `nm -a` table.

Legend:

| tag | meaning |
|---|---|
| **ESTABLISHED** | read out of the instruction stream, the immediates or the literal pool. |
| **INFERRED** | fixed by the code shape and the surrounding recovered API; a source-level name/type had to be chosen. |
| **UNKNOWN** | the binary does not pin it down. The safest behaviour is implemented and marked where it is used. |

The original crate is the workspace package **`hydra`** (the `__cstring` panic
paths are `crates/hydra/src/{main,gui,play,connect,rest,update,instance}.rs`,
`0x103e21a91` … `0x103e22d78`). It is one binary crate of the Hydra workspace and
depends on `hydra_protocol`, `hydra_session`, `hydra_lich`, `hydra_net`,
`hydra_table`, `hydra_model`, `hydra_board`, `hydra_lookout`, `hydra_import`,
`hydra_update`, `hydra_gui`, `hydra_ui` and `eframe`/`tokio`.

**Phase 2 scope, updated by Phases 3, 4 and 6.** The crate is recreated with
path dependencies on `hydra_protocol`, `hydra_session` and `hydra_lich` plus the
workspace-pinned `tokio`, and — since Phase 4 — on `hydra_gui` and `eframe`
(see §5.1), and — since Phase 6 — on `hydra_net` and `hydra_table` (§8.1).
`hydra gui …` now *builds and opens* the hub window, `hydra play`/`hydra serve`
start this character's Lich when `.lich on` is set (§6.4), and `hydra login` /
`hydra play` log in for real — the `hydra_net` eaccess handshake, then the game
socket (§8.1-8.2). What is still *recognised and reported* rather than faked is
the resting/serve window and the ledger (`hydra_import`, `hydra_ledger`) — see
[Wired vs not](#wired-vs-not).

---

## 1. Module map: original files vs this reconstruction

| original file (`__cstring`) | symbols | here |
|---|---|---|
| `src/main.rs` (`0x103e21ab6`) | `hydra::main`, `hydra::started::*`, `hydra::startup::*`, `hydra::replay::*`, `hydra::render::*`, `hydra::status::*`, `hydra::lookout_seed` | `src/main.rs`, `src/started.rs`, `src/startup.rs`, `src/replay.rs`, `src/render.rs`, `src/cli.rs` |
| `src/gui.rs` (`0x103e21a91`) | `hydra::gui::{run, start_next, seat, game_runtime, stop_on_ctrl_c}` | — (the GUI boundary, §5) |
| `src/play.rs` (`0x103e21ad3`, `0x103e22b95`) | `hydra::play::{run, supervised}` | `src/session.rs` (the session half) + §6 |
| `src/connect.rs` (`0x103e21b8d`) | `hydra::connect::{Target::*, game_and_name, run, send}` | `src/login.rs` (the command line, the roster, `hydra_net`'s login and the game socket; §8.1) |
| `src/rest.rs` (`0x103e22bae`) | `hydra::rest::{doing, watch}` | — (needs `hydra_table`, the web UI) |
| `src/update.rs` (`0x103e22c3b`) | `hydra::update::{said, keep, fetch}` | — (needs `hydra_update`) |
| `src/instance.rs` (`0x103e22d78`) | `hydra::instance::{claim, answer}` | — (§5.3, it is the GUI hub's single-instance lock) |
| *(no panic paths — no `panic!`/index in it)* | `src/replay.rs`, `src/render.rs`, `src/status.rs`, `src/startup.rs`, `src/started.rs` | as above |

`src/source.rs` is new in this phase: the seam between "bytes from a recording"
and "lines from Lich" (§6). It has no symbol in the binary because the reference
build inlines both feeds straight into `Parser::push_bytes` / the relay loop.

## 2. Function table (crate-local symbols, `nm -a`)

`size` is the exact extent to the next symbol of any kind.

| symbol | address | size | status here | notes |
|---|---|---|---|---|
| `hydra::main` | `0x10021d750` | 3364 | ✅ wired | the whole chain in §3 |
| `hydra::replay::replay` | `0x100205ff4` | 5288 | ◐ wired | read + chunk + parse + report; **our** line format (§7) |
| `hydra::replay::Show::from_args` | `0x100205f3c` | 184 | ✅ | `--summary` = 2, `--text` = 1, else 0 |
| `hydra::started::begin` | `0x1001dd1f8` | 188 | ✅ | records the launch instant in a `OnceLock` |
| `hydra::started::stage` | `0x1001dd2b4` | 544 | ◐ | shape recovered; wording **UNKNOWN** |
| `hydra::started::write` | `0x1001dd4d4` | 436 | ◐ | writes + eprints; file path comes from the caller |
| `hydra::started::log_in` | `0x1001dd688` | 3076 | — | login timeline; not needed headless |
| `hydra::startup::exe` | `0x10021eec4` | 232 | ◐ | `OnceLock<String>` at `0x1044fd4a0` |
| `hydra::startup::Serve::args` | `0x10021efac` | 1120 | — | `serve` flags; `serve` is not wired |
| `hydra::startup::detach` | `0x10021f40c` | 1648 | — | relaunch-and-exit for `serve --detach` |
| `hydra::startup::EXE` (static) | `0x1044fd4a0` | — | ✅ | the cached executable path |
| `hydra::started::START` (static) | `0x1044fd460` | — | ✅ | `OnceLock<Mutex<Start>>` |
| `hydra::render::Renderer::frame` | `0x1002593ec` | 1164 | — | §7: not ported, facts recorded |
| `hydra::status::line` | `0x100112c78` | 1128 | — | `play`'s status line |
| `hydra::status::gauge` | `0x1001130e0` | 228 | — | |
| `hydra::lookout_seed` | `0x10021d088` | 1672 | — | needs `hydra_table`/`hydra_net` (§6) |
| `hydra::serve` | `0x10021e6d0` | 2036 | — | `Serve::args` → `detach` → `Target::many` → `gui::run` |
| `hydra::gui::run` | `0x1001103f4` | 9716 | — | **the eframe boundary** (§5) |
| `hydra::gui::start_next` | `0x100110208` | 492 | — | |
| `hydra::play::supervised` | `0x100258fd0` | 1052 | — | builds the supervised session |
| `hydra::rest::doing` | `0x10021e474` | 604 | — | |
| `hydra::update::said` | `0x1001131c4` | 300 | — | |
| `hydra::connect::game_and_name` | `0x1001132f0` | 388 | — | |
| `hydra::connect::Target::many` | `0x100113474` | 496 | — | `gui`/`serve` target list |
| `hydra::connect::Target::resolve` | `0x100113664` | 1436 | — | |
| `hydra::connect::Target::from_args` | `0x100113c00` | 1748 | ◐ | `--game` (6 bytes, `0x100113c84`) / `--account` (9, `0x100113cd8`) |
| `hydra::instance::claim` | `0x1001142d4` | 1412 | — | the single-instance lock (§5.3) |

Plus the closures: `main::{{closure}}`, `connect::run::{{closure}}` (three copies,
10568/18096/18096 bytes — the `login`/`play`/`import` futures),
`play::run::{{closure}}` (8136), `gui::seat::{{closure}}` (3832),
`gui::run::{{closure}}`, `rest::watch::{{closure}}`, `update::keep::{{closure}}`,
`instance::answer::{{closure}}`, `gui::stop_on_ctrl_c::{{closure}}`.

## 3. The startup chain (`hydra::main`, `0x10021d750` … `0x10021e474`)

`_main` (`0x100221d1c`, `T`, 64 bytes) is the standard Rust shim:

```text
100221d28  mov  x3, x1                     ; argv
100221d2c  sxtw x2, w0                     ; argc
100221d30  adrp x8, 0x10021d000
100221d34  add  x8, x8, #0x750             ; = hydra::main
100221d38  str  x8, [sp, #0x8]             ; the Fn() the runtime calls
100221d4c  bl   std::rt::lang_start_internal
```

so `hydra::main` is a plain `fn main` whose **return value is the exit status**
(`mov x0, x19` at `0x10021dcec`, with `w19` set to `0` at `0x10021dfa0` and to `1`
at `0x10021e040`). There is no `#[tokio::main]`: the runtime is built by hand
inside `main` (step 4).

| # | step | address | what it does |
|---|---|---|---|
| 1 | `hydra::started::begin()` | `0x10021d844` | records `Instant::now()` in `START` (`0x1044fd460`); idempotent |
| 2 | the data-folder prologue | `0x10021d790`…`0x10021dab8` | cached executable path (`EXE`, `0x1044fd4a0`); if it is `X.app/Contents/MacOS/hydra` then `HOME` must be set (`Hydra cannot find the home folder: HOME is not set`, `0x10206fc4c`), the folder `$HOME/Library/Application Support/Hydra` is created (`DirBuilder {recursive, mode 0o777}`, `0x10021e1c8`) and entered (`chdir`, `0x10021e9a0`). Outside a bundle: nothing |
| 3 | `std::env::args().skip(1)` | `0x10021dac4`/`0x10021dadc` | the token vector; `add x0, x19, #0x18` at `0x10021dd78` proves `tokens[0]` is the subcommand |
| 4 | `tokio::runtime::Builder` | `0x10021dae4`…`0x10021db54` | `new_multi_thread()` + `enable_all()` (the two driver flags `0x101` at `0x10021dae8`) + `build()`; failure → `cannot start the async runtime: {err}` (`0x103e253cf`) on stderr, exit 1 |
| 5 | dispatch on `tokens[0]` | `0x10021dc40`…`0x10021df18` | by *length* first, then bytes — see `src/cli.rs` for the full table. `replay` → `Show::from_args` (`0x10021de70`) + `replay` (`0x10021de84`); `login`/`play` → `Target::from_args` + `Runtime::block_on` (`0x10021ddc4`/`0x10021e108`); `gui` and the bare hub → `gui::run` (`0x10021dd40`/`0x10021e144`); `serve` → `hydra::serve` (`0x10021de1c`); `lookout seed` → `lookout_seed` (`0x10021dcc4`); `import` → `0x10021e0a8`; anything else → usage + exit 1 |
| 6 | `Runtime::shutdown_background` | `0x10021dfe0` | only for `login`/`play` (`0x10021df64`…`0x10021df88`); every other path lets the runtime drop |
| 7 | the message | `0x10021dffc`…`0x10021e040` | if the subcommand produced `Some(text)`, `eprint!("{text}\n")` and exit 1 |

Recovered literals on this path:

| literal | address | use |
|---|---|---|
| the 848-byte usage text | `0x10206f8ac` | step 5's fallthrough (printed to **stderr**, exit 1) |
| `Hydra cannot find the home folder: HOME is not set` | `0x10206fc4c` | step 2 |
| `Library` / `Application Support` / `Hydra` | `0x10206fc7e`/`0x10206fc85`/`0x10206fc98` | step 2's `Path::join` calls (7/19/5 bytes) |
| `"" cannot be made: ""` → `{path} cannot be made: {err}` | `0x103e25456` | step 2, `create_dir_all` failure |
| `{}: {}` → `{path}: {err}` | `0x103e25450` | step 2, `set_current_dir` failure |
| `cannot start the async runtime: {err}` | `0x103e253cf` | step 4 |
| `{}\n` | `0x103e21acf` | step 7 |
| `HOME` | `0x10206fd20` | step 2 |

### 3.1 What this crate reproduces

`src/main.rs` mirrors steps 1–7 in order, including building the tokio runtime
*before* looking at the subcommand (so a replay pays for it, exactly like the
reference build) and the `shutdown_background`-for-`login`/`play` asymmetry.
`ExitCode::from(hydra_main())` stands in for the `x0` hand-off, because Rust's
`fn main` cannot return an `i32`.

**UNKNOWN:** steps 2's exact inlined value. When `EXE` is already initialised the
prologue clones the path into the slot that `0x10021dab8` then prints and exits
on; the `Option`'s polarity could not be settled from the stream. The *observable*
behaviour (bundle → create + `chdir` into Application Support, missing `HOME` →
abort, otherwise nothing) is what `src/startup.rs` implements, behind the pure,
testable `prepare_from(exe, home)`.

## 4. `--help`, `--version`, and the CLI additions

The reference build has **no** `--help` and **no** `--version`:

* `--help` is 5 bytes; the dispatch compares 3/4/5/6/7 and 5 is only `login`, so
  it lands in the usage path (`0x10021df18`) — usage on **stderr**, exit **1**;
* there is no version subcommand at all; the version lives in
  `hydra_update::VERSION` = `0.1.0-beta.7` (`semver::Version::parse` in
  `hydra::update::keep`, `0x10009e744`; user agent `hydra/0.1.0-beta.7`).

This phase adds, and documents as additions:

* `hydra --help` / `-h` / `help` → the recovered usage text on **stdout**, exit 0;
* `hydra --version` / `-V` / `version` → `hydra 0.1.0-beta.7`, exit 0 (the package
  version in `Cargo.toml` *is* the recovered one);
* `hydra --replay <file>` as an alternative spelling of `hydra replay <file>`
  (the phase gate's required form). Both reach `Command::Replay`.

Unknown words, `replay`/`--replay`/`lookout` without their argument, and
`replay`'s bad input keep the recovered behaviour: a message on stderr, exit 1.

## 5. The GUI boundary (`hydra::gui::run`, `0x1001103f4`)

`gui::run` (9716 bytes) is the only place the window exists, and it is the
boundary this phase must not cross. Its call table contains:

* `eframe::Error` drop glue (`0x100111d98`, `0x1001125fc`) — the eframe entry
  point and the `NativeOptions`/`App` plumbing;
* `hydra_board::sessions::Sessions` (`hydra_board`), `hydra_host::Who`,
  `hydra_table::table::Resume`, `hydra::connect::Target`,
  `hydra_table::account::Login`;
* `hydra::instance::claim` twice and `hydra::instance::Asks` — the
  single-instance lock, so a second launch can hand its target list to the first
  (`__cstring` `[hydra] another launch cannot open the windows: `,
  `0x103e22d43`);
* `hydra::started::stage` four times — the launch timeline;
* `tokio::runtime::Handle::spawn` seven times and `std::thread::sleep` three
  times — the "let the window appear before logging in" pacing.

From `main`, `gui::run` is reached two ways: no arguments at all
(`0x10021dd14`, the hub) and `hydra gui …` after `connect::Target::many`
(`0x10021e144`). `hydra::serve` (`0x10021e6d0`) reaches it a third time.

**Phase 4: linked.** `Cargo.toml` now names `hydra_gui` (path) and `eframe`
(workspace pin, the same git rev `3da022d`), and the `gui` word is wired to
[`crate::gui`]:

* `crate::gui_launch(targets)` builds the window as a value —
  `hydra_gui::app::Start::new(first target, cwd)` → `hydra_gui::app::build`,
  which is `hydra_gui::app::plan` (the recovered `RunPlan`: title `"Hydra"`,
  placement `"hub"`, 660 × 520, the graphics folder, the logged-on flag) →
  `eframe::NativeOptions` with `Renderer::Wgpu`,
  `egui_wgpu::WgpuConfiguration::default()`, the 256 × 256 badge icon and the
  boxed `hydra_gui::app::Hub`. Building it touches no device and no event
  loop.
* `crate::gui(targets)` prints one line and calls
  `hydra_gui::app::Launch::run` (`eframe::run_native("Hydra", options, app)`),
  i.e. the recovered `gui::run` boundary (`0x1001103f4`) at its last step.
* The bare hub (`hydra` with no arguments, `0x10021dd40`) reaches the same
  `gui::run` in the binary but still reports instead of opening a window here;
  it is the same one-arm change when wanted.
* The unit test `the_gui_subcommand_builds_the_recovered_hub_window` asserts the
  built plan (`"Hydra"`, 660 × 520, `Renderer::Wgpu`, 256 × 256 RGBA icon)
  **without** launching: `gui_launch` is split out of `gui` for exactly that.
  `serve` remains Lich-only and `play`/`login` are wired through the Phase 6
  login, but none of them is the window's session plumbing yet (that needs the
  connection and the board's `Sessions`, not the window).

### 5.3 `hydra::instance::claim` (`0x1001142d4`) — the single-instance protocol

Not needed headless, recorded because `lookout seed` depends on it:

1. create the data folder (`DirBuilder::create`, `0x1001143a0`);
2. `<data>/hydra.lock` (`Path::join` of the literals `hydra.lock` at
   `0x10206bcb1`) opened with `OpenOptions`, then `File::try_lock`
   (`0x1001145xx`) — failure means another Hydra owns the folder;
3. a `TcpListener::bind` on `127.0.0.1:0`, whose `local_addr` port is written to
   `<data>/hydra.port` (`0x10206bca7`, `std::fs::write`);
4. `hydra_lich::new_token()` → the 64-hex-character key for that local protocol;
5. `instance::answer` (a task) serves the port.

`hydra::lookout_seed` (`0x10021d088`) calls `hydra_table::account::data_dir` and
then `instance::claim` purely to detect "Hydra is running on this data folder:
shut it down, then seed the lookout." (`0x10206f84c`).

## 6. What is wired: protocol, session, Lich

### 6.1 `hydra_protocol` — the parser, driven with bytes

`src/source.rs` + `src/replay.rs` feed the recording to
`hydra_protocol::Parser::push_bytes` in the **recovered 4 KiB pieces**
(`cmp x21, #0x1000` at `0x100206158`), then `Parser::finish`
(`0x1002061ac`). The stream is never split into lines on the replay side, so
CRLF, `\r`-only, invalid UTF-8, the `<settings …>` byte rules and the `0x40000`
over-long-line behaviour all go through the real state machine.

`Source` is the abstraction the phase gate asks for:

```text
Source ── Replay   (Vec<u8>, 0x1000-byte pieces → Chunk::Bytes)
       └─ Lich     (hydra_lich::relay::read_lines → Chunk::Line)
```

`pump()` is the single driver: `Chunk::Bytes` → `push_bytes`; `Chunk::Line` →
`push_bytes(line)` + `push_bytes("\n")` (the relay reader strips the terminator,
the byte machine needs it). Both feeds therefore run the *same* parser, and
`source::tests::the_lich_feed_reaches_the_same_parser` asserts that a fixture
produces identical frames either way.

### 6.2 `hydra_session` — the session seam

`src/session.rs` builds a `hydra_session::command::SessionHandle` over a
`claimant::Desk` (the recovered `SessionHandle::new` shape, `0x101a3bc9c`) and
exposes the four things a headless run needs:

| here | recovered | what it does |
|---|---|---|
| `Headless::new(character)` | `SessionHandle::new(desk)` `0x101a3bc9c` | builds the handle (generation cell, event log, broadcast, queue, alias table) |
| `Headless::note(level, text)` | `SessionHandle::say` `0x101a3c0ec` / `say_unlogged` `0x101a399f0` | a `notice::Notice` on both the cursor log and the broadcast channel |
| `Headless::fold(&frames)` | `SessionHandle::fold_in` `0x101a3ca04` | frame text into the log + `Event::Frames` |
| `Headless::over(reason)` | `LichDoor::over` → `Slot::close` `0x101a88408` + `SessionHandle::stop` | closes the Lich slot (both taps dropped, `session_over`), trips the cancellation token, emits `Event::Over` |
| `Headless::door()` | `SessionHandle::lich_door` `0x101a38d58` | the `LichDoor` the relay would talk to |

`session::relay(session, source)` is the composition: a `Source` (a recording or
Lich's stdout) → `hydra_protocol` parser → `SessionHandle::fold_in` in batches of
[`FOLD_BATCH`].  It is the body of `hydra::play::run` minus the connector and
minus the window, and `session::tests::a_relayed_stream_lands_in_the_session_in_order`
runs it over a real Lich line feed.

This is exactly the half of `hydra::play::run` (`0x100023b6c`) that does not need
a connection: its call table is `SessionHandle::{subscribe, snapshot}`,
`hydra_session::supervisor::SupervisedSession<hydra_table::connector::LiveConnector>`,
`hydra_lookout::save_all`, `hydra_table::{relay, commands, lich_command}`,
`hydra::render::Renderer::frame` and `hydra::status::line`. The supervisor and
the connector live in crates this phase may not depend on, so `play` is not
wired; the session half is, and is unit-tested against real frames.

### 6.3 `hydra_lich` — the relay, without spawning Lich

`Lich::listen(reader)` wraps `hydra_lich::relay::read_lines`
(`0x10167dcc0`): a `BufReader` over the child's stdout, a bounded 64-line
channel, lossy UTF-8, `\n`/`\r\n` stripped. Replay never goes near it.

**Not wired:** `relay::show`/`relay::run` (the relay loop proper) and anything
that needs a game stream to relay. Starting Lich itself *is* wired since Phase
3 — see §6.4.

### 6.4 The Lich start (`src/lich.rs`, Phase 3)

`src/lich.rs` is the half of a Lich start that only a binary can do; the `.lich`
commands themselves live in `hydra_session::lich_command` (see
`crates/hydra_session/NOTES.md` §8).

| function | what it does |
|---|---|
| `settings_path()` | `$HYDRA_LICH_SETTINGS`, else `$HYDRA_DATA_DIR/lich.json`, else `./lich.json`. The two-value form `settings_path_from(settings, data_dir)` is the test seam, so no test mutates the environment. |
| `load_settings()` / `load_from(path)` | `LichSettings::load`; a file that is not there is the defaults (`on: false`, `view: game`, no folder). |
| `plan_with(settings, ruby)` / `plan(settings)` | decide *before* any process exists: `FolderUnsaid` → `Where Lich is has not been said: lich folder <folder>.`, `ScriptMissing(dir)` → `<dir> has no lich.rbw in it`, `RubyMissing` → `Lich needs Ruby, which Lich's installer puts in C:\Ruby4Lich5; none was found there or on the PATH`. `plan()` supplies Ruby through the recovered `hydra_lich::find_ruby` (PATH, then `C:\Ruby4Lich5`). |
| `start(&plan)` | `hydra_lich::relay::start_keyed_minted(&Launch::new(ruby, folder/lich.rbw))`: bind loopback, spawn `ruby -e … -e … <folder>/lich.rbw --pipe --stormfront --sentinel -g 127.0.0.1:<port>`, write the two-line stdin handshake, accept the keyed connection (`docs/LICH_WIRE.md` §1-2). Its failure paths kill and reap the child, so a refused start leaves no orphan. |
| `announce(&plan, out)` | the two recovered start lines, on stdout: `Lich is at <dir>, for every character.` then `Lich is starting, from <dir>\lich.rbw.` — the message keeps the literal backslash of the Windows reference build while the path spawned is `<dir>/lich.rbw` on this platform (`lich_command::script_path`). |
| `relay(&mut keyed, out, session)` | print Lich's stdout lines through `hydra_lich::wire::LichOut` (so the 0x1F chunk byte is stripped) until `Relay::pump` reports the child gone, or until the session stops (`SessionHandle::stopping`, the recovered cancellation token), which kills and reaps Lich first. Returns Lich's exit status. |
| `relay_game(&mut keyed, &mut game, out, session)` | Phase 6's two-way relay: Lich's stdout through `LichOut`, game lines to Lich's accepted socket, Lich's `<c>`/bare frames back to the game, until the child or the game ends. Used by `play`; `serve` still calls the Lich-only `relay`. |
| `off_message(label)` | what `play`/`serve` say when `.lich` is off. The sentence is the Phase 3 text and still says "no game connection": that is exactly true of `serve`, and is kept verbatim for `play` rather than paraphrasing a recovered string. |
| `character_of(tokens)` | the character a `play`/`serve` command line names: the first word that is neither an option nor an option's value (`VALUE_OPTIONS` is `--account`/`--game`/`--in`/`--web`; `--detach` takes none). |

`main`'s `run` sends `Command::Play` through `login::play` (Phase 6: settings
gating, login, Lich start, announce, `relay_game`) and `Command::Serve` through
`lich_side(label, character, settings)` (the settings are an argument, so the
unit tests drive every branch without the filesystem): off → `off_message` on
stderr, exit 1; a refusal → that sentence on stderr, exit 1; otherwise start,
announce, relay, exit 0. `Command::Login` is `login::login`; the bare hub,
`import` and `lookout seed` are unchanged and still report what they need.

**Not wired, deliberately:** `serve` never writes to the accepted socket and
never reads Lich's `<c>`/bare command lines back, because it has no game
connection to relay them to (`play` does both through `relay_game`).
`--detach`/`--web`/`--in` for `serve` are still ignored.

## 7. `hydra replay` — the report

`src/replay.rs` matches the recovered stages: `fs::read` (error →
`"{file}: {io error}"`, the recovered `"{}: {}"` template at `0x103e21615`), the
stack `Parser`, the locked stdout, the 4 KiB chunk loop, `finish`, and the
recovered footer `"\n== " / " frames from " / " bytes\n"` (`0x103e24ce9`,
printed at `0x100206b1c`) — i.e. `== {frames} frames from {bytes} bytes`.

**Our line format** (the reference build's is `Display for Frame`, which our
`hydra_protocol::Frame` does not implement, plus `Renderer::frame` for `--text`):

```text
{index}\t{kind}\t{tag}\t{text}
```

* `index` — 1-based, in stream order;
* `kind` — the original `Frame` variant name (`Text`, `Prompt`, `Structural`,
  `UnknownTag`, `ClientCommand`, and every dedicated one such as `Component`,
  `DialogWidgets` or `MalformedTag`), as `hydra_protocol::Frame::kind()` spells
  it;
* `tag` — the tag name for `tag` frames, `-` otherwise;
* `text` — the frame's plain text (for `raw` frames, the raw line),
  escaped so one frame is one line: `\\`, `\t`, `\n`, `\r`, other C0/DEL →
  `\xNN`. UTF-8 is passed through untouched.

**Phase 5a changed the footer to the reference build's, byte for byte**, and it
is printed in *every* mode (the reference build does too; `--summary` only
suppresses the per-frame lines):

```text
␊== {frames} frames from {bytes} bytes␊
{count:>8}  {Variant}␊            # the non-zero kinds, sorted by variant name
== no unknown tags␊               # or:
== unknown tags␊
{count:>8}  <{tag}>␊              # sorted, one line per name, open+close counted
```

`{Variant}` is the original `Frame` variant name (`Component`, `Text`,
`DialogWidgets`, `StreamPopForced`, `MalformedTag`, …), which
`hydra_protocol::Frame::kind()` returns; the tally comes from
`Parser::unknown_tags()`.  `--summary` is byte-identical to the reference build
on every fixture in `$HYDRA_FIXTURES` (default `~/evidence161`);
`tools/diff_replay.sh` re-checks that and prints a per-kind delta table.

`--text` prints the readable text of the recording before that footer, and the
footer itself is the same one `--summary` prints (it is *not* suppressed — the
reference build prints it in every mode).  `--text` is byte-identical to the
reference build on every fixture in `$HYDRA_FIXTURES` too; `tools/diff_replay.sh
--text` re-checks it.

### 7.2 The `--text` renderer

`render::TextRender` is the readable-text side, verified byte for byte against
the reference build on all fixtures *and* on hand-written probe lines.  Only
two frame kinds print anything:

| frame | rendering |
|---|---|
| `Text` | its `Runs::plain()`, i.e. the run texts concatenated, entities decoded, control stripped — never the markup |
| `Prompt` | the prompt text |
| everything else (`Structural`, `Component`, `DialogWidgets`, `MalformedTag`, `ClientCommand`, `UnknownTag`, `Raw`, …) | nothing; `UnknownTag` does **not** print its tag, and `Raw` prints no line either |

A `Text` frame is **not** a line.  The renderer keeps one buffer and breaks it
into an output line only at these points:

* the input line ends after visible text — that is why
  `hydra_protocol::Parser::push_bytes_lines` exists and why `replay` drives
  `source::pump_groups` instead of `source::pump`;
* a `Prompt` frame arrives: the pending buffer is written first (even a
  previous prompt's), then the prompt text becomes the buffer;
* the recording ends: a pending `Text` buffer is written with its newline, a
  pending prompt is written **without** one — the footer's leading `\n`
  terminates it.

So `A<b>B</b>C` on one line prints `ABC`, a prompt's buffer survives across
input lines until the next `Text`/`Prompt`, a blank input line prints a blank
line (it is one empty `Text` frame), and a line of structural tags alone prints
nothing.

One further rule, taken from the reference build: a `Text` frame produced while
a stream is open is not printed and does not touch the buffer, which is why the
`<stream>`/`<pushStream>` dumps in a recording never show up.  Prompts are
always printed (the parser emits its `StreamPopForced` frames first).  The
renderer tracks the stream stack from the frame sequence — `StreamPush` opens,
`StreamPop` closes, `StreamPopForced` empties it — so it needs no parser state
of its own.

### 7.3 `hydra::render::Renderer::frame` (`0x1002593ec`) — recorded, not ported

Facts, for a later pass:

* it takes `(&mut self, &Frame) -> String` (`x0` = the renderer: a two-flag
  struct with a `String`, `x1` = the frame, `x8` = the sret);
* it reads a `String` at `frame + 0x20` (ptr) / `+0x28` (len) and compares it
  with `death` (`mov w10, #0x6564` / `movk #0x7461` + `ldrb #4 == 'h'`,
  `0x1002594e8`) and `group` (`mov w10, #0x7267` / `movk #0x756f` + `'p'`,
  `0x100259508`), bracketing with `[` (`#0x5b` at `0x1002595c0`);
* it pushes `'\n'` (`#0xa`) when the renderer's paragraph flag is set
  (`0x10025946c`, `0x100259550`), then appends the text;
* the exact output for every variant was **not** decoded, so `--text` here is
  plain text and the module says so.

## 8. Wired vs not

| command | state | why |
|---|---|---|
| `hydra --replay FILE` / `hydra replay FILE` | **runs** | the phase gate; parser + report + footer |
| `hydra --help`, `--version` | **runs** | additions (§4) |
| `hydra gui …` | **runs** | builds the `hydra_gui` hub window (`"Hydra"`, 660 × 520, wgpu) through eframe; §5.1 |
| `hydra` (bare hub) | recognised, exit 1 | the same `gui::run` in the binary; deliberately left reporting here (§5.1) |
| `hydra login <char> [--account A] [--game G]` | **runs** | the real eaccess login (`hydra_net::eaccess::handshake`), then the game socket: the raw XML goes to stdout; §8.1 |
| `hydra play …` with `.lich on` | **runs** | logs in, starts Lich with the observed semantics, prints the two recovered start lines, connects the game and relays both ways until either side ends; §8.1 |
| `hydra play …` with `.lich` off | recognised, exit 1 | `Lich is off for this character` — said before anything is dialled |
| `hydra serve …` with `.lich on` | **runs** (the Lich side only) | starts Lich, prints the two start lines, relays Lich's stdout until it exits; **no login, no game socket** (§8.2) |
| `hydra serve …` with `.lich` off | recognised, exit 1 | `Lich is off for this character` (§6.4) |
| `hydra import …` | recognised, exit 1 | needs `hydra_import` + `hydra_ledger` |
| `hydra lookout seed` | recognised, exit 1 | needs `hydra_table::account::data_dir` + `hydra_net::sink::config` (§5.3) |

Every still-unwired command prints
`hydra <name>: not wired up in this headless phase; it needs <crates>. See crates/hydra/NOTES.md.`
on stderr and exits 1 — the same shape as every other failure `hydra::main`
reports (`0x10021dffc`). `play`/`serve` with Lich off print
`hydra <name>: Lich is off for this character, and this headless phase has no
game connection; it needs …` instead: same channel, same status, and it names
both missing halves.

### 8.1 The login, and `play` through it (Phase 6)

`src/login.rs` is the binary's half of the login
(`hydra_net::eaccess::handshake` is the other half; `hydra_net/NOTES.md` §6):

* **The command line.** `hydra login <char>` / `hydra play <char>`, with
  `--account <account>` and `--game <code>` (the two options the recovered
  `connect::Target::from_args` at `0x100113c00` compared, 9 and 6 bytes). The
  first token that is not an option is the character; any other option is
  refused rather than ignored (`--acount` must not log in as the wrong account).
  The account defaults to `--account`, else the roster entry for the character
  (`hydra_table::roster::find`), else the character's own name; the game to
  `--game`, else that entry's `game_code`, else `GS3`.
* **The seam.** `HYDRA_EACCESS_HOST` / `HYDRA_EACCESS_PORT` (defaults
  `eaccess.play.net` / 7910). Every test points them at a loopback
  `hydra_mocks::MockEaccess`; nothing in the crate resolves a game host by
  itself.
* **The data folder** is `hydra_table::account::data_dir()` (`HYDRA_DATA_DIR`,
  else `data`), created if need be; the certificate pin is
  `<data folder>/simu.pem`.
* **The password** is `hydra_session::password::kept(account)` (the environment
  only, never printed), else `hydra_table::account::password` (the keychain,
  then the recovered `Password for account {account}: ` prompt). It reaches
  nothing but `LoginConfig::password`.
* **The stage lines**, on stderr, once per stage, exactly as the original
  printed them: `[login] [stage: cert_pin]`, `k_response`, `a_response`,
  `m_response`, `f_response`, `g_response`, `c_response`, `l_response`, then
  `[login] [stage: game_connect]` before the game socket. Nothing else about a
  login is printed: no account holder, no key, no hash, no `G` payload.
* **`hydra login`** streams the game's raw XML to stdout (`io::copy` until the
  game closes EOF) and exits 0.
* **`hydra play`** keeps the Phase 3 gating order *without any network*: Lich
  off → `lich::off_message`; no folder / no `lich.rbw` / no Ruby → the recovered
  sentence. Only then: login, `lich::start`, the two start lines, the game
  socket, and `lich::relay_game`.  (The `off_message` sentence still says the
  phase has no game connection: it is the Phase 3 text, kept verbatim because
  `serve` is exactly the command it is still true of, and because the recovered
  sentences are quoted, not paraphrased.)

### 8.2 What `play` relays, and what `serve` still does not

`lich::relay_game` is the Phase 6 relay (§6.4 is the Lich-only one):

* two reader threads — one over a `try_clone` of the game socket (lines), one
  over a `try_clone` of Lich's accepted socket (`LichFrame::parse`) — feeding
  `std::sync::mpsc` channels;
* one loop: `relay.pump()` + `drain` (Lich's stdout still reaches this process's
  stdout, 0x1F chunk byte stripped), game lines out through
  `KeyedRelay::send_game`, Lich frames into the game as `frame.text()` + `\n`
  (the `<c>` prefix stripped, a bare line unchanged);
* it stops when the child is gone, the game reaches EOF, or the session stops;
  then it gives Lich a bounded grace period for its last stdout lines, stops it,
  shuts both sockets down (which unblocks the readers) and joins them. A write
  that fails because Lich has already exited is ignored — that is what the fake
  Ruby in `tests/fixtures/lich_path/ruby` looks like.

`serve` is deliberately still Lich-only: it has no login, no game socket and no
relay, and no test gives it one.

**Not claimed:** nothing here has been run against a real eaccess server or a
real game. Every test is a loopback mock (`hydra_mocks`), and the protocol shape
comes from `SPEC_P6.md` and
`https://github.com/elanthia-online/lich-5/blob/cec0f60e/docs/eaccess-protocol-analysis.md`,
not from a capture this reconstruction made.

## 9. UNKNOWN / not recovered

1. The polarity of the inlined executable-path `Option` in `main`'s prologue
   (§3.1). The observable behaviour is implemented; the `Some`/`None` decode is
   not settled.
2. `started::stage`/`write` wording and the startup-log file name: the piece
   table was not decoded. Only the shape (elapsed seconds with a fraction,
   stderr *and* a file) is established, so `stage()`'s text is ours.
3. `Display for Frame` in the reference build: `replay`'s default per-frame line
   is whatever that impl printed. Our format is documented and tested instead.
4. `Renderer::frame`'s full output (§7.3) — `--text` itself now matches the
   reference build byte for byte; what is still unported is that other
   renderer (paragraph breaks, `[death]`/`[group]` brackets).
5. The `<prompt>`/`<settings>` and `client_region` (`<!-- ENDCLIENT -->`) rules
   inherited from `hydra_protocol`'s own UNKNOWN list — the replay feeds them
   through unchanged, so whatever the parser decides is what the report shows.
6. The `hydra_table`/`hydra_net` halves of `play`/`serve`, and the rest of
   `gui::run` beyond the window itself (`sessions`, `Who`, `Resume`, the
   single-instance claim, the login pacing): out of this crate's dependency set
   by instruction, not by evidence. The window build is wired (§5.1), and the
   Lich half of `play`/`serve` is wired (§6.4).
7. **Phase 3, the Lich start.** Which of the recovered sentences answers
   *which* moment of a start is INFERRED: `Lich is at {}, for every character.`
   and `Lich is starting, from {}` are printed in that order at a start, but the
   binary has one string table, not one call site per sentence. The same is
   true of `.lich`'s status composition — see `crates/hydra_session/NOTES.md`
   §8.3.
8. **Whether a start is retried.** The reference build's `.lich on` mid-session
   may spawn Lich immediately, and a failed start may be retried on a backoff
   (`hydra_session::supervisor` exists for the connection). This crate starts
   once, reports the refusal and returns; there is no restart loop.
9. **What Lich's own output looks like here.** The reference build shows it in
   the window (or not, per `lich view`). Headless, every stdout line goes to
   stdout; the `view` setting is recorded and mirrored on the session
   (`SessionHandle::set_lich_view`) but changes nothing about what is printed,
   because there is no other pane to hold the game's text.

## 10. Tests and gates

`cargo test -p hydra --offline` → **89 passing** (70 unit + 19 integration:
13 in `tests/cli.rs`, 6 in `tests/lich_relay.rs`).

Unit tests (in the modules):

* `cli` — every recovered subcommand word, the by-length-then-bytes
  consequences (`Replay` ≠ `replay`, `replays` is unknown), the `replay`/
  `--replay`/`lookout` argument guards, the help/version additions, the
  848-byte usage literal, the `shutdown_background`-for-`login`/`play` rule, the
  recovered version string;
* `replay` — `Show::from_args` ordering and discriminants, the empty recording,
  the exact default/summary/text reports, the `"{file}: {io error}"` error
  shape, 4 KiB chunking, and the `--text` cases: text coalescing across one
  input line, the footer after the text, a stream-hidden text frame, a prompt
  closed by the footer, a blank input line, and a line longer than a chunk
  staying one text line;
* `render` — kind words, the tab-separated line, escaping (controls, DEL, UTF-8
  kept), per-kind counts, and `TextRender`: coalescing inside a line, prompt
  flush/persistence, an unterminated prompt at the end, blank lines, "only
  `Text` and `Prompt` print", stream-hidden text and the stream stack;
* `source` — 4 KiB piece sizes, the empty recording, a line straddling a chunk
  boundary, `Lich` (through `hydra_lich::relay::read_lines`) yielding the same
  frames as `Replay`, and a final unterminated line being flushed;
* `session` — character, `Event::Frames` on fold, `Event::Said`/`Event::Over`,
  the `LichDoor` reaching `session_over`, and `session::relay` driving a
  **`Lich` source → parser → `SessionHandle`** end to end;
* `startup` — the bundle layout (including `.APP` and the five near misses), a
  missing `HOME`, the Application Support path, the cached executable;
* `started` — `begin` idempotence and `elapsed`;
* `main` — the usage path, help/version, a replay failure as a message, every
  still-unwired command's text, `play`/`serve` with Lich off and with each
  refusal (`Where Lich is has not been said…`, `… has no lich.rbw in it`), the
  settings-path seam, the built hub window (`"Hydra"`, 660 × 520,
  `Renderer::Wgpu`, 256 × 256 RGBA badge icon — built, not launched), and that
  `enable_all()` really turned the time driver on;
* `lich` (Phase 3) — the two environment seams (`HYDRA_DATA_DIR`,
  `HYDRA_LICH_SETTINGS`), a missing `lich.json` as the defaults, all three
  refusals with their exact sentences, a good plan's folder/script/ruby, the two
  start lines byte for byte, the Lich-off message, and `character_of`'s
  first-non-option-and-not-a-value rule. Nothing spawns and nothing looks at
  `PATH`: the ruby is passed in.

Integration (`env!("CARGO_BIN_EXE_hydra")`, fixtures under
`crates/hydra/tests/fixtures/`):

* `small_stream.xml` — 316 bytes, CRLF, hand-written from the documented
  syntax: a `<preset>` name, a structural `<room/>`, a text line with `&lt;`/
  `&gt;` entities, an object link, a bold span with UTF-8, an unknown
  `<questState>` tag and a `<prompt>`. Exact report asserted for all three
  modes; the `<room/>` line producing **no** frame and the unknown tag being
  preserved verbatim are asserted too, because that is what
  `hydra_protocol::Parser` does (it never emits `Frame::Unknown` from a line);
* `empty.xml` — 0 bytes → `== 0 frames from 0 bytes`, exit 0;
* `--help`/`-h`/`help`, `--version`/`-V`/`version`, an unknown word, `replay`
  with no file (usage on stderr) and a missing file (path + `No such file`,
  exit 1), the still-unwired commands' messages, and `play`/`serve` with
  `HYDRA_LICH_SETTINGS` pointed at a file that is not there (`Lich is off for
  this character`, exit 1, nothing on stdout);
* `tests/lich_relay.rs` (Phase 3) — `hydra play` and `hydra serve` run for real
  from `CARGO_BIN_EXE_hydra` against a temp `HYDRA_DATA_DIR` whose `lich.json`
  says `on: true` and points at a folder holding an empty synthetic `lich.rbw`,
  with `PATH` set to `tests/fixtures/lich_path` so `find_ruby` finds the
  synthetic `ruby` there. That script plays Lich's half of `docs/LICH_WIRE.md`
  with bash's `/dev/tcp` built-in: it reads the two stdin handshake lines,
  connects to `-g 127.0.0.1:<port>`, sends them back, writes one own line and
  one 0x1F-prefixed chunk, and exits. The tests assert both recovered start
  lines, Lich's own line, the forwarded chunk **without** its 0x1F byte, exit 0
  and an empty stderr; plus Lich off (nothing spawned), no Ruby (the recovered
  Ruby sentence), a folder without `lich.rbw`, and a settings file with no
  folder. Every run is capped at 30 s and killed on overrun.

Gates run on `p3/3` (all `--offline`):

```sh
cargo test  -p hydra --offline        # 70 + 13 + 6 passing
cargo test  -p hydra_session --offline # 217 passing
cargo check --workspace --offline     # clean
target/debug/hydra --help             # the recovered usage text, exit 0
target/debug/hydra --replay crates/hydra/tests/fixtures/small_stream.xml
```

`cargo clippy -p hydra --all-targets` reports nothing for this crate (the
warnings it prints belong to `hydra_protocol`, `hydra_session` and
`hydra_lich`, and pre-date this branch).

## 11. Integration risk for the parent merge

1. **`Cargo.toml` is the only shared file.** The change is one new member
   (`"crates/hydra"` in the root `members` list) and the new crate. No sibling
   `Cargo.toml`, no exposed API and no test was touched. `Cargo.lock` gains the
   new package entry.
2. **No dependency was added to the workspace.** `crates/hydra` uses the
   `tokio` pin and path dependencies only, so `p2/egui`'s (or anyone else's)
   edits to `[workspace.dependencies]` cannot collide with it.
3. **`hydra` is a binary, not a library.** Nothing can depend on it, so no
   merge can break a sibling; if a later phase wants `hydra::render`
   (`Renderer::frame`) shared with `hydra_gui`, that is a deliberate move, not an
   accident waiting to happen.
4. **The version string is the recovered one** (`0.1.0-beta.7`). If another job
   sets a workspace-wide `[workspace.package] version`, this crate's explicit
   version wins; that is intended (`--version` must report what the reference
   build reports).
5. **`startup::prepare()` really `chdir`s** when the executable lives in an
   `.app` bundle — faithful to `0x10021e9a0`, and the reason `--replay` takes
   relative paths relative to the data folder in a bundled build. Any integration
   test that runs the binary from inside a bundle must account for it; the
   committed integration tests run from `target/debug`, where the prologue is a
   no-op.
6. **The report format is ours.** If a later phase recovers `Display for Frame`
   or `Renderer::frame`, `render::frame_line` is the single place to change, and
   the integration test's `REPORT` constant the single place to update.
7. **`hydra_lich::relay::read_lines` is used from a binary test** (through
   `Lich::listen`), which spins the crate's shared runtime. That is already how
   `hydra_lich`'s own tests work; it adds no new requirement to that crate.
