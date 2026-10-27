# Hydra reconstruction: plan

State (2026-10-27): all 25 crates recovered from the binary symbols/disassembly as std-only code (2111 tests green).
Not yet a real app: tokio/serde/egui/wgpu replaced by std stand-ins or no-op seams; keyed-accept handshake guessed.
Local research only. Reference data: ~/Code/hydra-inspect/{hydra, symbols_demangled.txt, crate_versions.txt}.

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

## Phase 4: GUI (loop, after Phase 1)
Needs the egui version used by the binary (not in the registry-path strings: probably a git dependency; identify from
symbols) plus eframe/winit wiring for hydra_ui, hydra_board, hydra_gui, gs_puppet, gs_render.

## Phase 5: improvements
Backlog is the user to define; start by listing the pain points in Hydra, then one issue/spec per change.
