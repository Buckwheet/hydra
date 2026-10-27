//! The account-password helper: name the variable, know the prompt.
//!
//! Two literals sit next to each other in `__TEXT`:
//!
//! ```text
//! 0x103e613d0   "\x0fHYDRA_PASSWORD_{}"              "HYDRA_PASSWORD_" is 15 bytes
//! 0x103e613e2   "\x15Password for account {}: "      "Password for account " is 21
//! ```
//!
//! Both are used by **`hydra_table::account`**, not by `hydra_session` itself,
//! and the recovered code is:
//!
//! * `hydra_table::account::env_name` (`0x1011f76a6c`): trims the account, maps
//!   every character (`0x10128ed34`) to keep ASCII alphanumerics, fold
//!   `a`-`z` to upper case and replace anything else with `_`, then formats
//!   `HYDRA_PASSWORD_{}`.  So account `Aelith` becomes the variable
//!   `HYDRA_PASSWORD_AELITH` and `gem-stone` becomes
//!   `HYDRA_PASSWORD_GEM_STONE`.
//! * `hydra_table::account::kept` (`0x1011f7568`): trims and lower-cases the
//!   account and asks `keyring::Entry::get_password` for it — the **OS
//!   keychain**, with a 5-byte service name.
//! * `hydra_table::account::password` (`0x1011f7b68`): `kept`, and when that is
//!   `None`, `rpassword::prompt_password(&format!("Password for account {}: ",
//!   account))` — an interactive terminal read.
//!
//! So: the `Password for account {account}: ` text is **Hydra's own prompt**,
//! not a prompt printed by Lich, and the password reaches Lich through the
//! *stdin* of the child, never through argv or its environment:
//! `hydra_lich::child::command` removes **every** `HYDRA_PASSWORD_*` variable
//! from the child's environment before spawning (`child::is_password`,
//! `0x101683420`), precisely so user-supplied Ruby cannot read it.
//!
//! This module exposes the two recovered helpers so the session and the relay
//! agree on the names, and a prompt recogniser for the case where a front end
//! sees the prompt come back on a stream.
//!
//! **UNKNOWN**: the keychain service name (a 5-byte literal at `0x102407409`
//! that is not in `__TEXT,__cstring`) and whether the original ever typed a
//! stored password into Lich's stdin unprompted.  [`kept`] here reads the
//! environment variable instead of the keychain, which is the documented
//! non-interactive channel; nothing in this crate logs a password.

/// The prefix of the environment variable that carries an account password.
/// **ESTABLISHED** (15 bytes at `0x103e613d0`).
pub const ENV_PREFIX: &str = "HYDRA_PASSWORD_";
/// The prompt Hydra shows for an account password.  **ESTABLISHED** (the
/// 21-byte piece at `0x103e613e2`).
pub const PROMPT_PREFIX: &str = "Password for account ";
/// The prompt ends with this.
pub const PROMPT_SUFFIX: &str = ": ";

/// `HYDRA_PASSWORD_<ACCOUNT>`.
///
/// **ESTABLISHED** (`hydra_table::account::env_name`, `0x1011f76a6c`): trim,
/// then let every ASCII alphanumeric through, upper-case `a`-`z`, and turn
/// everything else into `_`.
pub fn env_name(account: &str) -> String {
    let mut out = String::with_capacity(ENV_PREFIX.len() + account.len());
    out.push_str(ENV_PREFIX);
    for c in account.trim().chars() {
        if c.is_ascii_lowercase() {
            out.push(c.to_ascii_uppercase());
        } else if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

/// The account password, if the environment carries one.
///
/// The original asks the OS keychain first and falls back to an interactive
/// prompt; this crate reads `HYDRA_PASSWORD_<ACCOUNT>` and never prompts, so it
/// is safe to call from a test or a headless helper.  **Never** log the value:
/// the only thing callers may report is [`env_name`].
pub fn kept(account: &str) -> Option<String> {
    let name = env_name(account);
    std::env::var_os(name)
        .map(|v| v.to_string_lossy().into_owned())
        .filter(|v| !v.is_empty())
}

/// The prompt Hydra shows for `account`.
///
/// **ESTABLISHED** (`hydra_table::account::password`, `0x1011f7b68`):
/// `format!("Password for account {}: ", account)`.
pub fn prompt(account: &str) -> String {
    format!("{PROMPT_PREFIX}{account}{PROMPT_SUFFIX}")
}

/// Recognise the password prompt in a line of text and return the account it
/// names.
///
/// The prompt may be preceded by anything (a `[login]` prefix, escape
/// sequences) but must end the line, which is how a terminal prompt arrives.
pub fn is_prompt(line: &str) -> Option<&str> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let rest = trimmed
        .strip_suffix(PROMPT_SUFFIX)
        .or_else(|| trimmed.strip_suffix(':'))?;
    let at = rest.rfind(PROMPT_PREFIX)?;
    Some(&rest[at + PROMPT_PREFIX.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_name_upper_cases_and_replaces() {
        assert_eq!(env_name("Aelith"), "HYDRA_PASSWORD_AELITH");
        assert_eq!(env_name("  aelith  "), "HYDRA_PASSWORD_AELITH");
        assert_eq!(env_name("gem-stone"), "HYDRA_PASSWORD_GEM_STONE");
        assert_eq!(env_name("a.b c"), "HYDRA_PASSWORD_A_B_C");
        assert_eq!(env_name("ACC9"), "HYDRA_PASSWORD_ACC9");
        assert_eq!(ENV_PREFIX, "HYDRA_PASSWORD_");
        assert_eq!(ENV_PREFIX.len(), 15, "the recovered piece is 15 bytes");
    }

    #[test]
    fn the_prompt_is_the_recovered_text() {
        assert_eq!(prompt("Aelith"), "Password for account Aelith: ");
        assert_eq!(PROMPT_PREFIX.len(), 21, "the recovered piece is 21 bytes");
    }

    #[test]
    fn the_prompt_is_recognised_with_a_prefix() {
        assert_eq!(is_prompt("Password for account Aelith: "), Some("Aelith"));
        assert_eq!(
            is_prompt("[login] Password for account gem-stone: "),
            Some("gem-stone")
        );
        assert_eq!(is_prompt("Password for account X: \r\n"), Some("X"));
        assert_eq!(is_prompt("what is your password?"), None);
        assert_eq!(is_prompt("Password for account"), None);
        // The suffix is required.
        assert_eq!(is_prompt("Password for account X"), None);
    }

    #[test]
    fn kept_reads_only_a_real_environment_variable() {
        // A name that cannot be set by accident.
        assert_eq!(kept("hydra-test-no-such-account"), None);
        let name = env_name("hydra-test-account");
        std::env::set_var(&name, "secret");
        assert_eq!(kept("hydra-test-account"), Some("secret".to_string()));
        std::env::set_var(&name, "");
        assert_eq!(kept("hydra-test-account"), None);
        std::env::remove_var(&name);
    }
}
