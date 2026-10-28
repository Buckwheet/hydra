//! Startup paths: where the executable is and which data folder Hydra uses.
//!
//! # Recovered shape
//!
//! `hydra::main` opens with the code at `0x10021d790` … `0x10021dab8`, which is
//! the same prologue as the small function `hydra::startup::exe`
//! (`0x10021eec4`, 232 bytes) — it reads the static `hydra::startup::EXE`
//! (`0x1044fd4a0`, a lazily filled `String` holding the executable path; the
//! `OnceLock::initialize` call is at `0x10021d7d0` / `0x10021ef18`, and the
//! `Clone::clone` of its value at `0x10021d7bc`) — and then:
//!
//! 1. `Path::parent` three times over the executable path
//!    (`0x10021d8f4`, `0x10021d908`, `0x10021d920`, `0x10021d938`);
//! 2. first parent's `file_name` compared with a 5-byte literal
//!    (`mov w9, #0x614d` / `movk #0x4f63` + `ldrb` — `MacOS`, `0x10021d960`…
//!    `0x10021d98c`);
//! 3. second parent's `file_name` compared with an 8-byte literal
//!    (`mov x9, #0x6f43` / `movk #0x746e` / `movk #0x6e65` / `movk #0x7374` —
//!    `Contents`, `0x10021d9ac`…`0x10021d9c4`);
//! 4. third parent's `extension`, ASCII-lowercased byte by byte
//!    (`csel`-guarded `orr #0x20`) and compared with `a`, `p`, `p`
//!    (`0x10021d9e0`…`0x10021da38`);
//! 5. `std::env::var_os("HOME")` (`0x10021da3c`/`0x10021da4c`, literal
//!    `0x10206fd20`).  Missing → the message `Hydra cannot find the home folder:
//!    HOME is not set` (`0x10206fc4c`, copied as 50 bytes at `0x10021da78`);
//! 6. `HOME/Library` (`0x10021e14c`, 7 bytes) → `.join("Application Support")`
//!    (`0x10021e16c`, 19 bytes) → `.join("Hydra")` (`0x10021e188`, 5 bytes);
//! 7. `DirBuilder::create` on that folder with `{recursive: true, mode: 0o777}`
//!    (`mov w8, #0x1ff` + `strh` at `0x10021e1c8`, `_create` at `0x10021e260`);
//!    failure → `"{dir} cannot be made: {io error}"` (`__cstring` `0x103e25456`);
//! 8. `std::env::set_current_dir` into it (`std::sys::paths::unix::chdir`,
//!    `0x10021e9a0`); failure → `"{dir}: {io error}"` (`0x103e25450`).
//!
//! So on macOS, when the binary runs from inside a bundle
//! (`X.app/Contents/MacOS/hydra`), the data folder is
//! `~/Library/Application Support/Hydra`, Hydra works from there (the `chdir`
//! is why every later relative path — logs, notes, the ledger — lands in the
//! data folder), and a missing `HOME` aborts the process.
//!
//! Outside a bundle the prologue does nothing and `hydra::main` falls through to
//! `hydra::started::begin`; the data folder is then whatever
//! `hydra_table::account::data_dir` decides (called from `hydra::lookout_seed`
//! at `0x10021d0d0`), which lives in a crate this phase may not depend on.
//!
//! **UNKNOWN:** the inlined value the prologue returns when `EXE` is already
//! initialised (`0x10021d7ac` clones the path into the "message" slot that
//! `0x10021dab8` prints and exits on) — the polarity of that `Option` could not
//! be settled from the instruction stream.  Nothing in the headless paths
//! depends on it: this module models the *observable* behaviour (a bundle run
//! creates and enters the Application Support folder, or aborts on a missing
//! `HOME`) and is a no-op otherwise.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The recovered message when `HOME` is unset (`__const` `0x10206fc4c`).
pub const HOME_MISSING: &str = "Hydra cannot find the home folder: HOME is not set";

/// The macOS bundle folder `~/Library/Application Support/Hydra`, as three
/// `Path::join` calls (`0x10206fc7e`, `0x10206fc85`, `0x10206fc98`).
pub const BUNDLE_DIR: [&str; 3] = ["Library", "Application Support", "Hydra"];

