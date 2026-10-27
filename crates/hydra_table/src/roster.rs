//! `roster.json` — the characters this account has played.
//!
//! Recovered from `hydra_table::roster` (`0x10129cb04` … `0x10129e584`).  The
//! file is a `serde_json` document (see [`crate::json`]) with the fields the
//! `expecting` strings in `__TEXT` name:
//!
//! ```text
//! 0x10240c7e8  "schema_version"   0x10240c7f1  "characters"
//! 0x10240c800  "character"  0x10240c809 "account"  0x10240c810 "game_code"
//! 0x10240c819  "favourite"  0x10240c822 "nickname"
//! ```
//!
//! The map keys are `game_code:character`, lower-cased and trimmed
//! ([`key`], `0x10129cb80`).

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use crate::json;

/// The file name inside the data directory.
///
/// **ESTABLISHED**: the 11-byte piece at `0x10240c82a` (`"roster.json, "`),
/// joined onto the data directory in [`load`] and in
/// `hydra::connect::Target::resolve` (`0x100113814`).
pub const FILE_NAME: &str = "roster.json";

/// The schema version written by [`save`].
///
/// **ESTABLISHED** (`roster::save`, `0x10129cf68`): `mov w8, #1` /
/// `str w8, [file, #0x18]!` right before the directory is created.
pub const SCHEMA_VERSION: u32 = 1;

/// One character on the roster.
///
/// **ESTABLISHED** layout (`Entry::of` `0x10129d288`, `Entry::card`
/// `0x10129d344`, `Entry::serialize` `0x10129c9b4`, size `0x68`):
///
/// | offset | field |
/// | --- | --- |
/// | `+0x00` | `character: String` |
/// | `+0x18` | `account: String` |
/// | `+0x30` | `game_code: String` |
/// | `+0x48` | `nickname: Option<String>` (niche `cap == isize::MIN`) |
/// | `+0x60` | `favourite: bool` |
///
/// The serialisation order is the declaration order recovered from
/// `Entry::serialize`: `character`, `account`, `game_code`, `favourite`,
/// `nickname` (the latter only when it is `Some`).
///
/// **UNKNOWN**: `Entry::card` also writes a byte at `+0x61`, which cannot be a
/// field of a five-field `Entry`; the binary stores `self.favourite` there.
/// Nothing in this reconstruction reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub character: String,
    pub account: String,
    pub game_code: String,
    pub favourite: bool,
    pub nickname: Option<String>,
}

/// The whole `roster.json`.
///
/// **ESTABLISHED** layout (`roster::load` `0x10129cd1c`): the map lives at
/// `+0x00` (`BTreeMap`, 24 bytes) and `schema_version: u32` at `+0x18`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct File {
    pub schema_version: u32,
    pub characters: BTreeMap<String, Entry>,
}

impl Entry {
    /// A blank entry for the same character: no nickname, not a favourite.
    ///
    /// **ESTABLISHED** (`Entry::of`, `0x10129d288`): clones `character`,
    /// `account` and `game_code`, writes `None` over the nickname slot
    /// (`isize::MIN` at `+0x48`) and `false` over `favourite` (`strb wzr,
    /// [x19, #0x60]`).
    pub fn of(&self) -> Entry {
        Entry {
            character: self.character.clone(),
            account: self.account.clone(),
            game_code: self.game_code.clone(),
            favourite: false,
            nickname: None,
        }
    }

    /// This entry as a card with `favourite` replaced.
    ///
    /// **ESTABLISHED** (`Entry::card`, `0x10129d344`): clones the three strings
    /// and the nickname, then stores the argument in `favourite` and — see the
    /// struct comment — `self.favourite` in the byte after it.
    pub fn card(&self, favourite: bool) -> Entry {
        Entry {
            character: self.character.clone(),
            account: self.account.clone(),
            game_code: self.game_code.clone(),
            favourite,
            nickname: self.nickname.clone(),
        }
    }

