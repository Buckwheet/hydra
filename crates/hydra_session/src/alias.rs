//! Per-character command aliases.
//!
//! Recovered from `alias::{character, AliasFile::{expand, load, of, put,
//! remove}, Aliases::{expand, for_character, problem}}`:
//! `0x101aafcf8`, `0x101aaead4`, `0x101aae85c`, `0x101aae0c8`, `0x101aae1ac`,
//! `0x101aaf7e4`, `0x101aad79c`, `0x101aad5fc`, `0x101aadc60`.
//!
//! **ESTABLISHED**:
//! * the file is **JSON** (`<impl Deserialize for alias::AliasFile>::…` is in
//!   the symbol table, and `AliasFile::load` formats a `serde_json` error as
//!   `"{} cannot be read: {}"` plus the `"{}: {}"` template).
//! * the file is keyed by character: `character()` and `for_character()` both
//!   use the `{}_{}` template at `0x103e21bc4`, and `for_character` calls
//!   `hydra_net::gemstone::endpoint::instance` to learn the instance name, so
//!   the key is `<instance>_<character>`.
//! * `Aliases::expand` stats the file and only re-reads it when the mtime
//!   changed (`fs::metadata` → `Metadata::modified`, compared under a
//!   `Mutex`), then hands the line to `AliasFile::expand`.
//! * `AliasFile::expand` (`0x101aaead4`) parses the line with
//!   `steps::pieces`, sorts, and produces `format!("{} {}", a, b)`
//!   (`0x103e21840`) — an expansion is the alias's command with the rest of
//!   the line appended.
//! * `Aliases::problem` reports a bad alias with `"The alias {}: {}"`
//!   (`0x103e7cc01`), and the sent-line failure is the one shared with
//!   `command::run_steps`:
//!   `"The alias {}* has more than one command and no \\? for `{}`: it was not
//!   sent."` (`0x103e22a74`).  So an alias may hold **several** commands, and
//!   arguments may only be appended when there is exactly one command *or* the
//!   command carries a `?` placeholder.
//! * `AliasFile::put` mentions `"its target: {}"` (`0x103e7cbf2`) — the rest of
//!   that message (the part before it) is in an inlined helper.
//!
//! **UNKNOWN**: the JSON member names.  We write `schema` and `aliases`
//! (`{"name": ["command", …]}`); that is our schema, chosen to match the
//! "one name, several commands" behaviour the messages prove.

use crate::steps;
use crate::store;
use crate::textfmt::{self, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// The extension of an alias file.  **INFERRED** from `character_path`'s call
/// sites (the alias file is a JSON file named after the character).
pub const ALIAS_EXT: &str = ".json";
/// The schema this build writes.
pub const SCHEMA_VERSION: u32 = 1;
/// The placeholder that receives the rest of a typed line.
pub const PLACEHOLDER: char = '?';

/// `<instance>_<character>` — the key every alias file is named by.
///
/// **ESTABLISHED** template (`0x103e21bc4`).
pub fn character(instance: &str, character: &str) -> String {
    format!("{instance}_{character}")
}

/// The name an alias is stored under: the first word of the typed line, lower
/// cased so `.K rat` and `.k rat` are the same alias.
pub fn name_of(line: &str) -> &str {
    let trimmed = line.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(i) => &trimmed[..i],
        None => trimmed,
    }
}

/// The rest of a typed line, after the first word.
pub fn rest_of(line: &str) -> &str {
    let trimmed = line.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(i) => trimmed[i..].trim(),
        None => "",
    }
}

/// One character's aliases, plus where they live.
#[derive(Debug, Clone, PartialEq)]
pub struct AliasFile {
    /// alias name (lower case) to the commands it expands to
    pub aliases: BTreeMap<String, Vec<String>>,
    /// the file this was read from, if it came from one
    pub path: Option<PathBuf>,
    /// the mtime the file had when it was read
    pub loaded: Option<SystemTime>,
    /// the schema version that was read
    pub schema: u32,
}

