//! `teams.json` — the teams of accounts that play together.
//!
//! Recovered from `hydra_table::teams` (`0x101212e34` … `0x1012148dc`).  Same
//! shape as `roster.json`, and the same real `serde`/`serde_json` document:
//! `{"schema_version":1,"teams":{…}}`, with the key `name.trim().to_lowercase()`.
//!
//! A team is a **set of accounts**, each of which brings one character, and no
//! two members may share an account — "… are on one account, which plays one at
//! a time." (`0x103e61a97`).

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::roster;

/// The file name inside the data directory.
///
/// **ESTABLISHED**: the 10-byte literal at `0x10240839c` (`"teams.json"`),
/// joined in `teams::load` (`mov w3, #0xa` at `0x101213b94`).
pub const FILE_NAME: &str = "teams.json";

/// The schema version written by [`save`].
///
/// **INFERRED**: `teams::save` uses the same `schema_version` field and the same
/// `serialize_entry` shape as `roster::save`, which is established as `1`
/// (`0x10129cf68`).
pub const SCHEMA_VERSION: u32 = 1;

/// One team.
///
/// **ESTABLISHED** layout (from `teams::keep` `0x101213038`, `teams::find`
/// `0x101212f30`, size `0x38`):
///
/// | offset | field |
/// | --- | --- |
/// | `+0x00` | `name: String` (read at `[team, #0x8]` = its pointer) |
/// | `+0x18` | `members: Vec<String>` (pointer `+0x20`, length `+0x28`) |
/// | `+0x30` | `favourite: bool` (`strb w9, [x19, #0x30]`) |
///
/// The serde name `members` survives in the `expecting` string at
/// `0x102407440`; `name` and `favourite` are read from the same declaration
/// order `roster::Entry` uses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub name: String,
    /// Written in list order; a file without it reads as an empty team.
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub favourite: bool,
}

/// The whole `teams.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    /// Written first, and stamped to [`SCHEMA_VERSION`] by [`save`].
    #[serde(default)]
    pub schema_version: u32,
    /// Written second, in `BTreeMap` (key) order.
    #[serde(default)]
    pub teams: BTreeMap<String, Team>,
}

impl Team {
    /// The map key for this team: `name.trim().to_lowercase()`.
    ///
    /// **ESTABLISHED** (`teams::find` `0x101212f0c`): `trim_matches` then
    /// `<str>::to_lowercase` before the map lookup.
    pub fn key(&self) -> String {
        key(&self.name)
    }
}

/// `name.trim().to_lowercase()` — the key of a team.
pub fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The process-wide guard for `teams.json`.
///
/// **ESTABLISHED**: `teams::keep` (`0x101213530`) and `teams::forget`
/// (`0x101214144`) lock a second `LazyLock<Mutex<()>>`-style `OnceBox` at
/// `0x10458ea30` — distinct from the roster's `0x10458ea48`.
static TEAMS: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Read `dir/teams.json`.
///
/// **ESTABLISHED** (`teams::load`, `0x101213b60`): identical to
/// `roster::load` — `NotFound` yields the default file, other I/O errors pass
/// through, JSON errors become `InvalidData`.
pub fn load(dir: &Path) -> Result<File, io::Error> {
    let path = dir.join(FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(File::default()),
        Err(error) => return Err(error),
    };
    serde_json::from_str::<File>(&text).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", path.display(), error),
        )
    })
}

/// Write `dir/teams.json`, creating the directory first.
pub fn save(dir: &Path, file: &mut File) -> Result<(), io::Error> {
    file.schema_version = SCHEMA_VERSION;
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FILE_NAME);
    let text = serde_json::to_string(file).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", path.display(), error),
        )
    })?;
    std::fs::write(path, text)
}

/// The serialised form of `file`.
///
/// **ESTABLISHED**: the recovered writer is `serde_json::to_string(&file)` —
/// `{"schema_version":N,"teams":{…}}`, teams in `BTreeMap` order, `serde_json`'s
/// escapes.  `File` is plain data, so the serialisation cannot fail; the
/// `Result` is flattened rather than panicking.
pub fn to_json(file: &File) -> String {
    serde_json::to_string(file).unwrap_or_default()
}

/// Every team, ordered by key.
///
/// **ESTABLISHED** (`teams::all`, `0x101212e34`): `load(dir)` then
/// `file.teams.into_iter().map(|(_, v)| v).collect()`; errors pass through with
/// `isize::MIN` in the cap slot.
pub fn all(dir: &Path) -> Result<Vec<Team>, io::Error> {
    let file = load(dir)?;
    Ok(file.teams.into_values().collect())
}