    /// The map key for this entry.
    pub fn key(&self) -> String {
        key(&self.game_code, &self.character)
    }

    fn from_json(value: &json::Value) -> Option<Entry> {
        let map = value.object()?;
        Some(Entry {
            character: map.get("character")?.text()?.to_string(),
            account: map.get("account")?.text()?.to_string(),
            game_code: map.get("game_code")?.text()?.to_string(),
            favourite: map.get("favourite").and_then(|v| v.bool()).unwrap_or(false),
            nickname: map
                .get("nickname")
                .and_then(|v| v.text())
                .map(|text| text.to_string()),
        })
    }

    fn write_json(&self, out: &mut String) {
        out.push('{');
        out.push_str("\"character\":");
        json::write_text(out, &self.character);
        out.push_str(",\"account\":");
        json::write_text(out, &self.account);
        out.push_str(",\"game_code\":");
        json::write_text(out, &self.game_code);
        out.push_str(",\"favourite\":");
        out.push_str(if self.favourite { "true" } else { "false" });
        if let Some(nickname) = &self.nickname {
            out.push_str(",\"nickname\":");
            json::write_text(out, nickname);
        }
        out.push('}');
    }
}

impl File {
    fn from_json(value: &json::Value) -> Option<File> {
        let map = value.object()?;
        let mut characters = BTreeMap::new();
        if let Some(entries) = map.get("characters").and_then(|v| v.object()) {
            for (name, entry) in entries {
                characters.insert(name.clone(), Entry::from_json(entry)?);
            }
        }
        Some(File {
            schema_version: map.get("schema_version").and_then(|v| v.u32()).unwrap_or(0),
            characters,
        })
    }
}

/// The serialised form of `file` (what [`save`] writes).
pub fn to_json(file: &File) -> String {
    let mut out = String::new();
    json::write_file(&mut out, SCHEMA_VERSION, "characters", |out| {
        for (at, (name, entry)) in file.characters.iter().enumerate() {
            if at > 0 {
                out.push(',');
            }
            json::write_text(out, name);
            out.push(':');
            entry.write_json(out);
        }
    });
    out
}

/// `game_code:character`, trimmed and lower-cased.
///
/// **ESTABLISHED** (`roster::key`, `0x10129cb80`): `trim_matches` on both
/// arguments, `format!("{}:{}", first, second)` (the two-byte piece `":"` at
/// `0x103e21670`) and `<str>::to_lowercase`.  Both call sites pass
/// `(game_code, character)`: `record` (`0x10129d954`) and `favourite`
/// (`0x10129e734`) read the two strings from `+0xd8`/`+0xa8` of the entry, the
/// `game_code` and `character` fields.
pub fn key(game_code: &str, character: &str) -> String {
    format!("{}:{}", game_code.trim(), character.trim()).to_lowercase()
}

/// The process-wide guard that serialises access to `roster.json`.
///
/// **ESTABLISHED**: `roster::record` (`0x10129d858`), `forget`, `nickname` and
/// `favourite` all load a `LazyLock<Mutex<()>>`-style `OnceBox` at
/// `0x10458ea48` and `Mutex::lock`/`unlock` around load/find/insert/save.
static ROSTER: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Read `dir/roster.json`.
///
/// **ESTABLISHED** (`0x10129cd1c`): `dir.join("roster.json")`,
/// `fs::read_to_string`; a `NotFound` error yields the default (empty) file, any
/// other I/O error is returned as-is and a JSON error becomes
/// `io::Error::new(ErrorKind::InvalidData, format!("{}: {}", _, _))`
/// (`0x10129ce14` passes `0x15` = `InvalidData`).
pub fn load(dir: &Path) -> Result<File, io::Error> {
    let path = dir.join(FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(File::default()),
        Err(error) => return Err(error),
    };
    match json::parse(&text) {
        Ok(value) => File::from_json(&value).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("{path:?}: bad roster"))
        }),
        Err(error) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", path.display(), error),
        )),
    }
}

