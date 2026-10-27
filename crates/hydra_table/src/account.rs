//! Account passwords and the account data directory.
//!
//! Recovered from `hydra_table::account` (`0x1011f70f0` … `0x1011f7b68`).
//!
//! Two literals live next to each other in `__TEXT` and are used here:
//! `HYDRA_DATA_DIR` (14 bytes at `0x10240740e`) and `HYDRA_PASSWORD_`
//! (the 15-byte piece of the format string at `0x103e613d0`).
//!
//! The original kept passwords in the **OS keychain** (`keyring::Entry::new`
//! with the 5-byte service name `hydra` at `0x102407409`, entry
//! `0x10139d07c`).  This build is `std`-only, so [`keep`], [`kept`] and
//! [`forget`] use a process-wide map *and* the `HYDRA_PASSWORD_<ACCOUNT>`
//! environment variable.  See `NOTES.md` for the full list of departures.

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::roster;

/// The variable that overrides the account data directory.
///
/// **ESTABLISHED**: the 14-byte literal at `0x10240740e`, read with
/// `std::env::_var_os` in [`data_dir`] (`0x1011f79dc`) and in
/// `account::from_roster` (`0x1011f70f0`).
pub const DATA_DIR_ENV: &str = "HYDRA_DATA_DIR";

/// The directory used when [`DATA_DIR_ENV`] is unset.
///
/// **ESTABLISHED**: the fallback allocates exactly 4 bytes and stores
/// `0x61746164` = `"data"` (`mov w8, #0x6164` / `movk w8, #0x6174, lsl #16` at
/// `0x1011f7a28`).
pub const DEFAULT_DATA_DIR: &str = "data";

/// The keychain service name the original used.
///
/// **ESTABLISHED**: 5 bytes at `0x102407409`, passed as the first argument of
/// `keyring::Entry::new`.
pub const KEYCHAIN_SERVICE: &str = "hydra";

/// The account data directory: `$HYDRA_DATA_DIR`, or `data`.
///
/// **ESTABLISHED** (`0x1011f79dc`): the environment value is returned verbatim
/// (it is not joined or canonicalised) and the fallback is `data`.
pub fn data_dir() -> PathBuf {
    match std::env::var_os(DATA_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(DEFAULT_DATA_DIR),
    }
}

/// `HYDRA_PASSWORD_<ACCOUNT>` for `account`.
///
/// **ESTABLISHED** (`0x1011f7a6c`): `trim_matches`, then a `Map` over the
/// characters that upper-cases `a`-`z`, keeps ASCII alphanumerics and replaces
/// anything else with `_`, formatted into `HYDRA_PASSWORD_{}`.
///
/// The character mapping is shared with
/// [`hydra_session::password::env_name`], which was recovered from this very
/// function; this crate calls it so the two can never drift apart.
pub fn env_name(account: &str) -> String {
    hydra_session::password::env_name(account)
}

/// The password kept for `account`, if any.
///
/// **ESTABLISHED** (`0x1011f7568`): trim the account, lower-case it, ask
/// `keyring::Entry::new("hydra", &lower).get_password()`, and — when that
/// yields nothing — fall back to `env::var(env_name(account))`.  An empty
/// password counts as "not kept" in both places.
///
/// **Departure**: the keychain lookup is replaced by the in-process map filled
/// by [`keep`]; the environment fallback is the original's own second step.
pub fn kept(account: &str) -> Option<String> {
    if let Some(password) = store().lock().unwrap().get(&entry_name(account)).cloned() {
        if !password.is_empty() {
            return Some(password);
        }
    }
    std::env::var(env_name(account))
        .ok()
        .filter(|password| !password.is_empty())
}

/// Whether a password is kept for `account`.
///
/// **ESTABLISHED** (`0x1011f777c`): `kept(account).is_some()`.
pub fn saved(account: &str) -> bool {
    kept(account).is_some()
}

/// Keep `password` for `account`.
///
/// **ESTABLISHED** (`0x1011f7370`): the account is trimmed and lower-cased and
/// becomes the keychain entry's *user* name, with the service name `hydra`;
/// `keyring::Entry::set_password` then stores the password.  Every failure is
/// returned as `Err(String)` holding the `Display` of the keyring error, and
/// success is `Ok(())` (encoded as `isize::MIN` in the cap slot).
///
/// **Departure**: no OS keychain, so the password is kept in memory for the
/// life of the process.
pub fn keep(account: &str, password: &str) -> Result<(), String> {
    store()
        .lock()
        .unwrap()
        .insert(entry_name(account), password.to_string());
    Ok(())
}

