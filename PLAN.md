# Hydra reconstruction: plan

State (2026-10-27, after Phase 7): the crates are reconnected to the real dependencies (tokio/serde/egui/wgpu,
plus rustls for the login), the Lich keyed accept is the observed one, and `hydra login`/`play`/`serve` run against
local mock eaccess/game servers (`cargo test --workspace --offline`: 2800 tests green). `play` reads stdin
(bare lines to the game or Lich, `.lich`/`;status` local, Ctrl-C quits); `serve` adds login + game socket +
two-way relay, `--in <dir>` (chdir + input spool), `--detach` and `--web <port>`; `import` and `lookout seed`
are wired to `hydra_import`/`hydra_ledger` and `hydra_session::lookout_seed`. Still not tested against a real
account or game server. Local research only.
Reference data: ~/Code/hydra-inspect/{hydra, symbols_demangled.txt, crate_versions.txt}.

## Phase 1: reconnect the real dependencies (DeepSeek loop, off-peak)
Goal: replace each std stand-in with the crate the original used, at the version recorded in crate_versions.txt.
Pins live in the root Cargo.toml [workspace.dependencies] (all pre-fetched: use cargo --offline). Crates use
x = { workspace = true }. tokio 1.53.1, serde 1.0.229, toml 1.1.6 / toml_edit 0.25.15, tungstenite 0.29, reqwest 0.12.28,
hyper 1.11.1, rusqlite 0.37 (bundled), jiff 0.2.37, zip 9, flate2 1.1.10, image 0.25.10, winit 0.30.13, wgpu 29.0.4.
The original had no sha2 crate (gs_base keeps its own SHA-256).
Rules: public API of each crate stays compatible with its siblings; every existing test keeps passing; each departure that
is removed gets struck from that crate NOTES.md. Layers (run in order):
  L1 gs_base, hydra_protocol, hydra_lich, hydra_net, hydra_session
  L2 hydra_model, hydra_table, hydra_exp, hydra_ledger, hydra_rewards
  L3a hydra_web, hydra_travel, hydra_map, hydra_combat_log, hydra_cast
  L3b hydra_lookout, hydra_update, hydra_survey, hydra_import, hydra_host
Gate per layer: cargo check/test --workspace --offline green, main pushed.

## Phase 2: a runnable headless binary (loop)
New crate hydra (bin): startup flow reconstructed from the binary (find main and its callees), session + lich relay + protocol
parser wired together, no GUI. Gate: starts, reads a recorded GemStone XML stream from a file, prints parsed frames.

## Phase 3: ground truth (needs the user + a live character)
Capture real Hydra<->Lich loopback traffic to settle the keyed-accept handshake and token framing; differential-test our
parser against the original on recorded streams (logs from the Tower2 archive). Not for the loop: needs the user.

## Phase 4: GUI (done 2026-10-27, off-peak loop)
egui/eframe/egui-wgpu/epaint are pinned by git (`emilk/egui` rev 3da022d, the binary's own rev — see
docs/EGUI.md) and all five GUI crates now draw through them, headless-testable:
  * gs_render: the real `egui_wgpu` paint callback (`FieldCallback` + `CallbackTrait`, `paint_view`,
    `Context::write` state, `Painter::image` for puppet figures);
  * gs_puppet: the recovered splat/limb/shadow geometry emitted as `epaint::Mesh`/`Shape` (the binary has
    no egui call edge from this crate, so there is no widget code to restore);
  * hydra_ui: `PlayView`/`Story`/`Bar`/frame painting over the recovered tokens and layout maths;
  * hydra_board: the two egui callees (`Target::named` via `Ui::available_width`, the `TextureHandle` cache)
    plus chip/hotbar/wheel painting and the `Windows` trait;
  * hydra_gui: the eframe `App` (`Hub`, `Launch`, `build`, `run`), `Bar` as `egui::Widget`, stack/vessel/
    titlebar/drawer painting, real `egui::Event` key capture.
