# `hydra_mocks` — notes

Test-only crate (Phase 6, step 1): the loopback stand-ins a login client is
tested against. Nothing here is linked into a shipped binary; there is no
`hydra` bin target and no production code depends on it.

Source for the behaviour: `SPEC_P6.md` §1-6 (the observed real login),
`crates/hydra_net/src/eaccess/wire.rs` (the client-side framing and the keyed
password), `docs/LICH_WIRE.md` §2 (the two-line game handshake). No live capture
was consulted for the mock's own answers: they are synthetic by construction.

## 0. Rules this crate obeys

* **Loopback only.** Both servers bind `127.0.0.1:0`. Nothing here can reach
  `play.net` or any other host, and no test resolves a name.
* **No fixtures.** The TLS certificate and its key are generated in-process at
  test time (`src/cert.rs`); the repository holds no certificate, no key and no
  captured game text. Every account, character, password and key in
  `EaccessScript::synthetic` is invented.
* **No network, no external process.** `rustls` + `ring` only, both offline
  pinned in this crate's `Cargo.toml`.

## 1. TLS: real, with a certificate made at test time

The mock eaccess server is a real `rustls` server, so the login client's TLS
path (handshake, SNI, peer certificate) is exercised for real rather than
simulated behind a transport trait.

rustls 0.23's default provider is `aws-lc-rs`, which is **not** in the offline
registry, so the crate declares
`rustls = { version = "=0.23.46", default-features = false, features = ["ring", "std", "tls12"] }`
and `lib::install_provider()` installs the `ring` provider
(`let _ = ring::default_provider().install_default()` — an already-installed
provider is left alone, so calling it from several tests is fine).

* ESTABLISHED (in the offline registry): `rustls 0.23.46`, `ring 0.17.14`.
* NOT AVAILABLE: `rcgen` (not fetched), so the DER is built by hand.

`cert::generate()` hand-encodes a minimal v3 self-signed certificate
(`CN=eaccess.invalid`, validity `2024-01-01`..`2034-01-01`, no extensions) and
signs the `tbsCertificate` with `sha256WithRSAEncryption`. The key is a
throwaway **synthetic RSA-2048** PKCS#8 key embedded in `cert.rs` as bytes (not
a file, and never read from disk), reused for every certificate; each call
mints a fresh serial number, so two calls never return the same certificate,
which is what makes a "the pinned certificate changed" test possible: start a
second `MockEaccess`.

RSA rather than Ed25519, because of the *client*: the real login uses the
platform TLS stack (`native-tls`), and macOS Secure Transport in TLS 1.2 does
not offer `ed25519` in its `signature_algorithms`. A rustls server holding an
Ed25519 key therefore aborts the handshake with `incompatible signing key`, and
the client only sees `connection closed via error`. RSA-2048 also matches the
real eaccess server, which is static-RSA TLS 1.2. The server config is pinned to
TLS 1.2 (`builder_with_protocol_versions(&[&rustls::version::TLS12])`) so it
offers the TLS 1.2 ECDHE suite the client expects instead of negotiating
TLS 1.3.

`CN=eaccess.invalid` is deliberate — `.invalid` is reserved by RFC 2606, so the
name can never resolve even if it escaped a test.

## 2. `MockEaccess`: the scripted login

| stage | client sends | server answers |
| --- | --- | --- |
| `k_response` | `K` | `<script key>` — bare: no `K\t` prefix and no newline (the only reply that is not a newline-terminated record) |
| `a_response` | `A\t<account>\t<keyed password>` | `A\t<account>\tKEY\t<session key>\t<holder>` or `PASSWORD` |
| `m_response` | `M` | ONE record carrying every game pair in script order: `M\t<code1>\t<name1>\t<code2>\t<name2>…` (a one-game script is `M\tGS3\tGemStone IV` exactly) |
| `f_response` | `F\t<game>` | `F\t<tier>` |
| `g_response` | `G\t<game>` | `G\t<g_response>` |
| `c_response` | `C` | `C\t<n>\t<n>\t0\t1` then `\t<code>\t<name>` per character |
| `l_response` | `L\t<code>\tSTORM` | `L\tOK\tUPPORT=5535\t...\tKEY=<launch key>` or `L\tPROBLEM\t1` |

Then the server closes the TLS connection (`send_close_notify`), as the real one
does.