static EXE: OnceLock<PathBuf> = OnceLock::new();

/// The executable path, cached like `hydra::startup::EXE` (`0x1044fd4a0`).
///
/// If the platform cannot report it (which the reference build never handled),
/// an empty path is cached; every later check then simply does not match.
pub fn exe() -> &'static Path {
    EXE.get_or_init(|| std::env::current_exe().unwrap_or_default())
        .as_path()
}

/// True when `exe` sits at `X.app/Contents/MacOS/<binary>`.
///
/// The three checks (`parent().file_name() == "MacOS"`,
/// `parent².file_name() == "Contents"`, `parent³.extension()` equals `app`
/// ignoring ASCII case) are the recovered ones — see the module docs for the
/// addresses.
pub fn is_app_bundle(exe: &Path) -> bool {
    let Some(macos) = exe.parent() else {
        return false;
    };
    let Some(contents) = macos.parent() else {
        return false;
    };
    let Some(bundle) = contents.parent() else {
        return false;
    };
    if macos.file_name() != Some(OsStr::new("MacOS")) {
        return false;
    }
    if contents.file_name() != Some(OsStr::new("Contents")) {
        return false;
    }
    matches!(bundle.extension(), Some(ext) if ext.eq_ignore_ascii_case("app"))
}

/// `$HOME/Library/Application Support/Hydra`.
pub fn app_data_dir(home: &Path) -> PathBuf {
    let mut dir = home.to_path_buf();
    for piece in BUNDLE_DIR {
        dir = dir.join(piece);
    }
    dir
}

/// The recovered prologue: create and enter the bundle data folder.
///
/// * `Ok(None)` — not running from an `.app` bundle; nothing to do (this is the
///   `cargo run` / `target/debug/hydra` case, and the replay gate).
/// * `Ok(Some(dir))` — the folder was created and is now the working directory.
/// * `Err(message)` — `HOME` is unset or the folder could not be made/entered;
///   `main` prints the message to stderr and exits 1.
pub fn prepare() -> Result<Option<PathBuf>, String> {
    let dir = match prepare_from(exe(), std::env::var_os("HOME").map(PathBuf::from))? {
        Some(dir) => dir,
        None => return Ok(None),
    };
    // `DirBuilder::create` with `{recursive: true, mode: 0o777}`
    // (`mov w8, #0x1ff` at 0x10021e1c8) and `set_current_dir` (0x10021e9a0).
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("{} cannot be made: {}", dir.display(), err))?;
    std::env::set_current_dir(&dir).map_err(|err| format!("{}: {}", dir.display(), err))?;
    Ok(Some(dir))
}

/// [`prepare`] with the executable path and `HOME` passed in: the pure part of
/// the decision, so the behaviour can be tested without touching the
/// environment or the working directory.
///
/// `Ok(None)` means "not a bundle, nothing to do".
pub fn prepare_from(exe: &Path, home: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    if !is_app_bundle(exe) {
        return Ok(None);
    }
    let home = home.ok_or_else(|| HOME_MISSING.to_string())?;
    Ok(Some(app_data_dir(&home)))
}

/// The recovered log file name inside the data folder (`__const`
/// `0x10206fc9f`): `hydra.log`.
pub const LOG_FILE: &str = "hydra.log";

/// The flag the detached child must not see again (`--detach`).
pub const DETACH_FLAG: &str = "--detach";

/// The flag the detached child is always given (`--again`).
///
/// **RECOVERED** (`hydra::startup::Serve::args`, `0x10021efac`): the child's
/// argv is `serve`, then `--again`, then the parent's own options (`--web
/// <port>`, `--open`, one `--resume <arg>` per resume, and the targets).  So a
/// detached session is told it is starting over rather than continuing
/// whatever the parent was doing — which is what "rest in the background"
/// means: the parent is leaving, and the child is the rest of the run.
pub const AGAIN_FLAG: &str = "--again";

/// The recovered message `hydra::startup::detach` prints for the parent
/// (`__const` `0x103e2546b`, `[hydra] resting in the background; its log is `).
pub fn resting(log: &Path) -> String {
    format!("[hydra] resting in the background; its log is {}", log.display())
}

/// The log's path inside `data` (`hydra_table::account::data_dir` in practice).
pub fn log_path(data: &Path) -> PathBuf {
    data.join(LOG_FILE)
}