/// Write `dir/roster.json`, creating the directory first.
///
/// **ESTABLISHED** (`0x10129cf40`): stamps `schema_version = 1`, creates the
/// directory with mode `0o777` and `recursive: true` (`0x10129cf74`), serialises
/// `{"schema_version":1,"characters":{…}}` and writes it; a serialisation error
/// becomes `InvalidData` (`0x10129d040`).
pub fn save(dir: &Path, file: &mut File) -> Result<(), io::Error> {
    file.schema_version = SCHEMA_VERSION;
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(FILE_NAME), to_json(file))
}

/// Every entry, ordered by key.
///
/// **ESTABLISHED** (`0x10129cb04`): `load(dir)` then
/// `file.characters.into_iter().map(|(_, v)| v).collect()` — the
/// `SpecFromIterNested` call at `0x10129cb6c`.  Errors are returned unchanged
/// (`isize::MIN` in the cap slot).
pub fn all(dir: &Path) -> Result<Vec<Entry>, io::Error> {
    let file = load(dir)?;
    Ok(file.characters.into_values().collect())
}

/// Look `what` up in the roster.
///
/// **ESTABLISHED** (`0x10129cc44`): `load(dir)` then [`find_in`] on the map.
pub fn find(dir: &Path, what: &str) -> Result<Option<Entry>, io::Error> {
    let file = load(dir)?;
    find_in(&file.characters, what)
}

/// Look `what` up in `map`.
///
/// **ESTABLISHED** (`roster::find_in`, `0x10129dc2c`):
///
/// * every `:` split `a:b` of `what` is looked up as [`key`]`(a, b)`
///   (`memchr` for `0x3a` at `0x10129dcb8`);
/// * when `what` has no `:`, every entry whose `account` matches `what`
///   case-insensitively is collected instead (the byte-wise ASCII fold that
///   `teams::keep` inlines is the same comparison);
/// * no match is `Ok(None)` (`isize::MIN` written to the result),
///   one match is `Ok(Some(entry))`, and more than one is
///   `io::Error::new(ErrorKind::InvalidInput, format!("{} is on more than one
///   game ({}); say which, as GAME:", what, games.join(", ")))` — the format
///   string at `0x10240c837`, `0x14` = `InvalidInput` at `0x10129e080`, and the
///   `join_generic_copy` with the `", "` separator at `0x10240c835`.
pub fn find_in(map: &BTreeMap<String, Entry>, what: &str) -> Result<Option<Entry>, io::Error> {
    let mut found: Vec<&Entry> = Vec::new();
    let mut split = what.split(':');
    let first = split.next().unwrap_or("");
    let mut looked_up = false;
    for rest in split {
        looked_up = true;
        if let Some(entry) = map.get(&key(first, rest)) {
            found.push(entry);
        }
    }
    if !looked_up {
        for entry in map.values() {
            if entry.account.eq_ignore_ascii_case(what) {
                found.push(entry);
            }
        }
    }
    match found.len() {
        0 => Ok(None),
        1 => Ok(Some(found[0].clone())),
        _ => {
            let games: Vec<&str> = found
                .iter()
                .map(|entry| entry.game_code.as_str())
                .collect();
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} is on more than one game ({}); say which, as GAME:",
                    what,
                    games.join(", ")
                ),
            ))
        }
    }
}

