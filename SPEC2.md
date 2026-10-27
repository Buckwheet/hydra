# Task: recover real Rust API + logic for hydra_session, hydra_net and gs_base

Context: ~/Code/hydra-recon is a workspace generated from the symbol table of the Hydra GemStone IV front end
(arm64 Mach-O at ~/Code/hydra-inspect/hydra, demangled symbols in ~/Code/hydra-inspect/symbols_demangled.txt).
Stub fns are "pub fn name() { todo!() }" with the binary address in a "/// @0x..." comment (unslid virtual address;
a function ends where the next symbol begins). hydra_protocol and hydra_lich are already recovered on main: read
crates/hydra_lich/NOTES.md first, it lists what hydra_session must provide (hydra_session::lich::LichDoor, the token
handshake accept_keyed, password-prompt flow). Local research use only.

## Orchestration
You are the parent. Use your reasonix tool to run THREE subagents in parallel, one per crate, each in its own git worktree:
  git worktree add .work/session -b recon/session
  git worktree add .work/net -b recon/net
  git worktree add .work/base -b recon/base
Subagent A: .work/session, crate hydra_session. Subagent B: .work/net, crate hydra_net. Subagent C: .work/base, crate gs_base.
Wait for all three, then merge the branches into main of ~/Code/hydra-recon, resolve conflicts, run
cargo check --workspace and cargo test --workspace, and report. Then run git push origin main.

## Per-crate job (give each subagent this text)
1. For each stub fn in your crate, disassemble it (otool -tV / objdump -d --disassemble-symbols, strings, otool -s __TEXT __cstring)
   and recover parameter types, return type and behaviour.
2. Replace each todo!() with a working implementation. Define structs/enums with the fields you can establish. Keep a clearly
   marked "// UNKNOWN:" comment where a detail is not recoverable, with the safest behaviour; no invented behaviour.
3. Dependencies: the build has no network. Use std only; where the original used tokio, serde, toml_edit, etc., write the
   synchronous std equivalent and record the departure in crates/<crate>/NOTES.md. Path dependencies on sibling workspace
   crates (hydra_protocol, hydra_lich, gs_base) are allowed.
4. hydra_session: session state, credentials, game connection, the lich door/handshake that hydra_lich expects.
   hydra_net: websocket/connection handling. gs_base: Elanthian calendar and time, sky lighting, data dirs, packs, body parts.
   Add unit tests that assert real values (known dates, sha256_hex vectors, parsing samples).
5. Gate: cargo check and cargo test pass in the worktree before you finish. Leave all other crates untouched.
   Acceptance checks are questions, not targets: report conflicts honestly instead of working around them.
6. Commit in the worktree branch. Your deliverables are only files under your crate directory (plus NOTES.md). Work only
   inside your worktree: no ssh, no network, no other hosts.
7. Finish with a short report: functions fully recovered, partly recovered, unknown.