/// The argv a detached child runs: `serve`, then `args` with every `--detach`
/// removed.
///
/// **RECOVERED** shape: `hydra::startup::Serve::args` (`0x10021efac`) built
/// `serve` + [`AGAIN_FLAG`] + the parent's own options (each `--web`/`--open`
/// and one `--resume` per resume, then the targets), and
/// `hydra::startup::detach` spawned that with `std::process::Command::new(exe())`.
/// This takes the parent's raw argument list instead of its parsed options —
/// the same words — minus the flag that would make the child detach again, plus
/// the `--again` the recovered builder always added.
pub fn detach_args(args: &[String]) -> Vec<String> {
    let mut out = vec![SUBCOMMAND.to_string(), AGAIN_FLAG.to_string()];
    out.extend(
        args.iter()
            .filter(|arg| arg.as_str() != DETACH_FLAG)
            .cloned(),
    );
    out
}

/// The subcommand a detached child runs.
pub const SUBCOMMAND: &str = "serve";

/// Everything the spawn needs, as a value, so the argv/cwd rules are testable
/// without starting a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detach {
    /// the executable to spawn (`hydra::startup::exe`)
    pub exe: PathBuf,
    /// `serve` + the parent's arguments without `--detach`
    pub args: Vec<String>,
    /// the data folder (`hydra_table::account::data_dir`)
    pub data: PathBuf,
    /// `<data>/hydra.log`
    pub log: PathBuf,
    /// the child's working directory (`--in <dir>`, if given)
    pub cwd: Option<PathBuf>,
}

/// [`Detach`] for these values — the pure half of [`detach`].
pub fn plan_detach(exe: &Path, args: &[String], data: &Path, cwd: Option<&Path>) -> Detach {
    Detach {
        exe: exe.to_path_buf(),
        args: detach_args(args),
        data: data.to_path_buf(),
        log: log_path(data),
        cwd: cwd.map(Path::to_path_buf),
    }
}

