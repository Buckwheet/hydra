# `hydra_net` — reverse-engineering notes

Source: arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra`, symbols at
`symbols_demangled.txt`; addresses below are unslid VAs, and a function ends
where the next symbol begins.  The original crate lives at
`crates/hydra-net/src/...` (the source paths survive in `__cstring`), and its
sibling modules that are *not* in the symbol table are named there too:
`eaccess/handshake.rs`, `eaccess/game.rs`, `live.rs`.

This crate is a **std-only** reconstruction: the original linked `tokio`,
`tokio-rustls`, `rustls`, `reqwest`, `tokio-tungstenite`, `jiff`, `regex`,
`base64` and `url`.  Nothing outside `std` is used here, so the async plumbing is
expressed as owned data structures plus synchronous helpers.

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
  of the timestamp.  Verified for 2023 and 2024, around each boundary.
* `date(t)` = `civil_from_days((t + offset(t)).div_euclid(86400))`.
* `midnight(d)` = `D*86400 - offset(D*86400 + 18000)`, so the switch days come
  out right (2024-03-10 → 05:00 UTC, 2024-11-03 → 04:00 UTC).
* `week_start(d)` is the **Sunday** on or before `d` (day numbers ≡ 3 mod 7).
* `stretch_start(d)` is `max(week_start(d), 1st of the month)` — the original
  walks backwards one day at a time, stopping on a Sunday or on day 1, so a week
  that straddles a month boundary is clipped to the 1st.
* `format` is `{:04}-{:02}-{:02}` (the `fmt::Arguments` piece table is at
  `0x103bfa729`).
* `parse` splits on `-`, requires field widths 4/2/2 and no fourth field, parses
  each as a signed integer (`str::parse::<i64>`, so `+3`/`-1` are accepted), then
  round-trips through the two day-number functions so `2023-02-30` is rejected
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

`base64_encode`/`base64_decode` are a std reimplementation of `base64-0.22`'s
default (padded, standard alphabet); `pem_encode`/`pem_decode` wrap them with the
PEM markers.

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
  response; this scans the header block.
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
  the path dropped; `shim_query` keeps the recovered `spcircname{}` key;
  `ws_to_io` pairs two halves into one `Duplex` (DEPARTURE: the original wrapped
  `tokio-tungstenite`'s `WebSocketStream`).

### 1.7 `store::page`, `store::http`

* `read(page, tag)` scans `<tag` then `</tag` (literals `<{}` / `</{}`) and
  returns the trimmed inner text.
* `offers_claim` is exactly `page.contains("/Store/ClaimReward")` (the original
  forwards to a shared `contains` helper with an 18-byte needle).
* `is_claimed` / `is_waiting` fold the page case and search the needles found in
  `0x101c5a3c4` (`clai…`) and `0x101c5a590` (`next sub`, `on bonus`).
  INFERRED: the exact longer spellings.
* `token` reads `__RequestVerificationToken` and its nearest `value=`.
* `asks_to_sign_in` / `signed_in`: INFERRED markers (the original compares a set
  of page fragments that are re-encoded through the entity table).
* `next_available` / `next_in`: the original parses the store's "next available"
  text through inlined month-name and HTML-entity tables
  (`0x103bfa7b7`..`0x103bfa808`).  UNKNOWN: only `M/D/YYYY` (and `M-D-YYYY`) is
  recovered here; anything else yields `None`.
* `store::http`: `Family::of` classifies a failure message (ESTABLISHED: it
  returns a small fieldless enum; the phrase lists come from
  `the store could not be read: {}` / `the store could not be reached: {}`),
  plus the recovered URL shapes
  (`{}/Account/SignIn?returnURL=%2Fstore%2Fpurchase%2F{}`,
  `{}/Store/ClaimReward`, `{}/store/purchase/{}`) and messages
  (`{}: the store's sign-in came to nothing`, `{}: the store did not take the
  password`, `{}: the store answered the claim with `{}``).

### 1.8 `trainer::{page,goals,http}`

* `page::page_url` = `https://www.play.net/gs4/play/cm/{}` (ESTABLISHED).
* `quoted_var` / `number_var` / `array_var` are the three recovered regexes
  (`var\s+NAME\s*=\s*"([^"]*)"\s*;`, `var\s+NAME\s*=\s*(-?\d+)\s*;`,
  `var\s+NAME\s*=\s*new\s+Array\(([^)]*)\)`) as hand-written scanners (no
  `regex` dependency).
* `read_page` reads the skill table
  (`(?is)<b class="normS2">{}</b></td>\s*<td[^>]*><b class="normS1">([^<]+)</b>`)
  and refuses a page with no rows (`the page has no {}`).
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

## 3. Departures from the original (std only)

| original | here |
| --- | --- |
| `jiff` (`Zoned`, `SignedDuration`) | `eastern.rs` + `SystemTime`, fixed US DST rule |
| `tokio` + `tokio-rustls` + `rustls` | synchronous helpers; `LiveSource` carries the DER |
| `reqwest` + cookie jar | URL/header helpers only (`store::http`, `weblogin::http`) |
| `tokio-tungstenite` | `shim::Duplex` over two halves |
| `regex` | hand-written scanners (`trainer::page`, `store::page`) |
| `base64-0.22` | `eaccess::pin::{base64_encode,base64_decode}` |
| `tracing` log lines | the recovered literal text, returned instead of logged |
| `url` | string splitting |

No sibling Hydra crate is a dependency: everything is `std` (so there is no
cycle and nothing here needs `hydra_protocol`/`gs_base`).

---

## 4. Tests

`cargo test -p hydra_net` — 117 assertions across:

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