/// Remember `entry` on the roster.
///
/// **ESTABLISHED** (`roster::record`, `0x10129d814`): take the mutex, `load`,
/// key the entry with [`key`]`(game_code, character)`, merge an existing entry
/// (`favourite` is OR-ed — `orr w8, w9, w8` at `0x10129da00` — and the old
/// nickname is kept when the new one is `None`, `0x10129da08`), insert, `save`.
/// The mutex is released on every path (`Mutex::unlock` at `0x10129db44`).
pub fn record(dir: &Path, entry: &Entry) -> Result<(), io::Error> {
    let _guard = ROSTER.lock().unwrap();
    let mut file = load(dir)?;
    let at = key(&entry.game_code, &entry.character);
    let mut entry = entry.clone();
    if let Some(old) = file.characters.get(&at) {
        entry.favourite |= old.favourite;
        if entry.nickname.is_none() {
            entry.nickname = old.nickname.clone();
        }
    }
    file.characters.insert(at, entry);
    save(dir, &mut file)
}

/// Remove the entry `what` names.
///
/// **ESTABLISHED** (`roster::forget`, `0x10129d45c`): mutex, `load`, [`find_in`],
/// then `BTreeMap::remove` of [`key`]`(game_code, character)` and `save`.  A
/// missing entry is **not** an error: the `Ok(None)` path falls through to the
/// same `isize::MIN` result as the removal.
pub fn forget(dir: &Path, what: &str) -> Result<(), io::Error> {
    let _guard = ROSTER.lock().unwrap();
    let mut file = load(dir)?;
    if let Some(entry) = find_in(&file.characters, what)? {
        file.characters
            .remove(&key(&entry.game_code, &entry.character));
    }
    save(dir, &mut file)
}

/// Set (or clear) the nickname of the entry `what` names.
///
/// **ESTABLISHED** (`roster::nickname`, `0x10129e148`): the third argument is a
/// `&Option<String>` (`ldp x20, x21, [x4]` / `ldr x27, [x4, #0x10]`), mutex,
/// `load`, [`find_in`], then the entry is cloned with the nickname replaced,
/// re-inserted under [`key`] and saved (`0x10129e350` … `0x10129e3f4`).  A
/// missing entry returns `Ok(None)` and changes nothing.
pub fn nickname(
    dir: &Path,
    what: &str,
    nickname: Option<String>,
) -> Result<Option<Entry>, io::Error> {
    let _guard = ROSTER.lock().unwrap();
    let mut file = load(dir)?;
    let Some(mut entry) = find_in(&file.characters, what)? else {
        return Ok(None);
    };
    entry.nickname = nickname;
    file.characters
        .insert(key(&entry.game_code, &entry.character), entry.clone());
    save(dir, &mut file)?;
    Ok(Some(entry))
}