/// Forget the password kept for `account`.
///
/// **ESTABLISHED** (`0x1011f77d0`): same entry as [`keep`],
/// `keyring::Entry::delete_credential`, errors formatted with `Display` into a
/// `String`.
///
/// **UNKNOWN**: the keyring crate's `NoEntry` message; this build says
/// `nothing is kept for <account>` instead.
pub fn forget(account: &str) -> Result<(), String> {
    match store().lock().unwrap().remove(&entry_name(account)) {
        Some(_) => Ok(()),
        None => Err(format!("nothing is kept for {account}")),
    }
}

/// The password for `account`: the kept one, or a prompt on the terminal.
///
/// **ESTABLISHED** (`0x1011f7b68`): `kept(account)` first; otherwise
/// `format!("Password for account {}: ", account)` — the 21-byte piece at
/// `0x103e613e2` — is handed to `rpassword::prompt_password`, whose
/// `io::Result<String>` is returned unchanged.
///
/// **Departure**: `rpassword` is not available, so the prompt is written to
/// stderr and one line is read from stdin **with echo** (the terminal cannot be
/// put into raw mode without a dependency).  `Ok` is returned with the trailing
/// line terminator removed; I/O failures become `Err`.
pub fn password(account: &str) -> io::Result<String> {
    if let Some(password) = kept(account) {
        return Ok(password);
    }
    let prompt = hydra_session::password::prompt(account);
    let mut stderr = io::stderr();
    stderr.write_all(prompt.as_bytes())?;
    stderr.flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }
    Ok(line)
}

/// The roster entry that names `account`, as `(character, account)`.
///
/// **ESTABLISHED** (`0x1011f70f0`): `data_dir()` then `roster::find(dir,
/// account)`; the found entry's `character` and `account` strings are returned
/// in that order.  A missing entry is `Ok(None)`, and an I/O failure from the
/// roster is flattened to `Err(error.to_string())` (`<io::Error as
/// Display>::fmt` at `0x1011f71d0`), which is why the error type is `String`.
///
/// **INFERRED**: the tuple order follows the recovered `Entry` field order
/// (`character` at `+0x00`, `account` at `+0x18`); the two strings are copied
/// from exactly those two fields.
pub fn from_roster(account: &str) -> Result<Option<(String, String)>, String> {
    match roster::find(&data_dir(), account) {
        Ok(Some(entry)) => Ok(Some((entry.character, entry.account))),
        Ok(None) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// The normalised key used for the password store: `trim().to_lowercase()`.
///
/// **ESTABLISHED**: every keychain call in `account` keys on
/// `account.trim().to_lowercase()` (`<str>::to_lowercase` right after
/// `trim_matches` in all four functions).
fn entry_name(account: &str) -> String {
    account.trim().to_lowercase()
}

fn store() -> &'static Mutex<BTreeMap<String, String>> {
    static KEPT: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_directory_defaults_to_data() {
        // The literal is exactly four bytes and spells "data".
        assert_eq!(DEFAULT_DATA_DIR, "data");
        assert_eq!(DEFAULT_DATA_DIR.len(), 4);
        assert_eq!(DATA_DIR_ENV, "HYDRA_DATA_DIR");
        assert_eq!(DATA_DIR_ENV.len(), 14, "the recovered piece is 14 bytes");
        assert_eq!(KEYCHAIN_SERVICE, "hydra");
        assert_eq!(KEYCHAIN_SERVICE.len(), 5, "the recovered piece is 5 bytes");
    }

    #[test]
    fn the_data_directory_follows_the_environment() {
        let dir = temp_dir("env");
        std::env::set_var(DATA_DIR_ENV, &dir);
        assert_eq!(data_dir(), dir);
        std::env::remove_var(DATA_DIR_ENV);
        assert_eq!(data_dir(), PathBuf::from("data"));
    }

    #[test]
    fn the_variable_name_is_the_session_one() {
        assert_eq!(env_name("Aelith"), "HYDRA_PASSWORD_AELITH");
        assert_eq!(env_name(" gem-stone "), "HYDRA_PASSWORD_GEM_STONE");
    }

    #[test]
    fn a_kept_password_is_saved_and_forgotten() {
        let account = "kept-test";
        assert!(!saved(account));
        keep(account, "hunter2").expect("keep");
        assert!(saved(account));
        assert_eq!(kept(account).as_deref(), Some("hunter2"));
        // The key is the trimmed, lower-cased account.
        assert_eq!(kept(" KEPT-TEST ").as_deref(), Some("hunter2"));
        forget(account).expect("forget");
        assert!(!saved(account));
        assert!(forget(account).is_err(), "nothing left to forget");
    }

    #[test]
    fn an_empty_password_does_not_count_as_saved() {
        keep("empty-test", "").expect("keep");
        assert!(!saved("empty-test"));
        assert_eq!(kept("empty-test"), None);
        forget("empty-test").expect("forget");
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_table_account_{name}"));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