/// The team called `name`.
///
/// **ESTABLISHED** (`teams::find`, `0x101212eb0`): `load(dir)` and then
/// `BTreeMap::remove(&name.trim().to_lowercase())`, returning the removed value
/// as `Option<Team>` (`isize::MIN` cap = `None`).  The file is *not* written
/// back, so the removal is unobservable — this is what the instruction stream
/// does, and it is equivalent to `get(...).cloned()`.
pub fn find(dir: &Path, name: &str) -> Result<Option<Team>, io::Error> {
    let mut file = load(dir)?;
    Ok(file.teams.remove(&key(name)))
}

/// Keep `team`, after checking that it can be played.
///
/// **ESTABLISHED** (`teams::keep`, `0x101213008`) — the messages are all in
/// `__TEXT` at `0x102408366` … `0x103e61b04`:
///
/// * `team.name.trim()` empty → `Err("A team needs a name.")` (20 bytes),
/// * the name holding `:` → `Err("A team's name cannot hold a colon.")`
///   (`memchr` for `0x3a` at `0x1012130cc`),
/// * no members → `Err(format!("{} needs a character.", name))`
///   (`ldr x8, [team, #0x28]; cbz` at `0x10121313c`),
/// * every member is resolved with `roster::find(dir, member)`: an I/O error
///   becomes `Err(error.to_string())`, a miss becomes
///   `Err(format!("{} is not on the roster.", member))`,
/// * two members on the same account become
///   `Err(format!("{} and {} are on one account, which plays one at a time.",
///   first.character, second.character))`,
/// * an existing team of that name is `Err(format!("A team called {} is kept
///   already.", name))`, otherwise it is inserted, the file saved, and the
///   answer is `Ok(format!("{} is kept.", name))`.
///
/// The original's return type is a two-variant enum with an explicit tag word
/// (`mov w8, #1; str x8, [x19]` at `0x1012133f0` for the error arm, `0` for the
/// answer), i.e. `Result<String, String>`.
///
/// **UNKNOWN**: the second parameter is an `Option<&str>` (`csel x2, xzr, x22,
/// eq` on the `isize::MIN` length slot in `Table::answer`).  Nothing in `keep`
/// reads it, so it is accepted and ignored here.
pub fn keep(dir: &Path, _current: Option<&str>, team: &Team) -> Result<String, String> {
    let name = team.name.trim();
    if name.is_empty() {
        return Err("A team needs a name.".to_string());
    }
    if name.contains(':') {
        return Err("A team's name cannot hold a colon.".to_string());
    }
    if team.members.is_empty() {
        return Err(format!("{name} needs a character."));
    }
    let mut resolved: Vec<roster::Entry> = Vec::new();
    for member in &team.members {
        let member = member.trim();
        if member.is_empty() {
            continue;
        }
        match roster::find(dir, member) {
            Ok(Some(entry)) => {
                for other in &resolved {
                    if other.account.eq_ignore_ascii_case(&entry.account) {
                        return Err(format!(
                            "{} and {} are on one account, which plays one at a time.",
                            other.character, entry.character
                        ));
                    }
                }
                resolved.push(entry);
            }
            Ok(None) => return Err(format!("{member} is not on the roster.")),
            Err(error) => return Err(error.to_string()),
        }
    }
    let _guard = TEAMS.lock().unwrap();
    let mut file = load(dir).map_err(|error| error.to_string())?;
    let at = key(name);
    if file.teams.contains_key(&at) {
        return Err(format!("A team called {name} is kept already."));
    }
    file.teams.insert(
        at,
        Team {
            name: name.to_string(),
            members: team.members.clone(),
            favourite: team.favourite,
        },
    );
    save(dir, &mut file).map_err(|error| error.to_string())?;
    Ok(format!("{name} is kept."))
}

