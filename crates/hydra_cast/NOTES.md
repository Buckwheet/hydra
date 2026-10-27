# `hydra_cast` — reverse-engineering notes

Source: arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra` (no DWARF).  Every
address below is an unslid virtual address and can be re-read with
`/usr/bin/objdump -d --start-address=0xA --stop-address=0xB --no-show-raw-insn`.

This crate is a **std-only** reconstruction of `hydra_cast`: the original linked
`tokio`, `serde`/`serde_json` and `hydra_model`.  See §4 for each departure.

Legend

* **ESTABLISHED** — directly visible in the disassembly, the literals or the
  field offsets.
* **INFERRED** — follows from the code shape, but the original name/shape is not
  in the binary.
* **UNKNOWN** — not recoverable; the safest behaviour was chosen and is marked
  `// UNKNOWN:` in the source.

## 0. How the evidence was read

* The parent inventory lists 18 `hydra_cast` text symbols, but `nm -an` has
  **105**: the stub table only kept the `T` (external) ones.  The local `t`
  symbols gave the crate's helpers and, more importantly, the tokio state
  machines (`Game::next::{{closure}}` etc.) and the serde impls.  They are
  listed in `src/lib.rs`.
* `__TEXT,__cstring` format strings are stored as length-prefixed pieces with a
  single `0xC0` byte per `{}`, e.g. at `0x103e7b6ef`:
  `c0 15 " is on the hide list." c0` is `"{} is on the hide list.{}"`.
* `__TEXT,__const` holds plain literals **concatenated without separators**
  (e.g. `0x1039c4640` reads `castchannelevoke…`), so a literal's length always
  comes from the instruction stream (usually `mov w1, #len` before a
  `contains`/`memcmp`, or the length half of a `[&str; N]` table).
* `[&str; N]` constants are stored as two parallel arrays — pointers in
  `__DATA_CONST` and `u64` lengths in `__TEXT,__const`.  Reading both gives the
  stance list, the verb words and the incant suffixes exactly.
* Rust's `Result`/`Option` layouts show up as niche words, mostly
  `0x8000_0000_0000_0000`.  In this toolchain that word sits in the `cap` field
  of the `String` variant and means "the other variant" (`Ok` for
  `Result<u16, String>`, `None` for `Option<String>`), so a payload at `+8` with
  that word at `+0` is *not* a `String`.

## 1. Function table

`full` = recovered from the disassembly; `partly` = entry contract, literals and
called helpers recovered, body not; `seam` = a departure (see §4).

| module | function | address | status |
| --- | --- | --- | --- |
| `settings` | `CasterSettings::set_option` | `0x1019b2690` | full |
| `settings` | `CasterSettings::set_stance` | `0x1019b2c40` | full |
| `settings` | `CasterSettings::clear_alias` | `0x1019b3308` | full |
| `settings` | `CasterSettings::toggle_hide` | `0x1019b33fc` | full |
| `settings` | `CasterSettings::verb` | `0x1019b350c` | full |
| `settings` | `CasterSettings::alias` | `0x1019b3720` | full |
| `settings` | `CasterSettings::listed` | `0x1019b3d14` | full (the per-entry format of `Verbs:` is INFERRED from the shared `"{} {}"` literal) |
| `settings` | `CasterSettings::stance` | `0x1019b4384` | full |
| `settings` | `CasterSettings::switch` | `0x1019b4598` | full |
| `settings` | `CasterSettings::set_verb` | `0x1019b4640` | full |
| `settings` | `Chosen::named` | `0x1019b4ce8` | full |
| `settings` | `CasterSettings::which` (not in the stub table) | `0x1019b3b24` | full |
| `settings` | `settings::spell` (not in the stub table) | `0x1019b4b14` | full except the spell table itself (seam) |
| `settings` | `listed::{{closure}}` | `0x1019b4270` | full (`"none"` when the list is empty, else `", "`-joined) |
| `answers` | `answers::answer` | `0x1019a0564` | partly — six whole-sentence arms recovered, the rest UNKNOWN |
| `answers` | `answers::stance` (not in the stub table) | `0x1019a1de8` | full |
| `plan` | `plan::plan` | `0x1019b0ecc` | partly — checks, order, literals and command formats recovered; the tail of the builder is UNKNOWN |
| `request` | `request::request` | `0x1019a1ef0` | partly — byte-slice scanner; only two of its texts recovered |
| `driver` | `Game::fold` | `0x1019a7c70` | full |
| `driver` | `driver::start` | `0x1019a7d14` | seam — 37 KB tokio task, synchronous equivalent |
| `driver` | `Game::{out_of_roundtime,idle,next,send,stance,release}` | `0x10197a58c`..`0x10197bd4c` | partly — call graph recovered, bodies are `async` state machines |
| `driver` | `Settings::get` | `0x1019a82dc` | partly — `Arc<Mutex<Read>>` shape recovered |

