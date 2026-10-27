# Task: Phase 6, the real game login and connection (against local mock servers only)

Read PLAN.md, docs/LICH_WIRE.md, crates/hydra_net/NOTES.md, crates/hydra_session/NOTES.md, crates/hydra/NOTES.md. Always cargo --offline.
No ssh, no other hosts. NEVER connect to play.net or any real game server: all tests use local mock servers on 127.0.0.1.
PRIVACY: public repo; synthetic accounts/characters/keys only.

Observed in a real login (structure only, from the original's own progress lines `[login] [stage: ...]`):
  1. TLS to eaccess.play.net:7910 with NO SNI.
  2. cert_pin: trust-on-first-use. First use records the server certificate to <data dir>/simu.pem; every later login must present the
     same certificate or is refused.
  3. k_response: a 32-byte key from the server ("used whole"; never printed).
  4. a_response: `A\t<ACCOUNT>\tKEY\t<KEY>\t<NAME>` (key redacted in logs).
  5. m_response: `M\t<code>\t<name>\t...` game list (DR, DRD, DRF, DRT, DRX, GS3, GS4D, GSF, GST, GSX); f_response: `F\tNORMAL`;
     g_response: `G\t<game>\tNORMAL\t0\t\tROOT=...\tMKTG=...`.
  6. character list / pick, then l_response: `L\tOK\tUPPORT=5535\tGAME=STORM\tGAMECODE=GS\tFULLGAMENAME=Wrayth\tGAMEFILE=WRAYTH.EXE\t`
     `GAMEHOST=<host>\tGAMEPORT=<port>\tKEY=<session key>` (key redacted in logs).
  7. Hydra then connects to GAMEHOST:GAMEPORT, sends the KEY line then the `/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML` line,
     and streams the XML game stream (the same first two lines it later hands to Lich, see docs/LICH_WIRE.md).
  The password is hashed with the 32-byte key (hydra_net::hash_password, which refuses a key shorter than the password: keep that).
  Account password comes from the keyring or the HYDRA_PASSWORD_<ACCOUNT> env var.

You are the parent. Run reasonix subagents ONE AT A TIME, each in its own worktree (`git worktree add .work/p6-<n> -b p6/<n>`), merging each
into main before the next:
1. mock eaccess: a test-only TLS server (self-signed cert generated at test time; add the needed dev-dependency from the pinned
   registry only if it is already fetched, otherwise use the existing deps; if no TLS server crate is available, report that and
   build the mock at the framing level behind a transport trait) speaking the sequence above, plus a mock game server that
   accepts KEY + /FE line and emits a short synthetic XML stream.
2. login client: implement the sequence in hydra_net/hydra_session (stages named exactly as the original prints them, TOFU pin with
   refusal on a changed certificate, key handling never printed), and wire `hydra login <char> [--account A] [--game G]` (raw stream) and
   `hydra play` to it, then to the Lich relay from Phase 3. Tests: full happy path against the mocks, changed-certificate refusal,
   wrong password, short key refusal.
Stop starting new work after 00:30 UTC. Final: cargo test --workspace --offline, git push origin main, report what runs end to end and
what remains unwired. Acceptance checks are questions, not targets.