/// Forget the team called `name`.
///
/// **ESTABLISHED** (`teams::forget`, `0x10121410c`): lock, `load`, remove by
/// [`key`]; a missing team answers `Ok(format!("No team is called {}.",
/// name))` (the 20-byte piece at `0x103e2281e`), a removed one is saved and
/// answers `Ok(format!("{} is forgotten.", name))` (`0x103e61b00`).  Only the
/// load/save failures are `Err(io::Error)`.
pub fn forget(dir: &Path, name: &str) -> Result<String, io::Error> {
    let _guard = TEAMS.lock().unwrap();
    let mut file = load(dir)?;
    if file.teams.remove(&key(name)).is_none() {
        return Ok(format!("No team is called {name}."));
    }
    save(dir, &mut file)?;
    Ok(format!("{name} is forgotten."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_table_teams_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn roster_entry(character: &str, account: &str) -> roster::Entry {
        roster::Entry {
            character: character.to_string(),
            account: account.to_string(),
            game_code: "GS4".to_string(),
            favourite: false,
            nickname: None,
        }
    }

    fn team(name: &str, members: &[&str]) -> Team {
        Team {
            name: name.to_string(),
            members: members.iter().map(|m| m.to_string()).collect(),
            favourite: false,
        }
    }

    #[test]
    fn the_key_is_lower_cased_and_trimmed() {
        assert_eq!(key(" The Crew "), "the crew");
    }

    #[test]
    fn a_team_needs_a_name() {
        let dir = dir("nameless");
        assert_eq!(
            keep(&dir, None, &team("  ", &["main"])).unwrap_err(),
            "A team needs a name."
        );
    }

    #[test]
    fn a_name_cannot_hold_a_colon() {
        let dir = dir("colon");
        let error = keep(&dir, None, &team("GS4:crew", &["main"])).unwrap_err();
        assert_eq!(error, "A team's name cannot hold a colon.");
    }

    #[test]
    fn a_team_needs_a_character() {
        let dir = dir("empty");
        let error = keep(&dir, None, &team("crew", &[])).unwrap_err();
        assert_eq!(error, "crew needs a character.");
    }

    #[test]
    fn a_member_must_be_on_the_roster() {
        let dir = dir("stranger");
        let error = keep(&dir, None, &team("crew", &["nobody"])).unwrap_err();
        assert_eq!(error, "nobody is not on the roster.");
    }

    #[test]
    fn keeping_and_forgetting_round_trips() {
        let dir = dir("round_trip");
        roster::record(&dir, &roster_entry("Aelith", "alpha")).expect("record");
        roster::record(&dir, &roster_entry("Wynter", "beta")).expect("record");
        let crew = team("Crew", &["alpha", "beta"]);
        assert_eq!(keep(&dir, None, &crew), Ok("Crew is kept.".to_string()));
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE_NAME)).expect("file"),
            r#"{"schema_version":1,"teams":{"crew":{"name":"Crew","members":["alpha","beta"],"favourite":false}}}"#
        );
        assert_eq!(all(&dir).expect("all"), vec![crew.clone()]);
        assert_eq!(find(&dir, "CREW").expect("find"), Some(crew.clone()));
        // A second team of the same name is refused.
        assert_eq!(
            keep(&dir, None, &crew).unwrap_err(),
            "A team called Crew is kept already."
        );
        assert_eq!(forget(&dir, "crew").expect("forget"), "crew is forgotten.");
        assert_eq!(
            forget(&dir, "crew").expect("forget"),
            "No team is called crew."
        );
        assert!(all(&dir).expect("all").is_empty());
    }

    #[test]
    fn a_missing_file_is_an_empty_file() {
        let file = load(&dir("missing")).expect("load");
        assert_eq!(file, File::default());
    }

    #[test]
    fn the_file_round_trips_through_serde() {
        let mut file = File {
            schema_version: SCHEMA_VERSION,
            teams: BTreeMap::new(),
        };
        for crew in [
            Team {
                name: "Crew".to_string(),
                members: vec!["alpha".to_string(), "beta".to_string()],
                favourite: true,
            },
            Team {
                name: "Alts \"and\" mains".to_string(),
                members: Vec::new(),
                favourite: false,
            },
        ] {
            file.teams.insert(crew.key(), crew);
        }
        let text = to_json(&file);
        assert_eq!(
            text,
            concat!(
                r#"{"schema_version":1,"teams":{"#,
                r#""alts \"and\" mains":{"name":"Alts \"and\" mains","members":[],"favourite":false},"#,
                r#""crew":{"name":"Crew","members":["alpha","beta"],"favourite":true}}}"#,
            )
        );
        // `BTreeMap` key order, whatever the insertion order was.
        let alts = text.find("alts").expect("alts");
        let crew = text.find("crew").expect("crew");
        assert!(alts < crew);
        let back: File = serde_json::from_str(&text).expect("read back");
        assert_eq!(back, file);
    }

    #[test]
    fn a_saved_file_is_the_recovered_document() {
        let dir = dir("serde_save");
        roster::record(&dir, &roster_entry("Aelith", "alpha")).expect("record");
        let mut file = File {
            schema_version: 0,
            teams: BTreeMap::new(),
        };
        file.teams
            .insert("crew".to_string(), team("Crew", &["alpha", "beta"]));
        save(&dir, &mut file).expect("save");
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE_NAME)).expect("file"),
            r#"{"schema_version":1,"teams":{"crew":{"name":"Crew","members":["alpha","beta"],"favourite":false}}}"#
        );
        // A broken file is an `InvalidData` error naming the path, exactly as
        // `roster::load` does.
        std::fs::write(dir.join(FILE_NAME), "{\"teams\": 3}").expect("write");
        let error = load(&dir).expect_err("bad shape");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error
            .to_string()
            .starts_with(&format!("{}: ", dir.join(FILE_NAME).display())));
    }

    #[test]
    fn the_file_name_is_ten_bytes() {
        assert_eq!(FILE_NAME, "teams.json");
        assert_eq!(FILE_NAME.len(), 10);
    }
}
