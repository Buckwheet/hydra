//! Account passwords and the account data directory.
//!
//! Recovered from `hydra_table::account` (`0x1011f70f0` … `0x1011f7b68`).
//!
//! Two literals live next to each other in `__TEXT` and are used here:
//! `HYDRA_DATA_DIR` (14 bytes at `0x10240740e`) and `HYDRA_PASSWORD_`
//! (the 15-byte piece of the format string at `0x103e613d0`).
//!
//! The original kept passwords in the **OS keychain**: `keyring::Entry::new`
//! with the 5-byte service name `hydra` at `0x102407409` (entry
//! `0x10139d07c`), and asked for a missing one with `rpassword::prompt_password`
//! (`0x1011f7b68`).  Both crates are back: [`keep`]/[`kept`]/[`forget`] are the
//! real keychain calls and [`password`] is the real no-echo prompt.  The
//! `HYDRA_PASSWORD_<ACCOUNT>` environment variable stays as the original's own
//! second step.  See `NOTES.md` §5.

use std::io;
use std::path::PathBuf;

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
/// The keychain is the real one again.  A keychain that cannot be opened at all
/// (`Entry::new`/`get_password` erroring) is treated the same way the original
/// treats `NoEntry`: the environment is asked next.
pub fn kept(account: &str) -> Option<String> {
    if let Ok(entry) = entry(account) {
        if let Ok(password) = entry.get_password() {
            if !password.is_empty() {
                return Some(password);
            }
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
pub fn keep(account: &str, password: &str) -> Result<(), String> {
    entry(account)
        .and_then(|entry| entry.set_password(password))
        .map_err(|error| error.to_string())
}

/// Forget the password kept for `account`.
///
/// **ESTABLISHED** (`0x1011f77d0`): same entry as [`keep`],
/// `keyring::Entry::delete_credential`, errors formatted with `Display` into a
/// `String` — so forgetting an account that has no password reports the
/// keyring's own `NoEntry` text ("No matching entry found in secure storage")
/// instead of the placeholder the earlier stand-in printed.
pub fn forget(account: &str) -> Result<(), String> {
    entry(account)
        .and_then(|entry| entry.delete_credential())
        .map_err(|error| error.to_string())
}

/// The password for `account`: the kept one, or a prompt on the terminal.
///
/// **ESTABLISHED** (`0x1011f7b68`): `kept(account)` first; otherwise
/// `format!("Password for account {}: ", account)` — the 21-byte piece at
/// `0x103e613e2` — is handed to `rpassword::prompt_password`, whose
/// `io::Result<String>` is returned unchanged.
///
/// **No departure left**: the prompt is `rpassword::prompt_password`, which puts
/// the terminal into raw mode, writes the prompt to the TTY and reads the line
/// **without echo**.  The prompt text itself comes from
/// [`hydra_session::password::prompt`], the crate that recovered the 21-byte
/// literal, so the two cannot drift apart.
pub fn password(account: &str) -> io::Result<String> {
    if let Some(password) = kept(account) {
        return Ok(password);
    }
    rpassword::prompt_password(hydra_session::password::prompt(account))
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

/// The normalised key used for the password entry: `trim().to_lowercase()`.
///
/// **ESTABLISHED**: every keychain call in `account` keys on
/// `account.trim().to_lowercase()` (`<str>::to_lowercase` right after
/// `trim_matches` in all four functions).
fn entry_name(account: &str) -> String {
    account.trim().to_lowercase()
}

/// The keychain entry for `account`, under the recovered service name.
///
/// **ESTABLISHED** (`0x1011f7568`, `0x1011f7370`, `0x1011f77d0`): every one of
/// the three keychain functions builds `keyring::Entry::new(hydra, <lower>)`
/// before calling `get_password`/`set_password`/`delete_credential`.
fn entry(account: &str) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &entry_name(account))
}

/// An in-memory stand-in for the OS keychain, for tests only.
///
/// `keyring` builds entries with whichever credential store its features select,
/// so a test that ran against the default builder would write real credentials
/// into the user's Keychain.  Every test that touches a password therefore
/// installs this builder (once, before any entry exists) and holds the returned
/// guard, which also serialises those tests and starts each one from an empty
/// store.  Entries are keyed exactly as the real keychain keys them —
/// `(service, user)` — so [`KEYCHAIN_SERVICE`] and the account normalisation are
/// still what the assertions check.  Nothing here can block on a terminal.
#[cfg(test)]
pub(crate) mod test_keychain {
    use std::any::Any;
    use std::collections::BTreeMap;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use keyring::credential::{
        Credential, CredentialApi, CredentialBuilderApi, CredentialPersistence,
    };
    use keyring::{Error as KeyringError, Result as KeyringResult};

    pub(crate) fn store() -> &'static Mutex<BTreeMap<(String, String), String>> {
        static STORE: OnceLock<Mutex<BTreeMap<(String, String), String>>> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    /// Install the in-memory store and clear it; hold the guard for the test.
    pub(crate) fn install() -> MutexGuard<'static, ()> {
        static GUARD: Mutex<()> = Mutex::new(());
        static INSTALLED: OnceLock<()> = OnceLock::new();
        let guard = GUARD.lock().unwrap_or_else(|error| error.into_inner());
        INSTALLED.get_or_init(|| {
            keyring::set_default_credential_builder(Box::new(TestBuilder));
        });
        store().lock().unwrap().clear();
        guard
    }

    /// The password the store holds for `(service, user)`, if any.
    pub(crate) fn password(service: &str, user: &str) -> Option<String> {
        store()
            .lock()
            .unwrap()
            .get(&(service.to_string(), user.to_string()))
            .cloned()
    }

    struct TestCredential {
        service: String,
        user: String,
    }

    impl TestCredential {
        fn key(&self) -> (String, String) {
            (self.service.clone(), self.user.clone())
        }
    }

    impl CredentialApi for TestCredential {
        fn set_secret(&self, secret: &[u8]) -> KeyringResult<()> {
            let password = String::from_utf8(secret.to_vec())
                .map_err(|error| KeyringError::BadEncoding(error.into_bytes()))?;
            store().lock().unwrap().insert(self.key(), password);
            Ok(())
        }

        fn get_secret(&self) -> KeyringResult<Vec<u8>> {
            store()
                .lock()
                .unwrap()
                .get(&self.key())
                .map(|password| password.as_bytes().to_vec())
                .ok_or(KeyringError::NoEntry)
        }

        fn delete_credential(&self) -> KeyringResult<()> {
            match store().lock().unwrap().remove(&self.key()) {
                Some(_) => Ok(()),
                None => Err(KeyringError::NoEntry),
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    struct TestBuilder;

    impl CredentialBuilderApi for TestBuilder {
        fn build(
            &self,
            _target: Option<&str>,
            service: &str,
            user: &str,
        ) -> KeyringResult<Box<Credential>> {
            Ok(Box::new(TestCredential {
                service: service.to_string(),
                user: user.to_string(),
            }))
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn persistence(&self) -> CredentialPersistence {
            CredentialPersistence::UntilDelete
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_keychain::install;
    use super::*;

    use keyring::Error as KeyringError;

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
        let _keychain = install();
        let account = "kept-test";
        assert!(!saved(account));
        keep(account, "hunter2").expect("keep");
        // The entry is the recovered one: service `hydra`, user
        // `account.trim().to_lowercase()`.
        assert_eq!(
            super::test_keychain::password(KEYCHAIN_SERVICE, "kept-test"),
            Some("hunter2".to_string())
        );
        assert!(saved(account));
        assert_eq!(kept(account).as_deref(), Some("hunter2"));
        // The key is the trimmed, lower-cased account.
        assert_eq!(kept(" KEPT-TEST ").as_deref(), Some("hunter2"));
        forget(account).expect("forget");
        assert!(!saved(account));
        let error = forget(account).expect_err("nothing left to forget");
        // `forget` reports the keyring's own message now (`Display` of the
        // error, as the recovered `Err(String)` does).
        assert_eq!(error, KeyringError::NoEntry.to_string());
    }

    #[test]
    fn each_account_keeps_its_own_password() {
        let _keychain = install();
        keep("main", "one").expect("keep");
        keep("alt", "two").expect("keep");
        assert_eq!(kept("main").as_deref(), Some("one"));
        assert_eq!(kept("alt").as_deref(), Some("two"));
        forget("main").expect("forget");
        assert_eq!(kept("main"), None);
        assert_eq!(kept("alt").as_deref(), Some("two"));
    }

    #[test]
    fn an_empty_password_does_not_count_as_saved() {
        let _keychain = install();
        keep("empty-test", "").expect("keep");
        assert!(!saved("empty-test"));
        assert_eq!(kept("empty-test"), None);
        forget("empty-test").expect("forget");
    }

    #[test]
    fn the_environment_is_asked_when_the_keychain_holds_nothing() {
        let _keychain = install();
        let account = "env-test-account";
        let name = env_name(account);
        // The keychain is asked first...
        keep(account, "from-keychain").expect("keep");
        std::env::set_var(&name, "from-env");
        assert_eq!(kept(account).as_deref(), Some("from-keychain"));
        // ...and the environment is the fallback.
        forget(account).expect("forget");
        assert_eq!(kept(account).as_deref(), Some("from-env"));
        // An empty variable is not a password either.
        std::env::set_var(&name, "");
        assert_eq!(kept(account), None);
        std::env::remove_var(&name);
    }

    #[test]
    fn a_kept_password_answers_the_prompt_without_a_terminal() {
        let _keychain = install();
        keep("prompt-test", "hunter2").expect("keep");
        // `kept` answers first, so `password` never reaches `rpassword` and
        // cannot block on a terminal in a test.
        assert_eq!(password("prompt-test").expect("password"), "hunter2");
    }

    #[test]
    fn the_prompt_is_rpasswords_and_carries_the_recovered_text() {
        // `rpassword::prompt_password` itself wants a TTY, so the prompt goes
        // through rpassword's configurable entry point with a reader and a file
        // for the output: the text is the recovered one and the line comes back
        // without its terminator.
        let prompt = hydra_session::password::prompt("Aelith");
        assert_eq!(prompt, "Password for account Aelith: ");
        let out = temp_dir("prompt").join("prompt.txt");
        std::fs::write(&out, "").expect("touch");
        let config = rpassword::ConfigBuilder::new()
            .input_data("hunter2\n")
            .output_file_path(out.to_string_lossy().into_owned())
            .build();
        let typed = rpassword::prompt_password_with_config(prompt.clone(), config).expect("read");
        assert_eq!(typed, "hunter2");
        assert_eq!(std::fs::read_to_string(&out).expect("prompt"), prompt);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_table_account_{name}"));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