impl Default for AliasFile {
    fn default() -> Self {
        AliasFile {
            aliases: BTreeMap::new(),
            path: None,
            loaded: None,
            schema: SCHEMA_VERSION,
        }
    }
}

impl AliasFile {
    /// An empty file for `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        AliasFile {
            aliases: BTreeMap::new(),
            path: Some(path.into()),
            loaded: None,
            schema: SCHEMA_VERSION,
        }
    }

    /// Read `path`.  A missing file is an empty alias set, not an error.
    ///
    /// **ESTABLISHED** message shapes: `"{} cannot be read: {}"` for the file
    /// and the parse error.
    pub fn load(path: &Path) -> Result<AliasFile, String> {
        let mut file = AliasFile::new(path);
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(file),
            Err(e) => return Err(format!("{} cannot be read: {e}", path.display())),
        };
        let v = textfmt::parse_json(&text)
            .map_err(|e| format!("{} cannot be read: {e}", path.display()))?;
        file.loaded = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        file.absorb(&v)?;
        Ok(file)
    }

    /// Read aliases out of an already parsed document.
    pub fn absorb(&mut self, v: &Value) -> Result<(), String> {
        self.schema = v.u64_of("schema").unwrap_or(0) as u32;
        let aliases = match v.get("aliases") {
            Some(Value::Obj(pairs)) => pairs,
            _ => return Ok(()),
        };
        for (name, target) in aliases {
            let commands: Vec<String> = match target {
                Value::Str(one) => vec![one.clone()],
                Value::Arr(items) => items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect(),
                _ => continue,
            };
            if commands.is_empty() {
                continue;
            }
            self.aliases.insert(name.to_lowercase(), commands);
        }
        Ok(())
    }

    /// The JSON text of the file.
    pub fn to_json(&self) -> String {
        let aliases: Vec<(String, Value)> = self
            .aliases
            .iter()
            .map(|(name, commands)| {
                (
                    name.clone(),
                    Value::Arr(commands.iter().cloned().map(Value::Str).collect()),
                )
            })
            .collect();
        textfmt::to_json(&Value::Obj(vec![
            ("schema".to_string(), Value::Num(self.schema as f64)),
            ("aliases".to_string(), Value::Obj(aliases)),
        ]))
    }

    /// Write the file through [`store::save_bytes`].
    pub fn save(&self) -> Result<(), String> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| "these aliases have no file".to_string())?;
        let dir = path.parent().unwrap_or(Path::new("."));
        store::save_bytes(dir, &path, self.to_json().as_bytes())
            .map_err(|e| format!("{} was not saved: {e}", path.display()))
    }

    /// The commands `name` expands to.
    ///
    /// **ESTABLISHED** shape (`0x101aae0c8`): a map lookup, no allocation.
    pub fn of(&self, name: &str) -> Option<&[String]> {
        self.aliases
            .get(&name.to_lowercase())
            .map(|v| v.as_slice())
    }

    /// Add or replace an alias.
    ///
    /// **ESTABLISHED** messages: the target is parsed with `steps::steps`, so
    /// `wait 2; go north` works, and a target with no commands is refused with
    /// `"{}: its target: {}"`.
    pub fn put(&mut self, name: &str, target: &str) -> Result<(), String> {
        let name = name.trim().to_lowercase();
        if name.is_empty() {
            return Err("an alias needs a name: its target: ".to_string() + target);
        }
        let commands: Vec<String> = steps::pieces(target);
        if commands.is_empty() {
            return Err(format!("{name}: its target: {target}"));
        }
        self.aliases.insert(name, commands);
        Ok(())
    }

    /// Remove an alias; returns whether it was there.
    pub fn remove(&mut self, name: &str) -> bool {
        self.aliases.remove(&name.to_lowercase()).is_some()
    }

    /// Expand a typed line (without its command symbol) through the aliases.
    ///
    /// * no alias for the first word → `Ok(None)`;
    /// * one command → that command with the rest of the line appended;
    /// * several commands → the rest of the line replaces every `?`, and if
    ///   there is no `?` the recovered error text is returned.
    pub fn expand(&self, line: &str) -> Result<Option<Vec<String>>, String> {
        let name = name_of(line);
        let rest = rest_of(line);
        let Some(commands) = self.of(name) else {
            return Ok(None);
        };
        if commands.len() == 1 {
            let single = &commands[0];
            let expanded = if single.contains(PLACEHOLDER) {
                single.replace(PLACEHOLDER, rest)
            } else if rest.is_empty() {
                single.clone()
            } else {
                format!("{single} {rest}")
            };
            return Ok(Some(steps::pieces(&expanded)));
        }
        if !commands.iter().any(|c| c.contains(PLACEHOLDER)) {
            return Err(format!(
                "The alias {name}* has more than one command and no \\? for `{rest}`: it was not \
                 sent."
            ));
        }
        let mut out = Vec::new();
        for command in commands {
            let filled = command.replace(PLACEHOLDER, rest);
            out.extend(steps::pieces(&filled));
        }
        Ok(Some(out))
    }
}

