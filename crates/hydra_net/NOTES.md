# `hydra_net` — reverse-engineering notes

Source: arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra`, symbols at
`symbols_demangled.txt`; addresses below are unslid VAs, and a function ends
where the next symbol begins.  The original crate lives at
`crates/hydra-net/src/...` (the source paths survive in `__cstring`), and its
sibling modules that are *not* in the symbol table are named there too:
`eaccess/handshake.rs`, `eaccess/game.rs`, `live.rs`.

This crate was a **std-only** reconstruction; Phase 1 has put the real crates back.
The original linked `tokio`, `tokio-rustls`, `rustls`, `reqwest`,
`tokio-tungstenite`, `jiff`, `regex`, `base64` and `url`.  Of those, `tokio`,
`tokio-tungstenite`, `reqwest`, `jiff`, `regex` and `base64` are reconnected here
at the workspace-pinned versions (see §3); `tokio-rustls`/`rustls`, `url` and
`tracing` are *not* workspace pins, so their stand-ins stay and are documented.

Legend:

* **ESTABLISHED** — visible directly in the disassembly, the immediates or the
  literal pools (`__const` at `0x103bf9xxx`/`0x103e22xxx` held most of this
  crate's strings).
* **INFERRED** — follows from the code shape, the call sites or the literals, but
  the original type/name is not recoverable.
* **UNKNOWN** — not recoverable from this binary; the safest behaviour is
  implemented and marked with a `// UNKNOWN:` comment in the source.

---

## 1. Recovered functions

Every stub in `crates/hydra_net/src/**` now has an implementation and no
unfinished-stub macro remains anywhere in the crate.  Signatures are recovered from the argument
registers and the sret structs (`Vec`/`String` is cap/ptr/len — cap at +0, ptr at
+8, len at +16 — and enum tags sit in the first word or in the string capacity
niche, which shows as `i64::MIN`).

### 1.1 `eastern.rs` — Eastern-time civil date math (ESTABLISHED)

**Phase 1**: this module runs on `jiff` again — civil dates are
`jiff::civil::Date`, the zone is `America/New_York` from the time zone database,
and the DST rule is the database's own.  The recovered day-number arithmetic
(Hinnant, inlined in the binary) survives as the *fallback* for years outside
jiff's `-9999..=9999` range, and the recovered fixed US rule survives as the
fallback for a machine with no time zone database at all; neither runs otherwise.
The public API is unchanged: `Date` still holds three `i64`s.

The binary has Howard Hinnant's `days_from_civil` / `civil_from_days` inlined
verbatim (`0x101c5858c`, `0x101c58408`), and the DST rule is folded into
`date`/`offset` as compares against two computed boundaries.  Recovered by
executing the original functions against a range of dates
(`/tmp`-scratch ARM64 interpreter, thrown away):

| function | address | recovered signature |
| --- | --- | --- |
| `civil_from_days` | `0x101c58408` | `fn(i64) -> Date` |
| `days_from_civil` | `0x101c5858c` | `fn(&Date) -> i64` |
| `now` | `0x101c5866c` | `fn() -> i64` (unix seconds, clamped, pre-epoch → 0) |
| `date` | `0x101c586c0` | `fn(i64) -> Date` |
| `offset` | `0x101c58dc8` | `fn(i64) -> i64` (`-18000` or `-14400`) |
| `midnight` | `0x101c59098` | `fn(&Date) -> i64` |
| `week_start` | `0x101c57dc0` | `fn(&Date) -> Date` |
| `stretch_start` | `0x101c5803c` | `fn(&Date) -> Date` |
| `format` | `0x101c58d70` | `fn(&Date) -> String` |
| `parse` | `0x101c58abc` | `fn(&str) -> Option<Date>` |

Facts established by running the original:

* `Date` is three 8-byte signed words (year, month, day) at offsets 0/8/16;
  `Option<Date>` is 32 bytes with a tag in the first word.