## 2. Recovered constants and literals

### 2.1 `CasterSettings` layout (ESTABLISHED)

```
+0x00  aliases: BTreeMap<String, u16>       name -> spell number
+0x18  verbs:   BTreeMap<String, Chosen>     name -> verb      (1-byte value)
+0x30  stances: BTreeMap<String, String>     name -> stance
+0x48  hide:    BTreeSet<u16>
+0x60  options: channel conserve safety stance hide   (bytes 0x60..0x64)
+0x65  on:      bool
```

Evidence: `BTreeMap::remove`/`insert` receivers at `+0x00` (`0x1019b333c`,
`0x1019b3a48`), `+0x18` (`0x1019b4850`), `+0x30` (`0x1019b2d70`,
`0x1019b30f8`), `+0x48` (`0x1019b343c`, `0x1019b348c`); the option bytes are
hard-coded in `set_option` (`mov w8, #0x60/0x61/0x62/0x63/0x64` before
`strb w24, [x26, x8]`), `toggle_hide` reads `+0x64`, `switch` writes `+0x65`,
and `listed` loads all six.

### 2.2 serde shape (ESTABLISHED)

`__Field` indices from the field visitors `0x1019a95ac` (`CasterSettings`) and
`0x1019a948c` (`Options`), cross-checked against the `Serialize` impls
(`0x101276278`, `0x1012760e8`, `0x101276008`):

```
CasterSettings { on, options, aliases, verbs, stances, hide }
Options        { channel, conserve, safety, stance, hide }
Chosen         = "cast" | "channel" | "evoke"
```

`expecting` at `0x1019b51ec` is `"struct Options"`, at `0x1019b5200` is
`"struct CasterSettings"`; the `Chosen` enum deserialiser is at `0x1019b49a8`
(unknown variants go through `serde::de::Error::unknown_variant`).

### 2.3 Word tables (ESTABLISHED)

| table | pointers | lengths | contents |
| --- | --- | --- | --- |
| verb words | `0x10449c388` | `0x1039c46d0` | `cast`, `channel`, `evoke` (4, 7, 5) |
| stances | `0x10449c640` | inline | `offensive, advance, forward, neutral, guarded, defensive` (9, 7, 7, 7, 7, 9) |
| incant suffixes | `0x10449c628` | `0x1039c4998` | `channel`, `evoke`, `channel evoke` (7, 5, 13) |

The two "on"/"off" words are `0x1039c49b8` and `0x1039c49ba`; the list separator
is `0x1039c49f6` (`", "`) and the empty-list word is `"none"`.

### 2.4 Messages (ESTABLISHED, verbatim)

`settings` (from `__TEXT,__cstring`):

```
0x103e7b536  "{} is not a spell."
0x103e7b5c9  "`{}` is not an option: channel, conserve, safety, stance or hide."
0x103e275df  "`{}` is not on or off."
0x103e62c3e  "{} is {}."
0x103e7b60c  "{} had no stance of its own."
0x103e7b629  "{} has no stance of its own."
0x103e7b646  "`{}` is not a stance: {}."
0x103e7b661  "{} was cast from {}; it is now cast from {}."
0x103e7b68e  "{} is cast from {}."
0x103e7b6a2  "There is no alias {}."
0x103e7b6b9  "The alias {} is taken away."
0x103e7b6d6  "{} is off the hide list."
0x103e7b6ef  "{} is on the hide list.{}"
0x103e7b708  "\nThe alias {} was spell {}; it is now {}."
0x103e7b732  "\nThe alias {} casts spell {}."
0x103e7b750  "Typed spells cast: {}"
0x103e7b766  "Options: channel {}, conserve {}, safety {}, stance {}, hide {}"
0x103e6164b  "Aliases: {}"
0x103e7b7a6  "Verbs: {}"
0x103e7b7b0  "Stances: {}"
0x103e7b7bc  "Hide after: {}"
0x103e7b7cb  "{} had no verb of its own."
0x103e7b7e6  "{} is cast as it would be."
0x103e7b801  "{} was cast with {}; it is now {}."
0x103e7b824  "{} is cast with {}."
0x103e7b838  "`{}` is not a verb: cast, channel or evoke."
0x103e7b865  "`{}` is not a spell number."
```

