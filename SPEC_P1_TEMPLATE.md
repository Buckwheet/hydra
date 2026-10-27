# Task: reconnect real dependencies for: @CRATES@

Read PLAN.md (Phase 1) and each crate NOTES.md first. For each crate in [@CRATES@], create a worktree and run ONE reasonix
subagent in it, all in parallel:  git worktree add .work/p1-<crate> -b p1/<crate>
Per-crate job (give each subagent this text):
1. Replace the std stand-ins listed in the crate NOTES.md departures with the real crates at the pinned workspace versions
   (x = { workspace = true }). Use cargo --offline always; the registry is pre-fetched; no other network use.
2. Keep the public API compatible with sibling crates and keep every existing test passing; add tests for behaviour that
   the real crate now provides (async relay, serde round trips, TOML manifests, etc.).
3. Where the original used a crate not in the workspace pins, say so in NOTES.md and keep the std stand-in.
4. Gate: cargo check and cargo test --offline pass in the worktree. Leave other crates untouched; your deliverables are
   files under your crate directory. No ssh, no other hosts. Acceptance checks are questions, not targets: report conflicts honestly.
5. Commit on the branch; finish with a short report of what was reconnected and what was not.
Parent: wait for all, merge into main, cargo check/test --workspace --offline, fix integration breakage, git push origin main, report.