* `offset` = `-14400` exactly when `dst_start <= t < dst_end`, where
  `dst_start` is 02:00 EST on the **second Sunday of March** (07:00 UTC,
  `n*86400 + 25200`) and `dst_end` is 02:00 EDT on the **first Sunday of
  November** (06:00 UTC, `n*86400 + 21600`).  The year is the Eastern civil year
  of the timestamp.  Verified for 2023 and 2024, around each boundary.  This is
  the post-2007 US rule and is now only the no-time-zone-database fallback: the
  tests assert that `jiff`'s `America/New_York` agrees with it for every hour of
  2007..2027, and that the database also knows what the rule cannot (January 1945
  was War Time, UTC-4, and the 1974 experiment was year-round daylight time).
* `date(t)` = `civil_from_days((t + offset(t)).div_euclid(86400))`.
* `midnight(d)` = `D*86400 - offset(D*86400 + 18000)`, so the switch days come
  out right (2024-03-10 → 05:00 UTC, 2024-11-03 → 04:00 UTC).
* `week_start(d)` is the **Sunday** on or before `d` (day numbers ≡ 3 mod 7).
* `stretch_start(d)` is `max(week_start(d), 1st of the month)` — the original
  walks backwards one day at a time, stopping on a Sunday or on day 1, so a week
  that straddles a month boundary is clipped to the 1st.
* `format` is `{:04}-{:02}-{:02}` (the `fmt::Arguments` piece table is at
  `0x103bfa729`); the rendering is now jiff's own ISO 8601 printer, which pads
  the year to four digits exactly as that literal does.
* `parse` splits on `-`, requires field widths 4/2/2 and no fourth field, parses
  each as a signed integer (`str::parse::<i64>`, so `+3`/`-1` are accepted), then
  round-trips through `jiff::civil::Date::new` so `2023-02-30` is rejected
  instead of normalised.

### 1.2 `eaccess::wire` — the login wire protocol

| function | address | recovered signature |
| --- | --- | --- |
| `character_names` | `0x101c57584` | `fn(&str) -> Vec<&str>` (`trim().split('\t')`) |
| `offered_game_codes` | `0x101c578b8` | same split, over the `M` response |
| `is_launch_ok` | `0x101c56c34` | `fn(&str) -> bool` — 4-byte compare with `0x4b4f094c` = `"L\tOK"` |
| `expect_echo` | `0x101c56a5c` | `fn(&str, u8, &'static str) -> Result<(), Refusal>` |
| `hash_password` | `0x101c572b8` | `fn(&[u8], &[u8]) -> Result<Vec<u8>, Refusal>` |
| `parse_launch` | `0x101c56c5c` | `fn(&str) -> Result<Launch<'_>, Refusal>` |
| `redact` | `0x101c57a14` | `fn(&str) -> String` |
| `redact_char_code` | `0x101c575e8` | `fn(&str) -> String` |
| `resolve_char_code` | `0x101c5775c` | `fn(&str, &str) -> Option<&str>` |
| `err` (private in the original) | `0x101c57918` | builds the `Result` error struct |

* **`hash_password`** (ESTABLISHED) is `out[i] = ((pw[i] - 0x20) ^ key[i]) + 0x20`
  — the SGE keyed hash, kept printable.  The key must be at least as long as the
  password; the refusal message is verbatim:
  `key ({n} bytes) shorter than password ({m} bytes) -- refusing to truncate, which would send a wrong password rather than fail`.
  A byte that would leave `0..=255` is refused with
  `password byte {} hashes out of range, outside 0..=255. No login`.
* **`parse_launch`** (largely ESTABLISHED): the payload is split on tabs, each
  field on `=`, and `KEY` / `GAMEHOST` / `GAMEPORT` are picked out (the two
  8-byte immediates `GAMEHOST`/`GAMEPORT` are compared directly; the port goes
  through an integer parse that accepts a leading sign).  Missing fields produce
  `no KEY in launch payload`, `no GAMEHOST in launch payload`,
  `no GAMEPORT in launch payload`.  Which third string the `Launch` struct keeps
  is INFERRED (`GAMECODE`); the struct shape — three `&str` plus a `u16` — is
  ESTABLISHED from the 0x32-byte sret.
* **`redact_char_code`** (ESTABLISHED): split on `_`; with ≥3 fields and field 1
  exactly `W`, the answer is `W_<ACCOUNT>_<last field>` (`0x103e7fd02`).
