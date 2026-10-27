# Task: Phase 5a, differential parity of "hydra replay" against the original binary

Ground truth: the ORIGINAL binary ~/Code/hydra-inspect/hydra runs locally. `hydra replay FILE --summary` prints a frame-kind
histogram and `--text` prints readable text. Ours: `cargo build --offline --release -p hydra` gives target/release/hydra (same CLI).
Fixtures: `find ~/evidence161 -type f` (real GemStone IV session recordings, about 2.3 MB each). On
~/evidence161/xml/Mejora/2026-09-25_12-58-56.xml the original reports 49670 frames in about 35 kinds (CastTime, ClearContainer,
ClearDialogData, ClearStream, Compass, Component, Container, ContainerItem, ...) and ours reports only 7248 (text and prompt only).

PRIVACY: these recordings are private character data and the repo is PUBLIC. Never copy fixture bytes or real names or text into the
repo (tests, NOTES, commit messages). Tests that use them must read from $HYDRA_FIXTURES (default ~/evidence161) and skip when absent;
committed fixtures must be tiny hand-written lines.

Read PLAN.md, crates/hydra_protocol/NOTES.md and crates/hydra/NOTES.md first. Always cargo --offline. No ssh, no other hosts.

You are the parent. Run reasonix subagents ONE AT A TIME, each in its own worktree (`git worktree add .work/p5-<n> -b p5/<n>`),
merging each into main before starting the next:

1. Summary parity: make our frame classification identical to the original. Work from the original histogram names (they are the
   Frame variants; confirm against the symbols for hydra_protocol::frame), implement every variant's parse from the XML tags, and
   iterate: run both binaries on every fixture, diff the --summary output, fix the first difference, repeat until all fixtures match
   exactly. Add a script tools/diff_replay.sh that does the comparison and prints a per-kind delta table.
2. Text parity: the same loop for `--text` output (diff -u, first mismatch first) until byte-identical on all fixtures, or the
   remaining differences are catalogued in crates/hydra/PARITY.md with the cause of each.

Stop starting new work after 00:30 UTC. Final: merge, `cargo test --workspace --offline`, `git push origin main`, report the final
per-fixture match status (exact, or N differing kinds) and what is left. Acceptance checks are questions, not targets: never
special-case a fixture.
