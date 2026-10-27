# Task: Phase 4 (GUI crates on real egui)

Read PLAN.md, docs/EGUI.md and every NOTES.md of: hydra_ui, hydra_board, hydra_gui, gs_puppet, gs_render first. egui/eframe/egui-wgpu/epaint
are pinned in the root Cargo.toml (git rev 3da022d, already fetched and already listed in those crates Cargo.toml). Always cargo --offline.
You are the parent. For each crate in [gs_render, gs_puppet, hydra_ui, hydra_board, hydra_gui] create a worktree and run ONE reasonix
subagent in it, all in parallel:  git worktree add .work/p4-<crate> -b p4/<crate>
Per-crate job (give each subagent this text):
1. Replace the no-op draw seams with real egui code: widgets, layout, painting, hotbars/windows exactly as the symbols and
   disassembly show (strings give labels and ids). Keep the model/state/logic already recovered; keep sibling APIs compatible.
2. Anything headless-testable (layout maths, state transitions) gets value-asserting tests. Draw code is tested by building
   an egui::Context and running one frame with a fixed RawInput; assert on the resulting shapes/state, never only that it did not panic.
3. gate: cargo check and cargo test --offline pass in the worktree; headless frames only, never open a window. Leave other crates untouched.
   Deliverables are files under your crate directory plus NOTES.md updates. No ssh, no other hosts. Acceptance checks are questions, not targets.
4. Commit on the branch, finish with a short report: recovered / partial / unknown.
Parent: wait for all, merge into main, cargo check/test --workspace --offline, fix integration breakage, then wire the "hydra gui"
subcommand in crates/hydra to open the hub window through eframe (wgpu) built but not launched; git push origin main; report.