* **`resolve_char_code`** (ESTABLISHED): split on tabs, then walk fields 6, 9, 12,
  … comparing ASCII case-insensitively (the original ORs `0x20` into `A`..`Z`),
  and return the field before the match (5, 8, 11, …).
* **`redact`** (INFERRED for the mask text): the line is split and rejoined on
  tabs; when field 0 is a single `A` and field 2 is `KEY`, the fields from 2 on
  are masked as `<key>`, matching `not printed -- key material`.

### 1.3 `eaccess::pin` — certificate pinning (ESTABLISHED shapes)

`verify(pin_path: &Path, presented_der: &[u8]) -> Result<(), String>`:

* pin missing → *first use*: the presented DER is written as PEM through
  `{}.{}.tmp` + rename (`0x103e7f8d3`); a failure to record is fatal
  (`no certificate was pinned and one could not be recorded at {}: {}. Refusing to log in: without a stored pin every login would be a first use, which is no pin at all.`)
  — there is deliberately no "trust this time" path;
* pin unreadable → `the certificate pin {} exists but could not be read: {}. Refusing to log in unpinned.`;
* pin not PEM → `the certificate pin {} is not a PEM certificate ({}). Refusing to log in unpinned. Delete {} to trust the certificate the server presents on the next login.`
  with the inner reasons `no `-----BEGIN CERTIFICATE-----` line`,
  `no `-----END CERTIFICATE-----` line`, `the certificate block is empty`,
  `invalid base64: …`, `not UTF-8 text`;
* decoded DER differs → `the eaccess server's certificate CHANGED: …`
  (`0x103bf9d38` pool);
* match → `[stage: cert_pin] certificate matches the pin at {}` (logged in the
  original; the string is kept as documentation).

**Phase 1**: `base64_encode`/`base64_decode` are `base64-0.22.1`'s default engine
(`general_purpose::STANDARD`: `+/` alphabet, `=` padding) again, with the same
public signatures; the only thing on top of the engine is that whitespace is
dropped before decoding (a PEM body arrives wrapped).  `pem_encode`/`pem_decode`
wrap the engine with the PEM markers at 64 columns.

### 1.4 `eaccess::refusal` — launch refusals

A refusal is a `L\tPROBLEM<tab>N` line.  `describe_launch_refusal` redacts the
line and prefixes `launch refused ({}): {}` for a known sub-code and
`launch refused: {}` otherwise; `launch_refusal_is_fatal` is true exactly for
codes 1..=4.  The four code messages are verbatim from `__const`
(`0x103bfa1e0`, `0x103bfa145`, `0x103bf9f61`, `0x103bf9f16`), e.g. code 1 is
*"the account's access level does not permit playing this instance … DO NOT
RETRY: this is account state"* and code 3 is the long
*"the server has no configuration for the selected game … NOTHING IN THIS CLIENT
VERIFIES IT"* note.  The unknown-code text is also recovered
(`unrecognised PROBLEM sub-code. Codes 1-4 are known; a fifth means the server
grew one.`).

### 1.5 `eaccess::reject`, `eaccess::fallback`, `weblogin::failure`

* `classify_a_rejection` (ESTABLISHED needles/messages): the page is case-folded
  and searched for `REJECT`/`INVALID`/`loader.asp` →
  `the link was used or old: send GOALS again for a new one`; for
  `NORECORD`/`PASSWORD`/`home.asp` →
  `the website did not start its session: send GOALS again`.  The fall-through
  text is INFERRED.
* `carry_forward` / `web_only_error` / `from_web`: `{} (web-login fallback also
  failed: {})`, `[fallback] web login also failed: {}`, and the field-by-field
  copy of a web launch (two `String`s and a `u16`).
* `WebLoginFailure`: the original's `Display` is called from
  `web_only_error`; the literals `unexpected response from play.net at {}` and
  `could not reach play.net: {}` are kept.  UNKNOWN: the variant list (the type
  has no symbols of its own).

### 1.6 `gemstone::endpoint`, `gemstone::weblogin::{instance,http,scrape}`,
### `gemstone::shim`