Fixed (no argument) messages are also in `__TEXT,__const`:

```
0x1039c4a21  "An alias is one word of letters and digits."        (43 bytes)
0x1039c4a4c  "An alias is not a number: that is a spell's own."   (48 bytes)
0x1039c49f8  " The hide option is off: .ec set hide on."          (41 bytes, leading space)
0x1039c4a80  "Typed spells are sent as typed."                    (31 bytes)
0x1039c4aa4  "Typed spells are cast."                             (22 bytes)
```

`plan`:

```
0x103e7b521  "You do not know {}."
0x103e7b4c4  "Not enough mana to cast {}."
0x103e7b4e1  "No live target here is {}."
0x103e7b4fd  "No live target here to cast {} at."
0x103e7b549  "{} asks nothing to be done: it works by itself."
0x103e7b579  "{} is not cast by a command."
0x103e6c6d3  "prepare {}"      0x103e6c6dd  "cast {}"
0x103e7b596  "cast{}"          0x103e7b59d  "cast {}{}"
0x103e7b5a6  "target {}"       0x103e7b5b0  "incant {}{}"
0x103e7b5bb  "incant {} {}{}"  0x1039c4965  "offensive"
0x10206f4b0  "mana"            0x1039c4973  "second"
```

`answers` (all six are `str::contains` tests, with the length in the code):

```
0x1039c47f8  34  "You can only evoke certain spells."
0x1039c481a  52  "You can only channel certain spells for extra power."
0x1039c484e  28  "Spells of War cannot be cast "
0x1039c473f  69  "As you begin to prepare the spell the wind blows small objects at you"
0x1039c47aa  78  "As you focus on your magic, your vision swims with a swirling haze of crimson."
0x1039c4784  38  "That is not something you can prepare."
```

`answers::stance` is immediate comparisons, recovered as
`line.starts_with("You are now in a") && line.ends_with(" stance.")` for lines
of at least 16 bytes, or
`line.starts_with("Cast Roundtime in effect: Setting stance to ")` for lines of
at least 45 bytes.

`driver`:

```
0x103e7b1cc  "The stance did not change to {}."
0x103e7b1ee  "{} could not be prepared."
0x103e7b208  "No answer to `{}`."
0x103e7b010  "crates/hydra-cast/src/driver.rs"   (the crate's only panic location)
```

The module's own help text (linked from the GUI, not from this crate) confirms
the grammar: `ec on|off|status`, `ec alias <spell> <name>`, `ec alias clear
<name>`, `ec verb <spell|alias> <verb>`, `ec stance <spell|alias> <stance>`,
`ec hide <spell>`, `ec set <option> on|off` with
`channel conserve safety stance hide`.  It is stored as one blob at file offset
`0x2407c7a`; it is not referenced from any `hydra_cast` symbol.

## 3. Behaviour details worth keeping

* `alias`, `clear_alias` and `which` trim and **Unicode**-lower-case
  (`str::to_lowercase` at `0x101eb538c`); `set_option`, `set_verb` and
  `set_stance` lower-case **ASCII** only and do not trim (the vectorised
  `sub #0x41 / cmp #0x1a / orr #0x20` loop at e.g. `0x1019b274c`).
* `spell` trims, parses a `u16` (a leading `+` is accepted — `cmp w8, #0x2b`,
  and `-` is not: `cmp w8, #0x2d` jumps to the error) and then requires
  `hydra_model::spells::spell(n)` to exist.  Its error message keeps the
  **untrimmed** argument.
* An alias name may contain `_` (`cmp w22, #0x5f`) as well as any Unicode
  alphanumeric, and it must not be all ASCII digits.
* `alias` messages start with `\n`; `clear_alias` does not.
* `set_option` reports the **lower-cased** option name but the **raw** value
  (`mov x8, sp` with the value saved before it was lower-cased).
* `verb(n, name)`/`stance(n, name)` search the name first and then
  `n.to_string()`, with no lower-casing of their own.
* `plan` only asks `KnownSpells::knows` for numbers below 9000
  (`lsr w8, w20, #3; cmp w8, #0x465`).
