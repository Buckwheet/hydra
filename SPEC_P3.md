# Task: Phase 3, replace the guessed Lich handshake with the observed one

Read docs/LICH_WIRE.md first: it is the ground truth, captured from the real Hydra driving the real Lich 5.22.0. Then read
crates/hydra_lich/NOTES.md and crates/hydra_session/NOTES.md (their sections on `accept_keyed`, `connect_keyed`, the token and the
newline-delimited stand-in that was guessed). Always cargo --offline. No ssh, no other hosts.

PRIVACY: the repo is public. Use only synthetic fixtures (invented character names, ids and text). Never add real captured text.

You are the parent. Run reasonix subagents ONE AT A TIME, each in its own worktree (`git worktree add .work/p3-<n> -b p3/<n>`), merging
each into main before the next:

1. wire: in hydra_lich and hydra_session implement the real protocol from docs/LICH_WIRE.md: the exact spawn argv
   (-e trace_var ..., -e '$0 = ARGV.shift; load $0', <folder>/lich.rbw --pipe --stormfront --sentinel -g 127.0.0.1:<port>),
   writing `<token>\n/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n` to Lich's stdin, the keyed accept on the listening socket
   (first line must equal the token, second line is the client identification line, reject and close on a mismatch), the `<c>` command
   framing versus bare typed lines, and the 0x1F prefix on Lich stdout chunks. Remove the old guessed format and every
   `// UNKNOWN:` that this capture resolves; update both NOTES.md files.
2. fake lich: a test helper (a small Rust binary under crates/hydra_lich/tests or tests/bin, no Ruby needed) that behaves like Lich
   per the document: reads token + client line from stdin, connects to the `-g` port, sends the handshake, echoes `<c>` commands.
   Add loopback integration tests: good handshake streams both ways; wrong token is refused; typed `look` reaches the socket as a
   bare line; Lich stdout 0x1F chunks are recognised.
3. `hydra` binary: make `hydra play` (and `serve`) start Lich with these semantics when `.lich on` is set, mirroring the original's
   messages ("Lich is at <dir>, for every character.", "Lich is starting, from <dir>\lich.rbw."); the commands `.lich on|off|folder <dir>|view lich|game|help`
   are in hydra_session. No real network/game connection is needed in tests.

Stop starting new work after 00:30 UTC. Final: cargo test --workspace --offline, git push origin main, short report of what the capture
resolved and what remains unknown. Acceptance checks are questions, not targets.