* `instance_for` (ESTABLISHED): exactly three bytes, matched against `GS3`,
  `GST`, `GSF`; the static rows at `0x1044a8ca8`/`0x1044a8d00`/`0x1044a8d58` are
  four `&'static str`s each — `{"GS3","GS4","gs4","/gs4/play/home.asp"}`,
  `{"GST","GST","gs4","/gs4/play/play_test.asp"}`,
  `{"GSF","GSF","gs4","/gs4/play/playf.asp"}`.
* `Instance::error_page` = `/{}/login_error.asp`, `okay_page` =
  `/{}/play/home.asp`, both with the instance's web code (the format tables are
  at `0x103e7faa4`/`0x103e7fab9`; the leading `/` is a literal piece).
* `http::location_of` / `status_of`: the original read `Location` off a `reqwest`
  response.  **Phase 1**: `location_of_response`/`status_of_response` do exactly
  that on a real `reqwest::Response` again (`HeaderMap` lookup, empty value → no
  location); the text spellings stay for a caller that only kept the bytes.
* `scrape::*`: `classify_login_redirect` compares against the instance's error
  and ok pages and recognises `signin_needed.asp`; `is_subscription_needed`
  looks for `/play/subscription`; `find_char_code` walks `id="W_` rows and their
  `<label for="…">` names; `next_hop` follows only `https://` hosts under
  `play.net`/`simutronics.net` and otherwise refuses with
  `character selection: untrusted redirect`; `parse_launch` reads `host`, `port`
  and `key` out of a launch URL with the recovered messages
  (`launch url: no query`, `… blank host, port or key`, `… missing host, port or
  key`, `… duplicate host, port or key`, `… port is not a number`,
  `… host or port is not this instance's {}`).
* `shim`: `shim_request` = `wss://{}/shim/{}`; `shim_host` = `https://{}` with
  the path dropped; `shim_query` keeps the recovered `spcircname{}` key.
  **Phase 1**: `ws_to_io` no longer *needs* to stand in for
  `tokio-tungstenite` — `shim_over(stream, host, character)` performs the real
  handshake over a connection the caller already holds (the original's was TLS
  through `tokio-rustls`) and hands back `tokio_tungstenite::WebSocketStream`,
  and `shim_plain` dials a plaintext `ws://` shim.  A `wss://` dial needs a TLS
  feature on `tokio-tungstenite`, which is not in the pins, so it is done through
  `shim_over` with the caller's TLS stream.  `Duplex`/`ws_to_io` stay as the
  synchronous compatibility shape for the std-only siblings in this workspace.

### 1.7 `store::page`, `store::http`

* `read(page, tag)` scans `<tag` then `</tag` (literals `<{}` / `</{}`) and
  returns the trimmed inner text.
* `offers_claim` is exactly `page.contains("/Store/ClaimReward")` (the original
  forwards to a shared `contains` helper with an 18-byte needle).
* `is_claimed` / `is_waiting` fold the page case and search the needles found in
  `0x101c5a3c4` (`clai…`) and `0x101c5a590` (`next sub`, `on bonus`).
  INFERRED: the exact longer spellings.
* `token` reads `__RequestVerificationToken` and its nearest `value=`.  **Phase 1**:
  the value itself is a `regex` pattern (`^\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))`)
  anchored just past the recovered `value=` literal, so the literal searches
  around it are unchanged.
* `asks_to_sign_in` / `signed_in`: INFERRED markers (the original compares a set
  of page fragments that is re-encoded through the entity table).
* `next_available` / `next_in`: the original parses the store's "next available"
  text through inlined month-name and HTML-entity tables
  (`0x103bfa7b7`..`0x103bfa808`).  UNKNOWN: only `M/D/YYYY` (and `M-D-YYYY`) is
  recovered here; anything else yields `None`.  **Phase 1**: the run of digits
  and separators is found with a `regex` pattern that ends where the run ends, so
  a fourth field or a five-digit year is still one invalid run.