/// A character's alias file, reloaded when it changes on disk.
#[derive(Debug, Clone)]
pub struct Aliases {
    shared: Arc<Mutex<AliasFile>>,
    problem: Arc<Mutex<Option<String>>>,
}

impl Aliases {
    /// An in-memory alias set (no file).
    pub fn new() -> Self {
        Aliases {
            shared: Arc::new(Mutex::new(AliasFile::default())),
            problem: Arc::new(Mutex::new(None)),
        }
    }

    /// The alias set for `instance`/`character` under `root`.
    ///
    /// **ESTABLISHED** shape (`0x101aad5fc`): the instance name comes from the
    /// endpoint configuration and the file name is
    /// `<instance>_<character>.json`.
    pub fn for_character(root: &Path, instance: &str, character: &str) -> Self {
        let path = store::character_path(root, instance, character, ALIAS_EXT);
        let mut file = match &path {
            Some(p) => AliasFile::load(p).unwrap_or_else(|e| {
                let mut f = AliasFile::new(p.clone());
                f.schema = 0;
                // Keep the failure for `problem`, but stay usable.
                eprintln!("{e}");
                f
            }),
            None => AliasFile::default(),
        };
        if file.path.is_none() {
            file.path = path;
        }
        Aliases {
            shared: Arc::new(Mutex::new(file)),
            problem: Arc::new(Mutex::new(None)),
        }
    }

    /// Load the file if its mtime changed, then expand `line`.
    ///
    /// **ESTABLISHED** shape (`0x101aad79c`): stat, compare `modified()` with
    /// the cached value, reload under the lock, then expand.
    pub fn expand(&self, line: &str) -> Result<Option<Vec<String>>, String> {
        self.refresh();
        let file = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        file.expand(line)
    }