Framing rule (the same on both sides): **every client command is one record, and
every reply is one record, read once.** So `M` is answered with a *single* record
holding all the game pairs — never one record per game, which would leave every
game after the first unread and shift the rest of the session by a reply. The
same is true of `C`, whose character pairs already travel in one line.

The password check is the recovered `hash_password` recipe, reimplemented here
rather than imported: `out[i] = ((password[i] - 0x20) ^ key[i]) + 0x20`, refused
outright when the key is shorter than the password. `hydra_mocks` deliberately
does not depend on `hydra_net`, so the client under test cannot satisfy the mock
with the mock's own code.

What the server records:

* `sni_seen()` — `ServerConnection::server_name()` after an eager handshake. A
  real login dials `eaccess.play.net` **by address with no SNI**; the tests
  prove the client under test emits none.
* `requests()` — the client's lines in order. The `A` line is recorded as
  `A\t<account>\t<key>`: the keyed password is never kept, so a failing
  assertion cannot print key material.
* `errors()` — malformed lines (`[<stage>] ...`), TLS failures, and the refusals
  the server served (wrong password, unknown character). Empty after a clean
  run. INVARIANT: the server thread never panics; every failure is recorded and
  the session/connection ends.

Behaviour choices worth knowing:

* The listener keeps serving connections (one at a time) until `join()`/`drop`,
  so a trust-on-first-use test can log in twice against the same certificate.
  `join()` sets a stop flag and wakes the blocked `accept` with a throwaway
  connection, so it returns even when no client ever connected.
* Reads time out after 10 s (a client that connects and goes quiet cannot park
  the thread forever); a timeout is recorded as an error.
* The account comparison is ASCII case-insensitive (a real server does not care
  about the case of an account name); the password hash is compared byte for
  byte.
* `GAMECODE` in the `L` response follows the game the client named in the `G`
  stage, falling back to `LaunchInfo::code`.
* The second field of the `L` request (`STORM` in the observed protocol) is
  accepted but not checked.
* `EaccessScript::synthetic().launch.port` is `0`: a test that walks as far as
  the `L` response sets `launch.host`/`launch.port` from a `MockGame::addr()`
  first.

### UNKNOWN / departure

* The `M` response tail is unknown: SPEC_P6 records `M\t<code>\t<name>\t...` but
  not the remaining fields, so the mock sends `\t<code>\t<name>` per game and
  nothing more — all of them in the one record, in script order.
* `C\t<n>\t<max>`: the observed line does not say what `max` counts, so the mock
  sends `max == n` (the character count).
* The character list is one line with **two** fields per character
  (`\t<code>\t<name>`), as SPEC_P6 states. Note for the login client:
  `hydra_net::eaccess::wire::resolve_char_code` strides the line in threes
  (`parts[5]/[6]`, then `[8]/[9]`, …), which matches a three-field-per-character
  layout. A client that uses `resolve_char_code` against this mock will find the
  first character by name and then mis-step; scan the pairs instead, or say so
  and this mock changes. Flagged rather than guessed.

## 3. `MockGame`: the two-line handshake

Plain TCP, no TLS. Accepts one connection, reads exactly two newline-terminated
lines (the launch `KEY`, then
`/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML` = `CLIENT_IDENTIFICATION`),
records them, writes the synthetic XML stream it was started with **verbatim**
(nothing added — not even a trailing newline), flushes, half-closes, drains to
EOF and closes.

* `received()` — the two lines, in order.
* `sent()` — the lines the client sent **after** the handshake, in order. They
  are read (and recorded) by the drain loop either way; a client that types
  nothing leaves it empty.
* `start_lingering(stream)` — the same server with the half-close suppressed:
  the connection stays open after the stream, so a live session can keep writing
  to it, and `join()` (or the client's own close, or `LINGER_LIMIT` = 60 s) ends
  it.  Phase 7 added it for `crates/hydra/tests/play_stdin.rs`: `hydra play`
  ends when the game closes, so a test that types has to give it a game that
  stays open.  `start()` still half-closes, which is what every Phase 6 test
  expects.
* A client that closes before both lines are in is an `UnexpectedEof` error from
  `join()`, not a hang: the mock reads with a 10 s timeout.
* The drain after the half-close is what keeps the close orderly; a socket
  dropped with unread input can be reset, and a reset can lose the stream the
  client is about to read.  A lingering server polls that drain (500 ms
  timeouts) so `join()` can stop it before the limit.
* The lines after the handshake are recorded by `read_until` on `\n`, so a
  client that writes without newlines shows up as one long entry — nothing here
  tries to speak the game protocol.