* `store::http`: `Family::of` classifies a failure message (ESTABLISHED: it
  returns a small fieldless enum; the phrase lists come from
  `the store could not be read: {}` / `the store could not be reached: {}`),
  plus the recovered URL shapes
  (`{}/Account/SignIn?returnURL=%2Fstore%2Fpurchase%2F{}`,
  `{}/Store/ClaimReward`, `{}/store/purchase/{}`) and messages
  (`{}: the store's sign-in came to nothing`, `{}: the store did not take the
  password`, `{}: the store answered the claim with `{}``).
  **Phase 1**: `fetch`/`fetch_with` are the `reqwest` request again (following
  redirects, which the sign-in flow needs), `Family::of_reqwest` classifies
  `reqwest`'s own error instead of its message, `hop_client` is the
  redirect-free client the sign-in hops used, and `fetch_blocking` is the
  synchronous bridge for the still-sync siblings.  The status line in a
  non-success answer (`the store answered {} for {}`) is INFERRED text.

### 1.8 `trainer::{page,goals,http}`

* `page::page_url` = `https://www.play.net/gs4/play/cm/{}` (ESTABLISHED).
* `quoted_var` / `number_var` / `array_var` are the three recovered regexes
  (`var\s+NAME\s*=\s*"([^"]*)"\s*;`, `var\s+NAME\s*=\s*(-?\d+)\s*;`,
  `var\s+NAME\s*=\s*new\s+Array\(([^)]*)\)`), compiled with `regex-1.13.1` per
  name (the name is `regex::escape`d).  **Phase 1**: the hand-written scanners are
  gone; the patterns are the recovered literals, which is why `var skill` still
  cannot match `var skills` — the name is followed by `\s*=`.
* `read_page` reads the skill table
  (`(?is)<b class="normS2">{}</b></td>\s*<td[^>]*><b class="normS1">([^<]+)</b>`)
  and refuses a page with no rows (`the page has no {}`).  **Phase 1**: that
  literal is the pattern now (with a capture in place of the per-skill `{}`), so
  the `</td> … <td>` bridge is required — the old hand-written scan merely looked
  past it.  Every recovered test page has the bridge.
* `answer` reads the `var start_…` values; `resolved`/`landed` accept only a
  trainer page.
* `SkillGoals` is a sorted map of skill → target rank with the recovered
  operations (`aim`, `up`, `down`, `step`, `reset`, `has`, `goal`, `targets`,
  `changed`, `cost_of_next`, `goal_fields`).  `goal_fields` emits `skill{n}` in
  map order plus `cm_spell` and the derived `phy_tp`/`mnt_tp`/`phy_left`/
  `mnt_left`/`phy_conv`/`mnt_conv`/`phy_spent`/`mnt_spent` fields (those names are
  recovered literals; the values are ESTABLISHED as zero unless something was
  trained).  UNKNOWN: `cost_of_next` — the original's per-skill cost table is a
  static array not reachable from the symbol table, so this reports the flat cost
  of one rank.
* `trainer::http`: **Phase 1** — `Trainer::get_page` and `Trainer::post_goals`
  are the recovered `reqwest` requests again (GET the page, POST the goal form
  with `application/x-www-form-urlencoded`, both carrying the session cookie as
  the explicit `Cookie:` header the recovered `cookie_header` built; no cookie
  jar, because the `cookies` feature is not pinned).  `get_page_at` /
  `post_goals_at` take another origin, which is what lets the tests drive them
  over a loopback socket; the recovered URL stays the literal in `page::page_url`.

### 1.9 `sink::{config,redactions,writer}`

`config` (ESTABLISHED env vars and format shapes):

| function | address | recovered form |
| --- | --- | --- |
| `log_dir` | `0x101c509f0` | `HYDRA_LOG_DIR` via `var_os`, else a home-relative default |
| `rotate_after_lines` | `0x101c50548` | `HYDRA_LOG_LINES`, trimmed and parsed |
| `bytes_timestamps_enabled` | `0x101c50718` | `HYDRA_LOG_TIMESTAMPS`, trimmed |
| `file_stamp` | `0x101c50398` | `{:04}-{:02}-{:02}_{:02}-{:02}-{:02}` |
| `date_dir` | `0x101c50a80` | `{:04}-{:02}-{:02}` |
| `moment` | `0x101c507f0` | `{:04}-{:02}-{:02} {:02}:{:02}` |
| `line_time` | `0x101c50c00` | `{:02}:{:02}:{:02}.{:03}` |
| `stamp_ago` | `0x101c50da0` | a moment plus the age, from a `jiff::SignedDuration` |