* `Game::fold` returns only the lines it has not returned before; an event that
  replaces the model resets that counter and returns an empty `Vec`.

## 4. Departures from the original (std only)

| original | here | why |
| --- | --- | --- |
| `hydra_model::spells`, `state::GameState`, `KnownSpells`, `spell_cost`, `ProcLine` | `game::World` trait + `game::install`/`game::world` | `hydra_model` is being recovered on another branch; the crate may not depend on it yet.  With no `World` installed every spell number is refused — nothing is invented. |
| `hydra_session` command queue (from `Game::send`, `cast_line`, `cast_through`) | `driver::Sink` (`fn say(&mut self, line)`) | keeps the model/state logic, no-op seam for the transport |
| `tokio` (task, `mpsc::UnboundedReceiver<Request>`, `broadcast`, `Mutex`/`Arc`) | `std::sync::mpsc::Receiver`, `std::sync::{Arc, Mutex}` | synchronous equivalent: requests are consumed in order, no waiting |
| `serde` + `serde_json` (`Serialize`/`Deserialize` for `CasterSettings`, `Options`, `Chosen`) | plain structs with `Default`/`Clone`/`Debug`/`PartialEq`; the JSON member names are recorded above but no reader/writer is shipped | only the *shape* is in the binary; a JSON implementation was not needed by any recovered function |
| `regex`-free byte scanner in `request::request` | `request(line: &str)` over the crate's own command grammar | the original scans a byte slice for game text (see below) |

## 5. Unknowns

1. **`request::request`** (19 856 bytes): the argument is a byte slice
   (`x26 = x0 + x1` once at `0x1019a1f34`) and the body is a chain of
   length-then-byte comparisons against game text.  Only
   `"keeps the spell from working."` (`0x1039c4706`) and
   `"keep the spell from working."` (`0x1039c4723`) could be named; they are kept
   as `request::BLOCKED_TEXTS`.  The rest needs the game's message corpus.
   `Request`'s field list is UNKNOWN — the crate only knows that it is sent to
   the driver over an unbounded channel and owns heap data.
2. **`driver::start`** (37 304 bytes, 11 604-byte body): a tokio task that
   selects over requests, session events and settings changes.  The synchronous
   `start` keeps the order (fold events → plan → send) and the settings cell but
   has no waiting.
3. **`Game::{next, send, stance, release, cast, cast_line, cast_through,
   prepare, answer, out_of_roundtime}`**: the call graph
   (`out_of_roundtime → idle → next → fold`, `stance → next, send,
   answers::answer`, `release → send, next, answers::answer`) is established;
   the bodies are `async` state machines and were not unwound.  `Game::fold` is
   the one that was.
4. **`hydra_cast::driver::Halt`** variants: only its drop glue
   (`0x1019857dc`) is in the table.  `Halt::{Closed, Unprepared}` is INFERRED
   from "{} could not be prepared.".
5. **`driver::Read`**: the type behind the settings mutex; only its drop glue
   (`0x1019a9de4`) is listed.  The generation counter is INFERRED.
6. **`answers::answer`'s enum**: by value, niche-tagged
   (`0x8000_0000_0000_0000 | index`), at least eight arms.  Six whole-sentence
   arms are recovered; the short-prefix arms compare immediates and were not
   enumerated, so those lines come back as `None`.
7. **`plan`'s builder tail**: the relative order of `prepare`/`target`/`cast`
   and the flag that selects `evoke` vs `channel evoke`
   (`options.channel` at `0x1019b1514` feeds the suffix index, but the predicate
   was not recovered).
8. **`U+… kraken tentacle`**: `0x103e7b4b1` is the format `"{} kraken tentacle"`
   and is used four times in `plan`'s target section.  It is recorded but not
   implemented — it looks like a fixture, and guessing its condition would be
   invented behaviour.
9. **`settings::spell`'s "is it a spell" test** needs `hydra_model`; see §4.

## 6. Tests

`cargo build -p hydra_cast --offline` and the two test harnesses
(`--test cast` and the lib unit tests) pass; 24 tests:

```
tests/cast.rs       14  every expected string is a recovered literal
src/answers.rs       3
src/request.rs       4
src/driver.rs        3  (fold, the start loop, the settings cell)
```

`tests/cast.rs` installs a three-spell `game::World` fixture; the spell numbers
and names there are made up (the real table is `hydra_model`'s) but every
message, list order and state transition asserted is from the binary.