/// Mark (or unmark) the entry `what` names as a favourite.
///
/// **ESTABLISHED** (`roster::favourite`, `0x10129e584`): mutex, `load`,
/// [`find_in`], `strb w23, [entry, #0x60]` (the third argument is the new
/// `favourite`), re-key, clone, `BTreeMap::insert`, `save`.
pub fn favourite(dir: &Path, what: &str, on: bool) -> Result<Option<Entry>, io::Error> {
    let _guard = ROSTER.lock().unwrap();
    let mut file = load(dir)?;
    let Some(mut entry) = find_in(&file.characters, what)? else {
        return Ok(None);
    };
    entry.favourite = on;
    file.characters
        .insert(key(&entry.game_code, &entry.character), entry.clone());
    save(dir, &mut file)?;
    Ok(Some(entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_table_roster_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn entry(character: &str, account: &str, game: &str) -> Entry {
        Entry {
            character: character.to_string(),
            account: account.to_string(),
            game_code: game.to_string(),
            favourite: false,
            nickname: None,
        }
    }

    #[test]
    fn the_key_is_lower_cased_and_trimmed() {
        assert_eq!(key(" GS4 ", " Aelith "), "gs4:aelith");
        assert_eq!(key("gs3", "Wynter"), "gs3:wynter");
    }

    #[test]
    fn of_clears_the_nickname_and_the_favourite() {
        let mut old = entry("Aelith", "main", "GS4");
        old.nickname = Some("Aely".to_string());
        old.favourite = true;
        let blank = old.of();
        assert_eq!(blank.nickname, None);
        assert!(!blank.favourite);
        assert_eq!(blank.character, "Aelith");
        assert_eq!(blank.account, "main");
        assert_eq!(blank.game_code, "GS4");
    }

    #[test]
    fn card_replaces_the_favourite() {
        let mut old = entry("Aelith", "main", "GS4");
        old.nickname = Some("Aely".to_string());
        let card = old.card(true);
        assert!(card.favourite);
        assert_eq!(card.nickname.as_deref(), Some("Aely"));
        assert_eq!(card.character, "Aelith");
    }

    #[test]
    fn a_missing_file_is_an_empty_roster() {
        let file = load(&dir("missing")).expect("load");
        assert_eq!(file, File::default());
        assert_eq!(file.schema_version, 0);
    }

    #[test]
    fn record_then_find_round_trips_through_json() {
        let dir = dir("round_trip");
        let mut one = entry("Aelith", "main", "GS4");
        record(&dir, &one).expect("record");
        let text = std::fs::read_to_string(dir.join(FILE_NAME)).expect("file");
        assert_eq!(
            text,
            r#"{"schema_version":1,"characters":{"gs4:aelith":{"character":"Aelith","account":"main","game_code":"GS4","favourite":false}}}"#
        );
        // The key is `game:character`, so the short form looks it up directly.
        assert_eq!(find(&dir, "GS4:Aelith").expect("find"), Some(one.clone()));
        // A bare account name scans the entries.
        assert_eq!(find(&dir, "MAIN").expect("find"), Some(one.clone()));
        assert_eq!(find(&dir, "nobody").expect("find"), None);

        one.nickname = Some("Aely".to_string());
        record(&dir, &one).expect("record");
        assert_eq!(all(&dir).expect("all"), vec![one.clone()]);
        assert_eq!(
            nickname(&dir, "main", None).expect("nickname"),
            Some(entry("Aelith", "main", "GS4"))
        );
        assert_eq!(all(&dir).expect("all")[0].nickname, None);

        assert_eq!(
            favourite(&dir, "main", true)
                .expect("favourite")
                .map(|e| e.favourite),
            Some(true)
        );
        forget(&dir, "gs4:aelith").expect("forget");
        assert!(all(&dir).expect("all").is_empty());
    }

    #[test]
    fn record_keeps_the_old_nickname_and_favourite() {
        let dir = dir("merge");
        let mut old = entry("Aelith", "main", "GS4");
        old.nickname = Some("Aely".to_string());
        old.favourite = true;
        record(&dir, &old).expect("record");
        let fresh = entry("Aelith", "main", "GS4");
        record(&dir, &fresh).expect("record again");
        let kept = find(&dir, "main").expect("find").expect("entry");
        assert_eq!(kept.nickname.as_deref(), Some("Aely"));
        assert!(kept.favourite, "favourite is or-ed with the stored one");
    }

    #[test]
    fn two_games_for_one_account_is_an_invalid_input_error() {
        let dir = dir("ambiguous");
        record(&dir, &entry("Aelith", "main", "GS4")).expect("record");
        record(&dir, &entry("Aelith", "main", "GS3")).expect("record");
        let error = find(&dir, "main").expect_err("ambiguous");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            "main is on more than one game (GS3, GS4); say which, as GAME:"
        );
        // The long form disambiguates.
        assert_eq!(
            find(&dir, "GS3:Aelith").expect("find").map(|e| e.game_code),
            Some("GS3".to_string())
        );
    }

    #[test]
    fn a_broken_file_is_an_invalid_data_error() {
        let dir = dir("broken");
        std::fs::write(dir.join(FILE_NAME), "{ not json").expect("write");
        let error = load(&dir).expect_err("bad json");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn the_recovered_file_name_is_eleven_bytes() {
        assert_eq!(FILE_NAME, "roster.json");
        assert_eq!(FILE_NAME.len(), 11);
    }
}
