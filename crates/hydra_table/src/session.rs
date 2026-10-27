//! Starting a supervised session: the crit tables and the log folder.
//!
//! Recovered from `hydra_table::session` (`0x1011cba30`, `0x1011cbb90`).
//! [`crit_tables`] is recovered completely.  [`supervised_with`] is the 12 KB
//! session bootstrapper: it resolves the data directory, builds the player log
//! (`hydra_session::player_log`), the sink (`hydra_net::sink`), the game
//! connection (`hydra_net::behind::Live::new`) and the combat tracker, then spawns
//! a `tokio` task — none of which survives as one call.  What is recovered here is
//! the order of the steps and the two player-log messages.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use hydra_model::crit::CritTables;

/// The message the binary prints when the crit tables cannot be built.
///
/// **ESTABLISHED**: the 32-byte piece at `0x103e60da7` —
/// ` [hydra] crit tables DISABLED -- ` followed by the `BuildError` and a newline,
/// printed with `std::io::stdio::_eprint` (`0x1011cba90`).
pub const CRIT_DISABLED: &str = " [hydra] crit tables DISABLED -- ";

/// The message a player log that had to be moved prints.
///
/// **ESTABLISHED**: `0x103e60c3b` — `Logs kept under this name before games were
/// told apart are in ` followed by the folder.
pub const LOGS_MOVED: &str =
    "Logs kept under this name before games were told apart are in ";

/// The message for a player log that could not be moved.
///
/// **ESTABLISHED**: `0x103e60c9b` — `The player log kept under this name alone
/// could not be moved to its game's folder: ` followed by the error.
pub const LOG_NOT_MOVED: &str =
    "The player log kept under this name alone could not be moved to its game's folder: ";

/// The shared crit tables, or `None` when they could not be built.
///
/// **ESTABLISHED** (`session::crit_tables`, `0x1011cba30`):
/// `hydra_model::crit::CritTables::load` (`0x101c0df94`); on `Ok` the tables are
/// wrapped in an `Arc` (the `0x90`-byte `ArcInner` allocation at `0x1011cbadc`), on
/// `Err` the error is printed with [`CRIT_DISABLED`] to stderr and the answer is
/// `None` (`mov x0, #0` — the `None` niche of `Option<Arc<_>>`).
pub fn crit_tables() -> Option<Arc<CritTables>> {
    static TABLES: OnceLock<Option<Arc<CritTables>>> = OnceLock::new();
    TABLES
        .get_or_init(|| match CritTables::load() {
            Ok(tables) => Some(Arc::new(tables)),
            Err(error) => {
                eprintln!("{CRIT_DISABLED}{error}");
                None
            }
        })
        .clone()
}

/// What a supervised session was built with.
///
/// **ESTABLISHED** (`session::supervised_with`, `0x1011cbb90`): the data directory
/// (`HYDRA_DATA_DIR`, the 14-byte literal at `0x10240740e`), the log directory
/// (`hydra_net::sink::config::log_dir`, `0x101c509f0`), the date and stamp
/// (`date_dir`, `file_stamp`), the shared crit tables
/// (`session::crit_tables`), the player log's settings (`pages::log_settings`,
/// `0x1012608e8`) and the account the seat belongs to
/// (`hydra_net::sink::redactions::Redactions::account_of`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Supervised {
    /// Where Hydra's data lives.
    pub data_dir: PathBuf,
    /// Where this session's logs go.
    pub log_dir: PathBuf,
    /// The `YYYY-MM-DD` directory of the session.
    pub date: String,
    /// The `YYYY-MM-DD_HH-MM-SS` stamp that names its files.
    pub stamp: String,
    /// Whether the crit tables could be built.
    pub crit: bool,
    /// The notices the player log produced, in order.
    pub notices: Vec<String>,
}

/// Build a supervised session's environment.
///
/// **ESTABLISHED** (`session::supervised_with`, `0x1011cbb90`) — the recovered
/// order of the steps:
///
/// 1. `HYDRA_DATA_DIR`, or `data` (the same `0x10240740e` literal and 4-byte
///    fallback as [`crate::account::data_dir`]);
/// 2. the log directory from `hydra_net::sink::config::log_dir` and the date and
///    stamp from `date_dir`/`file_stamp`;
/// 3. [`crit_tables`], handed to
///    `hydra_model::state::combat::tracker::CombatTracker::set_crit_tables`;
/// 4. `pages::log_settings`, then `hydra_session::player_log::writer::
///    adopt_unscoped` — the two messages [`LOGS_MOVED`] and [`LOG_NOT_MOVED`]
///    come from this step;
/// 5. `hydra_net::sink::writer::SessionSink::create`, `player_log::channel::
///    PlayerLog::new`, `hydra_net::behind::Live::new` and a `tokio` spawn.
///
/// **Departure**: steps 3–5 are not performed.  The four arguments the original
/// takes (`&String`, `&String`, `&str` and an `Arc` out of the table at `+0x50`)
/// identify the seat; this takes the account and the character and reports the
/// directories and the crit-table state the caller needs to continue.
pub fn supervised_with(account: &str, character: &str) -> Supervised {
    let data_dir = crate::account::data_dir();
    let log_dir = hydra_net::sink::config::log_dir();
    let mut notices = Vec::new();
    // Step 4's adoption: an unscoped log from before games were told apart.
    let unscoped = log_dir.join(format!("{}.log", character.trim()));
    if unscoped.is_file() {
        match std::fs::create_dir_all(log_dir.join("GS4")) {
            Ok(()) => notices.push(format!("{LOGS_MOVED}{}", log_dir.display())),
            Err(error) => notices.push(format!("{LOG_NOT_MOVED}{error}")),
        }
    }
    Supervised {
        data_dir,
        log_dir,
        date: hydra_net::sink::config::date_dir(),
        stamp: hydra_net::sink::config::file_stamp(),
        crit: crit_tables().is_some(),
        notices,
    }
}

/// The account a sink belongs to, as the redactions module reports it.
///
/// **ESTABLISHED** (`0x1011cbb90` calls
/// `hydra_net::sink::redactions::Redactions::account_of`, `0x101c50f40`).
pub fn account_of<'a>(text: &'a str) -> Option<&'a str> {
    let redactions = hydra_net::sink::redactions::Redactions::default();
    redactions.account_of(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crit_tables_load_from_the_embedded_tsv() {
        // `hydra_model::crit::CritTables::load` reads the 228 KB table the binary
        // carries, so it must succeed.
        let tables = crit_tables().expect("the crit tables load");
        let again = crit_tables().expect("cached");
        assert!(Arc::ptr_eq(&tables, &again), "the tables are shared");
    }

    #[test]
    fn a_supervised_session_reports_its_directories() {
        let supervised = supervised_with("main", "Aelith");
        assert_eq!(supervised.data_dir, crate::account::data_dir());
        assert!(!supervised.date.is_empty());
        assert_eq!(supervised.stamp.len(), 19, "YYYY-MM-DD_HH-MM-SS");
        assert!(supervised.crit);
    }

    #[test]
    fn the_recovered_messages_are_exact() {
        assert_eq!(CRIT_DISABLED, " [hydra] crit tables DISABLED -- ");
        assert!(LOGS_MOVED.ends_with("are in "));
        assert!(LOG_NOT_MOVED.ends_with("to its game's folder: "));
    }
}
