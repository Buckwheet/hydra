# Task: recover real Rust API + logic for: @CRATES@

Context: ~/Code/hydra-recon is a workspace generated from the symbol table of the Hydra GemStone IV front end
(arm64 Mach-O at ~/Code/hydra-inspect/hydra, demangled symbols in ~/Code/hydra-inspect/symbols_demangled.txt).
Stub fns are "pub fn name() { todo!() }" with the binary address in a "/// @0x..." comment (unslid virtual address;
a function ends where the next symbol begins). Already recovered on main: hydra_protocol, hydra_lich, hydra_session,
hydra_net, gs_base and others from earlier batches: read crates/*/NOTES.md and the existing type definitions before
you invent types, and depend on sibling crates by path instead of duplicating their types. Local research use only.

## Orchestration
You are the parent. For each crate in [@CRATES@], create a worktree and run ONE reasonix subagent in it, all in parallel:
  git worktree add .work/<crate> -b recon/<crate>
Wait for all, merge every branch into main of ~/Code/hydra-recon, resolve conflicts, run cargo check --workspace and
cargo test --workspace, fix integration breakage, then run git push origin main and report.

## Per-crate job (give each subagent this text)
1. For each stub fn in your crate, disassemble it (otool -tV / objdump -d --disassemble-symbols, strings,
   otool -s __TEXT __cstring) and recover parameter types, return type and behaviour.
2. Replace each todo!() with a working implementation. Define structs/enums with the fields you can establish. Keep a clearly
   marked "// UNKNOWN:" comment where a detail is not recoverable, with the safest behaviour; no invented behaviour.
3. Dependencies: the build has no network. Use std only; where the original used tokio, serde, toml_edit, egui, wgpu, etc.,
   write the synchronous std equivalent (for egui/wgpu drawing code: keep the model/state/layout logic and leave a
   documented trait or no-op seam for the draw calls) and record each departure in crates/<crate>/NOTES.md. Path
   dependencies on sibling workspace crates are allowed (add them to the crate Cargo.toml).
4. Add unit tests that assert real values. Prefer evidence from the binary (constants, strings, executed code paths).
5. Gate: cargo check and cargo test pass in the worktree before you finish. Leave all other crates untouched.
   Acceptance checks are questions, not targets: report conflicts honestly instead of working around them.
6. Commit in the worktree branch. Your deliverables are only files under your crate directory (plus NOTES.md). Work only
   inside your worktree: no ssh, no network, no other hosts.
7. Finish with a short report: functions fully recovered, partly recovered, unknown.