/// Rest in the background: spawn this executable as `hydra serve` with stdin
/// closed and stdout/stderr appended to `<data>/hydra.log`.
///
/// **RECOVERED** (`0x10021f40c`): `hydra_table::account::data_dir`, create it,
/// open `<data>/hydra.log` create+append, spawn `serve <args>` with
/// `stdin(Stdio::null())` and both output streams on that file.  The child is
/// *not* waited for; the parent goes on to print [`resting`].
pub fn detach(args: &[String], cwd: Option<&Path>) -> Result<PathBuf, String> {
    let plan = plan_detach(exe(), args, &hydra_table::account::data_dir(), cwd);
    std::fs::create_dir_all(&plan.data).map_err(|err| {
        format!(
            "{} cannot be made: {err}",
            plan.data.display()
        )
    })?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&plan.log)
        .map_err(|err| format!("{}: {err}", plan.log.display()))?;
    let copy = log
        .try_clone()
        .map_err(|err| format!("{}: {err}", plan.log.display()))?;

    let mut command = std::process::Command::new(&plan.exe);
    command
        .args(&plan.args)
        .stdin(std::process::Stdio::null())
        .stdout(copy)
        .stderr(log);
    if let Some(cwd) = &plan.cwd {
        command.current_dir(cwd);
    }
    command
        .spawn()
        .map_err(|err| format!("{}: {err}", plan.exe.display()))?;
    Ok(plan.log)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_layout_is_recognised() {
        assert!(is_app_bundle(Path::new(
            "/Applications/Hydra.app/Contents/MacOS/hydra"
        )));
        assert!(is_app_bundle(Path::new(
            "/Users/x/Applications/Hydra.APP/Contents/MacOS/Hydra"
        )));
    }

    #[test]
    fn everything_else_is_not_a_bundle() {
        for path in [
            "/Users/x/Code/hydra-recon/target/debug/hydra",
            "/Applications/Hydra.app/Contents/hydra",
            "/Applications/Hydra.app/MacOS/hydra",
            "/Applications/Hydra.app/Contents/Resources/hydra",
            "/Applications/Hydra/Contents/MacOS/hydra",
            "/Applications/Hydra.app/Contents/MacOSX/hydra",
            "hydra",
            "",
        ] {
            assert!(!is_app_bundle(Path::new(path)), "{path} must not match");
        }
    }

    /// `~/Library/Application Support/Hydra` built with joins here, so the
    /// expectation is spelled with this platform's separators.
    fn bundle_dir(home: &str) -> PathBuf {
        BUNDLE_DIR
            .iter()
            .fold(PathBuf::from(home), |dir, piece| dir.join(piece))
    }

    #[test]
    fn the_data_folder_is_application_support() {
        assert_eq!(app_data_dir(Path::new("/Users/x")), bundle_dir("/Users/x"));
    }

    #[test]
    fn a_bundle_run_makes_the_folder_and_a_missing_home_aborts() {
        let exe = Path::new("/Applications/Hydra.app/Contents/MacOS/hydra");
        assert_eq!(
            prepare_from(exe, Some(PathBuf::from("/Users/x"))).unwrap(),
            Some(bundle_dir("/Users/x"))
        );
        assert_eq!(
            prepare_from(exe, None).unwrap_err(),
            HOME_MISSING.to_string()
        );
    }

    #[test]
    fn a_plain_build_needs_nothing() {
        let exe = Path::new("/Users/x/Code/hydra-recon/target/debug/hydra");
        assert_eq!(prepare_from(exe, Some(PathBuf::from("/Users/x"))).unwrap(), None);
        assert_eq!(prepare_from(exe, None).unwrap(), None);
    }

    #[test]
    fn the_detached_argv_is_serve_without_the_detach_flag() {
        let args: Vec<String> = ["Aelith", "--detach", "--web", "7788", "--in", "/tmp/in"]
            .iter()
            .map(|word| word.to_string())
            .collect();
        assert_eq!(
            detach_args(&args),
            vec![
                "serve".to_string(),
                // The recovered `Serve::args` always appends this one.
                "--again".to_string(),
                "Aelith".to_string(),
                "--web".to_string(),
                "7788".to_string(),
                "--in".to_string(),
                "/tmp/in".to_string(),
            ]
        );
        // Every occurrence goes, not just the first, and no other word is
        // touched (including a value that merely starts with `--`).
        let args: Vec<String> = ["--detach", "--detach", "--web", "1"]
            .iter()
            .map(|word| word.to_string())
            .collect();
        assert_eq!(
            detach_args(&args),
            vec![
                "serve".to_string(),
                "--again".to_string(),
                "--web".to_string(),
                "1".to_string()
            ]
        );
        assert_eq!(
            detach_args(&[]),
            vec!["serve".to_string(), "--again".to_string()]
        );
    }

    #[test]
    fn the_detach_plan_names_the_log_and_the_folder() {
        let exe = Path::new("/opt/hydra/hydra");
        let data = Path::new("/synthetic/data");
        let args = vec!["Aelith".to_string(), "--detach".to_string()];
        let plan = plan_detach(exe, &args, data, Some(Path::new("/synthetic/in")));
        assert_eq!(plan.exe.as_path(), exe);
        assert_eq!(
            plan.args,
            vec![
                "serve".to_string(),
                AGAIN_FLAG.to_string(),
                "Aelith".to_string()
            ]
        );
        assert_eq!(plan.data.as_path(), data);
        // The log is `<data>/hydra.log`, spelled with this platform's
        // separators rather than a hard-coded slash path.
        assert_eq!(plan.log, log_path(data));
        assert_eq!(plan.cwd.as_deref(), Some(Path::new("/synthetic/in")));

        // Without `--in` the child inherits the parent's folder.
        let plan = plan_detach(exe, &args, data, None);
        assert_eq!(plan.cwd, None);
        assert_eq!(log_path(data), plan.log);

        // The recovered line names the log.
        let log = log_path(data);
        assert_eq!(
            resting(&log),
            format!(
                "[hydra] resting in the background; its log is {}",
                log.display()
            )
        );
    }

    #[test]
    fn the_cached_executable_is_this_test_binary() {
        let path = exe();
        assert!(path.is_absolute(), "{path:?}");
        // The test binary never lives in an .app bundle.
        assert!(!is_app_bundle(path));
        // …and the cache really is a cache.
        assert!(std::ptr::eq(path, exe()));
    }
}
