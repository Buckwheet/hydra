# Hydra <-> Lich wire protocol (observed)

Source: one live capture, 2026-10-27, Hydra v0.1.0-beta.7 (Windows build) running `play` for a GemStone IV Prime
character, Lich 5.22.0. Method: a shim `ruby.exe` first on PATH logged Lich's argv/env, tee'd Lich's stdin/stdout/stderr and
proxied the `-g` socket. Token-like strings were masked in the log. This document contains NO captured game text; all
examples are synthetic.

## 1. Launch
Hydra (with `.lich folder <dir>` and `.lich on` set) starts, with cwd = the Lich folder and no HYDRA_* env vars:

    <ruby> -e 'trace_var(:$frontend) { |name| $frontend = "stormfront" if name == "unknown" }' \
           -e '$0 = ARGV.shift; load $0' <lich folder>\lich.rbw --pipe --stormfront --sentinel -g 127.0.0.1:<port>

`<port>` is a loopback port Hydra listens on (ephemeral). Hydra plays the role of the GAME SERVER from Lich's point of view;
the real game connection is Hydra's own. `ruby` is the first of `find_ruby`.

## 2. Handshake (two redundant channels, same two lines)
Token = 32 random bytes as 64 lowercase hex (`hydra_lich::new_token`).
Client line = `/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML`  (the Wrayth/Stormfront front-end identification).

1. Hydra writes to Lich's STDIN, once, immediately after spawn:

       <token>\n/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n

2. Lich connects to `127.0.0.1:<port>` and sends the SAME two lines as the first bytes of the socket stream:

       <token>\n
       /FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n

   This is the "keyed accept": Hydra accepts the connection only if the first line equals the token it generated.
   (It is the usual Simutronics game-server login: the first line plays the role of the session KEY.)

## 3. Streaming
After the handshake the socket is a normal GemStone IV XML stream, line oriented, `\n` / `\r\n` terminated:
* Hydra -> Lich: the game stream, as received from the game (Hydra relays it; it may also inject its own frames).
* Lich -> Hydra: commands, one per line, `\n` terminated:
  * Lich-originated commands carry the Stormfront prefix: `<c>exp\n`, `<c>locker info\n`, `<c>open #123456\n`.
  * Text the player typed and Lich passed through is the bare line: `look\n`, `quit\n`.
  * Some bare protocol lines occur (for example `_injury 2\n`).
* Typed input also reaches Lich on its STDIN as a bare line: `look\n` (this is the "Lich takes typing" path).
  Lich then forwards it on the socket as `look\n`.

## 4. Lich stdout
Lich's stdout is the front-end-bound stream (what Hydra shows when Lich is the viewer):
* A banner `--- Lich v5.22.0 is active.  Type ;help for usage info.` then Lich's own messages wrapped in
  `<output class="mono"/>` ... `<output class=""/>`.
* Game XML that Lich forwards is prefixed with the byte 0x1F (US) per chunk.
* Lines are `\r\n`; Lich's own wrapped lines use `\r\r\n`.

## 5. Shutdown
`quit` typed by the player flows through Lich to the game; the game closes, Lich exits, Hydra reports
"session ended". Hydra cancels the relay by killing the child.

## 6. What this does NOT show
Scripts that write to the game (their commands also use `<c>` / bare lines, not distinguished here), what Hydra does when the
token mismatches, whether the stdin lines are re-sent on a Lich restart, other game instances (DR, Platinum), macOS/Linux
builds, and any behaviour with Lich disabled mid-session.
