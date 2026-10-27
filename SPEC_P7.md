# Task: Phase 7, wire the remaining `hydra` subcommands (mock servers only)

Read PLAN.md, docs/LICH_WIRE.md, crates/hydra/NOTES.md and the previous phases' reports in crates/*/NOTES.md. Always cargo --offline.
No ssh, no other hosts. NEVER contact play.net or any real game server: tests use the local mock eaccess/game servers from Phase 6.
PRIVACY: public repo; synthetic data only.

Still unwired after Phase 6: interactive stdin typing in `hydra play`, `hydra serve` (login + game socket + two-way relay,
`--detach`, `--in <dir>`, `--web <port>`), `hydra import`, `hydra lookout seed`. Behaviour comes from the binary: find each
subcommand's handler via symbols_demangled.txt and disassembly (usage text and messages are in `strings`), and from the
already-recovered crates (hydra_session, hydra_web, hydra_import, hydra_lookout).

You are the parent. Run reasonix subagents ONE AT A TIME (they all touch crates/hydra), each in its own worktree
(`git worktree add .work/p7-<n> -b p7/<n>`), merging each into main before the next:
1. play stdin: lines typed on stdin go to the game (or to Lich when Lich takes typing, per docs/LICH_WIRE.md section 3), with the
   `.`-prefixed Hydra commands handled locally (`.lich ...`, `;status` as recovered). Tests with the mocks.
2. serve: login, game socket, two-way relay with Lich, `--in <dir>` input directory, `--detach`, `--web <port>` using hydra_web.
   Tests: serve a mock session, send input through the `--in` directory, fetch the web endpoint on 127.0.0.1.
3. import and lookout seed: wire the existing hydra_import and hydra_lookout crates to the subcommands with the original's messages.
Stop starting new work after 00:45 UTC. Final: cargo test --workspace --offline, git push origin main, and a report of what is wired,
what is not, and how each item was verified (mock vs real). Acceptance checks are questions, not targets.