`Redactions` registers secrets and masks them (`<account>`, `<session key>`);
`apply` works on text (longest secret first), `apply_bytes_after` on the wire
bytes from an offset; `account`/`account_of` keep the account so the log can name
it back.  UNKNOWN: the exact mask spellings (only the *mechanism* is recovered).

`SessionSink` writes `<stem>.log` and `<stem>-000.bytes`, rotating to `-001`,
`-002`, … after `rotate_after` lines; the log opens with the recovered header
(`# Hydra session log -- PRIVATE DEVELOPMENT LOG, not automatically shareable.`
…) and a `recording gap ended: {} bytes …` line when bytes were dropped; log
lines are `{line_time} {text}`; wire records are `{line_time} {direction}: ` plus
the masked bytes; `redact_key` registers the session key and notes it in the log.

### 1.10 `behind`, `live`, `record`, `gemstone::shim`

* `behind::Behind` tracks outstanding bytes against a limit and reports
  `[hydra] {label}: the disk is too far behind; what more comes is left out
  until it catches up` once, then drops until it catches up; `Behind::settle`,
  `settle_all`, `Live::new` (a recording thread; on spawn failure
  `[hydra] {label}: no thread to write it: {`), `Shared::ended`.
* `live::configure_game_socket` sets `TCP_NODELAY` (DEPARTURE: `std` has no
  keepalive knob, so `[socket] WARNING: TCP keepalive could not be set (…)` is
  kept only as a documented helper); `LiveSource::peer_certificate_der` returns
  the DER the TLS layer handed over, or
  `could not read the server's certificate: …`.
* `record::Recorder::{inbound,outbound}` copy both directions into the sink
  before anything parses them.

---

## 2. Key wire formats (as used by this crate)

```
eaccess request/response lines, tab separated:
  K\t<key>                        key material, used whole
  A\tOK                           "a_response"
  M\t<codes...>                   offered game codes
  F\t<code>                       entitlement check
  G\t<field>=<value>              GAMEHOST / GAMEPORT / KEY / GAMECODE
  P\t…                            P response
  L\t<char code>\tSTORM           launch line;  "L\tOK" = already ok
  L\tPROBLEM\t<sub-code 1..=4>    refusal (fatal, messages above)
password hashing:  out[i] = ((pw[i] - 0x20) ^ key[i]) + 0x20
```

Session log layout:

```
<HYDRA_LOG_DIR>/<YYYY-MM-DD>/<YYYY-MM-DD>-<character>_<instance>.log
<HYDRA_LOG_DIR>/<YYYY-MM-DD>/<YYYY-MM-DD>-<character>_<instance>-000.bytes
                                            …             -001.bytes  (rotation)
```

---

## 3. Dependencies: what Phase 1 reconnected, and what it kept

### 3.1 Phase 1 reconnection

All at the versions pinned in the root `Cargo.toml`
(`[workspace.dependencies]`), declared here as `x = { workspace = true }`.

