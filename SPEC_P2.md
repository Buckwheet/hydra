# Task: Phase 2 (headless binary) plus audits

Read PLAN.md and every crates/*/NOTES.md first. You are the parent. Create one worktree per job and run ONE reasonix subagent in
each, all in parallel:  git worktree add .work/p2-<name> -b p2/<name>
Always cargo --offline (registry pre-fetched). No ssh, no other hosts. Acceptance checks are questions, not targets.

Jobs
1. name=bin: new crate crates/hydra (bin, add to workspace members). Reconstruct the startup flow from the binary (find main and
   its callees via symbols_demangled.txt and disassembly), wire hydra_session + hydra_lich + hydra_protocol together with no GUI.
   Gate: "hydra --replay FILE" reads a recorded GemStone XML stream from a file and prints parsed frames; unit/integration
   tests with a small fixture; "hydra --help" works. Document the flow in crates/hydra/NOTES.md.
2. name=audit-nav: audit hydra_travel and hydra_map. Compare the symbol list for each crate in symbols_demangled.txt (including
   private fns, closures, trait impls) against what is implemented; implement anything missing or shallow, with value-asserting
   tests. Update NOTES.md with a coverage table (symbol -> implemented/partial/unknown).
3. name=audit-core: same audit for hydra_protocol, hydra_exp, hydra_import (also decide from the binary whether each used a
   third-party crate that is in the workspace pins; reconnect it if so).
4. name=egui: research only, deliverable is docs/EGUI.md: determine which egui/eframe/epaint/egui-wgpu version the binary
   was built with (symbols, strings, type layouts, the registry or git path strings, Cargo.lock-like hints), list the egui
   API surface that hydra_ui / hydra_board / hydra_gui / gs_puppet / gs_render call, and recommend the exact dependency lines.
   Your deliverables are only docs/EGUI.md.
Parent: wait for all, merge into main, cargo check/test --workspace --offline, fix integration breakage, git push origin main, report.