    /// Re-read the alias file when it changed on disk.
    pub fn refresh(&self) {
        let mut file = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let Some(path) = file.path.clone() else {
            return;
        };
        let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        if mtime == file.loaded {
            return;
        }
        match AliasFile::load(&path) {
            Ok(fresh) => {
                *file = fresh;
                *self.problem.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Err(e) => {
                *self.problem.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
            }
        }
    }

    /// The aliases themselves.
    pub fn file(&self) -> std::sync::MutexGuard<'_, AliasFile> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The last problem the file had, if any.
    ///
    /// **ESTABLISHED** shape (`0x101aadc60`): returns an `Option<String>` and
    /// formats it as `"The alias {}: {}"`.
    pub fn problem(&self) -> Option<String> {
        self.problem
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Add an alias and write the file.
    pub fn put(&self, name: &str, target: &str) -> Result<(), String> {
        let mut file = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        file.put(name, target)?;
        file.save()
    }

    /// Remove an alias and write the file.
    pub fn remove(&self, name: &str) -> Result<bool, String> {
        let mut file = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let had = file.remove(name);
        if had {
            file.save()?;
        }
        Ok(had)
    }
}

impl Default for Aliases {
    fn default() -> Self {
        Aliases::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(aliases: &[(&str, &[&str])]) -> AliasFile {
        let mut f = AliasFile::default();
        for (name, commands) in aliases {
            f.aliases.insert(
                (*name).to_string(),
                commands.iter().map(|c| (*c).to_string()).collect(),
            );
        }
        f
    }

    #[test]
    fn character_key_uses_the_recovered_template() {
        assert_eq!(character("gemstone", "Aelith"), "gemstone_Aelith");
    }

    #[test]
    fn name_and_rest_split_on_the_first_space() {
        assert_eq!(name_of("  k rat  "), "k");
        assert_eq!(rest_of("  k rat  "), "rat");
        assert_eq!(name_of("k"), "k");
        assert_eq!(rest_of("k"), "");
    }

    #[test]
    fn a_single_command_takes_the_whole_line() {
        let f = with(&[("k", &["kill"])]);
        assert_eq!(f.expand("k rat").unwrap(), Some(vec!["kill rat".to_string()]));
        assert_eq!(f.expand("K").unwrap(), Some(vec!["kill".to_string()]));
        assert_eq!(f.expand("z").unwrap(), None);
    }

    #[test]
    fn several_commands_need_a_placeholder() {
        let f = with(&[("pr", &["prepare", "stance offensive", "kill ?"])]);
        assert_eq!(
            f.expand("pr rat").unwrap(),
            Some(vec![
                "prepare".to_string(),
                "stance offensive".to_string(),
                "kill rat".to_string()
            ])
        );
        let g = with(&[("pr", &["prepare", "kill"])]);
        let e = g.expand("pr rat").unwrap_err();
        assert_eq!(
            e,
            "The alias pr* has more than one command and no \\? for `rat`: it was not sent."
        );
    }

    #[test]
    fn put_parses_the_target_into_pieces() {
        let mut f = AliasFile::default();
        f.put("GOn", "go north; wait 2").unwrap();
        assert_eq!(f.of("gon").unwrap(), ["go north", "wait 2"]);
        let e = f.put("empty", "   ").unwrap_err();
        assert_eq!(e, "empty: its target:    ");
        assert!(f.remove("GON"));
        assert!(!f.remove("gon"));
    }

    #[test]
    fn the_file_round_trips_through_json() {
        let mut f = AliasFile::default();
        f.put("k", "kill ?").unwrap();
        f.put("gn", "go north").unwrap();
        let text = f.to_json();
        assert!(text.contains("\"k\""));
        let v = textfmt::parse_json(&text).unwrap();
        let mut back = AliasFile::default();
        back.absorb(&v).unwrap();
        assert_eq!(back.aliases, f.aliases);
        assert_eq!(back.schema, SCHEMA_VERSION);
    }

    #[test]
    fn a_missing_file_is_an_empty_alias_set() {
        let p = std::env::temp_dir().join("hydra-aliases-missing.json");
        let _ = std::fs::remove_file(&p);
        assert!(AliasFile::load(&p).unwrap().aliases.is_empty());
    }

    #[test]
    fn aliases_expand_through_the_shared_handle() {
        let a = Aliases::new();
        a.put("k", "kill ?").unwrap_or(());
        // no file, so put() cannot save; set it directly instead
        a.file().aliases.insert("k".into(), vec!["kill ?".into()]);
        assert_eq!(
            a.expand("k rat").unwrap(),
            Some(vec!["kill rat".to_string()])
        );
        assert_eq!(a.problem(), None);
    }
}