`hydra gui` builds and opens the hub window (`"Hydra"`, 660x520, wgpu); every test is a headless
`egui::Context` frame, never a window. What is still missing before the window is a game client is the
session side: `hydra_net`/`hydra_table` login, the Lich keyed accept (Phase 3) and `hydra::gui::run`'s
single-instance/session plumbing.


## Phase 5: improvements
Backlog is the user to define; start by listing the pain points in Hydra, then one issue/spec per change.

## Phase 6: real game login (done)

The login is real and `hydra` is a game client for the first time, though only
ever against loopback mocks — nothing here has been run against `eaccess.play.net`
or a live game.
  * `hydra_mocks` (test-only): a real-`rustls` scripted eaccess server with a
    certificate generated at test time, and a game server that reads the two-line
    handshake and writes a synthetic XML stream. Both on `127.0.0.1:0`.
  * `hydra_net::eaccess::handshake`: the eight stages (`cert_pin`, `k_response`,
    `a_response`, `m_response`, `f_response`, `g_response`, `c_response`,
    `l_response`), each named as the original printed it, over a synchronous
    `rustls` client that sends **no SNI**. Trust is trust-on-first-use: the first
    certificate is recorded in `<data dir>/simu.pem` and a *changed* one is
    refused before a single byte of the login is sent. The password is keyed with
    the server's key (`hash_password`, short-key refusal kept) and no key, hash
    or holder name is ever printed.
  * `hydra_net::eaccess::game`: the game socket — the launch `KEY` then
    `/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML`, then the XML stream.
  * `hydra login <char> [--account A] [--game G]` streams the raw XML on stdout;
    `hydra play` keeps Phase 3's Lich gating (off / no folder / no `lich.rbw` / no
    Ruby are reported with no network at all), then logs in, starts Lich, says
    the two recovered start lines, opens the game and relays both ways
    (`lich::relay_game`). `serve` is still Lich-only. `HYDRA_EACCESS_HOST`/`_PORT`
    are the test seam; the data folder, roster and password come from
    `hydra_table`/`hydra_session`.
  * Tests: the full happy path, a changed-certificate refusal, a wrong password and
    the short-key refusal in `crates/hydra_net/tests/login.rs`, and the binary end
    to end (stage lines in order, XML byte for byte, pin recorded, changed
    certificate refused) in `crates/hydra/tests/login.rs` plus the updated
    `tests/lich_relay.rs`. All loopback, all bounded.


## Phase 7: the remaining subcommands (done)

All loopback only, with the Phase 6 mock eaccess/game servers; nothing here has
been run against a real account, real Lich install or live game.
  * `play` stdin (`crates/hydra/src/typing.rs`): a bare line goes to Lich's stdin
    when Lich takes typing and to the game socket otherwise; a `.`-prefixed line
    is Hydra's (`SessionHandle::lich_command`, never forwarded); `;status` prints
    one local status line; Ctrl-C sends `quit` and cancels. Tests:
    `crates/hydra/tests/play_stdin.rs` (piped stdin, lingering mock game,
    synthetic Lich that forwards typing).
  * `serve` (`crates/hydra/src/serve.rs`): the same login/Lich/game session
    without a window, two-way relay, `--in <dir>` (recovered chdir plus an input
    spool), `--detach` (`startup::detach`, `<data>/hydra.log`) and `--web <port>`
    (`hydra_web::Server::serve_blocking`, a synthetic headless page). Tests:
    `crates/hydra/tests/serve.rs` (spool input reaches the mock game; `GET /` on
    127.0.0.1 returns the page; detach really re-runs and logs).
  * `import` (`crates/hydra/src/import.rs`) records `hydra_import::sessions`'
    files through `hydra_ledger::Ledger::note_import` and prints the recovered
    summary; `lookout seed` (`crates/hydra/src/lookout.rs`) locks the data folder
    (`hydra.lock`), reads the player logs and prints
    `hydra_session::lookout_seed` summaries. Tests:
    `crates/hydra/tests/import_lookout.rs`.