| real crate | pin | replaced |
| --- | --- | --- |
| `base64` | `0.22.1` | the hand-written encoder/decoder behind `eaccess::pin::{base64_encode,base64_decode}` (`general_purpose::STANDARD`, same public signatures) |
| `jiff` | `0.2.37` | the whole hand-rolled date layer in `eastern.rs`: Hinnant's day numbers, the fixed US DST rule, the `{:04}-{:02}-{:02}` printer and the round-trip date check (now `jiff::civil::Date` + `America/New_York`) |
| `regex` | `1.13.1` | the hand-written scanners in `trainer::page` (three `var` patterns, the `(?is)` skill-table pattern) and in `store::page` (the verification token's value, the "next available" date run) |
| `reqwest` | `0.12.28` | the header-text stand-ins: `store::http::{fetch,fetch_with,hop_client,Family::of_reqwest}`, `trainer::http::Trainer::{get_page,get_page_at,post_goals,post_goals_at}`, `weblogin::http::{location_of_response,status_of_response}` |
| `tokio` | `1.53.1` (`full`) | the async runtime behind the `reqwest`/`tokio-tungstenite` helpers and `#[tokio::test]` |
| `tokio-tungstenite` | `0.29.0` | `gemstone::shim::{shim_over,shim_plain}` return a real `WebSocketStream` again; `Duplex`/`ws_to_io` remain for the sync callers |

Extra feature flags: **none**.  `reqwest`'s `blocking` feature was wanted — the
store/trainer helpers are synchronous — but it pulls `futures-io`, which is not
in this offline registry, so resolution fails; the HTTP helpers are `async` and
`store::http::fetch_blocking` bridges the synchronous callers with a private
current-thread `tokio` runtime (it refuses to be called from inside a runtime
instead of panicking or deadlocking).  `reqwest`'s `cookies` feature is not
pinned either, so the session cookie travels as the explicit `Cookie:` header the
recovered code built.

`Cargo.lock` was pinned back to `hyper-util 0.1.20` (`cargo update -p hyper-util
--precise 0.1.20 --offline`): resolving `reqwest` otherwise picked `hyper-util
0.1.21`, which needs `base64 0.23.1` — not in the offline registry, and not the
workspace's `hyper-util` pin.

### 3.2 Kept stand-ins

| original | here | why |
| --- | --- | --- |
| `tokio-rustls` + `rustls` | `LiveSource::peer_certificate_der` carries the DER the TLS layer handed over; no TLS client is built here | not in the workspace pins, and the registry has no `tokio-rustls` at all.  `gemstone::shim::shim_over` therefore takes an already-established stream, which is exactly the seam the original's `ws_to_io` sat on |
| `url` | string splitting (`shim_host`, `next_hop`, the scrape helpers) | not in the workspace pins.  `reqwest` re-exports `url::Url`, but using it would be using the unpinned crate through a side door |
| `tracing` | the recovered literal text, returned instead of logged | not in the workspace pins; no sibling crate logs yet |
| `futures-util` (`StreamExt`/`SinkExt`) | not used: `shim_over`/`shim_plain` return the `WebSocketStream` and leave the frame pump to the caller | not in the workspace pins, and it is the one thing needed to drive frames.  **Integration risk**: whoever writes the relay needs it pinned (or a `send`/`recv` helper added once it is) |
| `jiff::SignedDuration` in `sink::config::stamp_ago` | the arrangement is text, built from the recovered format strings | `jiff` is pinned now, so this can be converted; it was left alone because the exact arrangement is UNKNOWN (§5) and converting it would encode a guess |
| `socket2`-style TCP keepalive (`live`) | `TCP_NODELAY` only, and the keepalive warning text stays a documented helper | `std` has no keepalive knob, and `socket2` is not a workspace pin |

No sibling Hydra crate is a dependency: nothing here needs `hydra_protocol` or
`gs_base`, so there is no cycle.

---

## 4. Tests

`cargo test -p hydra_net --offline` — 146 tests, all green.  **No test touches the
network**: every `reqwest` and `tokio-tungstenite` round trip is answered by a
loopback socket the test itself opened (`http::loopback::serve_once`, a one-shot
HTTP/1.1 server, or a `tokio::net::TcpListener` running the real
`tokio_tungstenite::accept_async`).

The original 117 assertions are all still there:

* `eastern`: known day numbers and round-trips (epoch, leap days, 1967/2100),
  the Sunday frame, the 2023/2024 DST boundaries on both sides, midnight on the
  switch days, week/stretch starts, `format`/`parse` round-trips and the
  rejections (`2024-3-05`, `2023-02-30`, `2023-02-29`, trailing junk).
* `eaccess::wire`: `hash_password` vectors (including the short-key refusal),
  `is_launch_ok`, `parse_launch` (each missing-field message), `expect_echo`,
  `redact`, `redact_char_code`, `resolve_char_code` (case-insensitive).
* `eaccess::pin`: base64 and PEM round-trips, the four decode failures, first-use
  recording, a changed certificate, a non-PEM pin, an unreadable pin.
* `eaccess::refusal`/`reject`/`fallback`: code 1..=4 mapping, unknown codes,
  non-refusal lines, rejection classes, the carried-forward failure.
* `gemstone`: instance rows and pages, redirect classes, subscription page,
  character-code scraping, hop trust, launch-URL errors, shim URLs.
* `store`: tag reading, claim detection, token scraping, family classification,
  URL shapes and messages, date parsing.
* `trainer`: var scanning (including the `skill`/`skills` prefix trap), skill
  rows, goal arithmetic, field numbering, submit body.
* `sink`: the four stamp shapes, rotation, redaction in the recording, the gap
  line, `hand_over`.
* `behind`/`live`/`record`: the lag warning text, `settle_all`, the recording
  thread, `TCP_NODELAY`, both directions recorded.

Added in Phase 1, for what the real crates now provide:

* `eastern`: the time zone database is the one answering (`America/New_York`
  resolves; January 1945 is War Time UTC-4 and 1974's February is daylight time,
  neither of which the recovered rule can produce); `jiff` and the recovered rule
  agree on the offset of **every hour from 2007 to 2027**; `midnight` agrees with
  the recovered arithmetic on every day of 2024; `week_start` is a Sunday 0..=6
  days back on every day of 1990..2030; jiff's printer matches the recovered
  `{:04}-{:02}-{:02}` on every day of 2024; out-of-range years still round-trip
  through the Hinnant fallback.
* `eaccess::pin`: the engine is the standard one (`+/` alphabet, URL-safe
  rejected), canonical padding required (`YQ`, `YQ=`, `YQ===` all rejected), all
  256 byte values round-trip, whitespace inside a body is ignored, a PEM block
  wraps at 64 columns and decodes flat or with surrounding text.
* `trainer::page`: the recovered patterns verbatim — the trailing `;`, `\s+` after
  `var`, no match for `vara`, an escaped name (`a.b` does not match `axb`), array
  items that are not numbers dropped; the skill table needs the recovered
  `</td> … <td>` bridge, matches case insensitively, and a non-numeric rank reads
  as zero.
* `store::page`: token values single-quoted, unquoted, unterminated (no value),
  `value =` with a space (no value), and a quoted token containing a line break;
  the "next available" run is read whole (a fourth field or a five-digit year is
  one invalid run).
* `store::http`: a real `reqwest` GET of a page (the request line checked from the
  server side), a 503 naming its status, a refused connection classified as
  `Unreachable` from `reqwest`'s own error, a bad URL as `Unusable`, the
  synchronous bridge both fetching and refusing inside a runtime, and the sign-in
  hop seam (`get_with` + `hop_client` → `location_of_response`) reading a real
  302 without following it.
* `trainer::http`: a real `reqwest` GET carrying the cookie and parsing the page,
  and a real POST whose urlencoded body (`skill0=3&train=1`) is checked from the
  server side; an unreachable page is `is_connect()`.
* `gemstone::shim`: a real `tokio-tungstenite` handshake over a loopback socket
  (`shim_over`, and `shim_plain`'s `ws://` dial), with the server's handshake
  handler confirming the recovered `/shim/Dridarn` path, and an absent shim
  reported as an I/O error.
* `weblogin::http`: a real 302 response's `Location` and status read off the
  `HeaderMap`, and a 200 that has no `Location`, in both spellings.

---

## 5. Significant unknowns

1. The `Launch` payload's third `&str` (`eaccess::wire`) — `GAMECODE` is a guess;
   the struct shape and size are solid.
2. The exact handshake sequencing (`eaccess/handshake.rs` is not in the symbol
   table; only its literals survive), so the stage order here is inferred from
   those literals.
3. `SkillGoals::cost_of_next` — the per-skill cost table is a static array.
4. The store's "next available" date parse (month-name and entity tables are
   inlined) — only numeric dates are handled.
5. The mask spellings in `wire::redact` and `sink::redactions` (`<key>`,
   `<account>`, `<session key>`).
6. `stamp_ago`'s exact arrangement of the moment and the age.
7. `Family::of`'s full phrase list.
8. `endpoint::other_spelling`'s canonical table (the host/port compares are
   recovered; the rows are not).
