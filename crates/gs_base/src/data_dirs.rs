//! Where Hydra keeps its on-disk data.
//!
//! Recovered from:
//! * `gs_base::data_dirs::base_dir`         @0x100a8f7c8
//! * `gs_base::data_dirs::base_generation`  @0x100a8f7bc
//! * `gs_base::data_dirs::bodies_dir`       @0x100a8f490
//! * `gs_base::data_dirs::global_dir`       @0x100a8f5b0
//! * `gs_base::data_dirs::set_base_dir`     @0x100a8f66c
//!
//! Layout (all joins established from the string constants):
//!
//! ```text
//! base_dir()            = $VELLUM_FE_DIR  or  home_dir()/vellum-fe
//! global_dir()          = base_dir()/global
//! bodies_dir()          = base_dir()/global/bodies
//! ledger file           = global_dir()/packs/installed.toml   (see packs::ledger)
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use crate::{Error, Result};

/// Environment variable consulted when no base directory has been set.
/// The literal `VELLUM_FE_DIR` (13 bytes) sits at 0x1021f0ea0.
pub const BASE_DIR_ENV: &str = "VELLUM_FE_DIR";

/// Directory created under the user's home directory when the environment
/// variable is absent.  The literal `.vellum-fe` (10 bytes) sits at 0x1021f0eca.
pub const BASE_DIR_NAME: &str = ".vellum-fe";

/// Subdirectory under the base directory.  Literal `global` (6 bytes) @0x1021f0e9a.
pub const GLOBAL_DIR_NAME: &str = "global";

/// Subdirectory under `global/`.  Literal `bodies` (6 bytes) @0x1021f0e94.
pub const BODIES_DIR_NAME: &str = "bodies";

/// `static BASE: RwLock<Option<PathBuf>>` — in the binary the value lives at
/// 0x104586340, guarded by the `RwLock` at 0x104586330.
static BASE_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Test-only: forget any [`set_base_dir`] override (the binary has no such
/// operation; it is needed only so unit tests can restore the original state).
#[cfg(test)]
fn clear_base_dir() {
    if let Ok(mut slot) = BASE_DIR.write() {
        *slot = None;
    }
}

/// `static GENERATION: AtomicU64` at 0x10458da48.
static BASE_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Override the base directory and bump the generation counter.
///
/// `@0x100a8f66c` — takes ownership of a `PathBuf`, swaps it into the global
/// under the write lock (dropping the previous value) and does
/// `BASE_GENERATION.fetch_add(1)`.
pub fn set_base_dir(dir: PathBuf) {
    if let Ok(mut slot) = BASE_DIR.write() {
        *slot = Some(dir);
    }
    BASE_GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Monotonic counter bumped by every [`set_base_dir`] call.
///
/// `@0x100a8f7bc` — a bare load of the atomic.
pub fn base_generation() -> u64 {
    BASE_GENERATION.load(Ordering::SeqCst)
}

/// The root data directory.
///
/// `@0x100a8f7c8`:
/// 1. if a base directory has been set, return a clone of it;
/// 2. else if `VELLUM_FE_DIR` is set in the environment, return it as a path;
/// 3. else return `home_dir()?.join("vellum-fe")`.
///
/// The `anyhow::Error` built when `home_dir()` is `None` carries the literal
/// `Could not find home directory` (29 bytes) @0x1021f0ead — reproduced verbatim
/// below.  **std departure:** `dirs::home_dir` becomes `$HOME`/`$USERPROFILE`.
pub fn base_dir() -> Result<PathBuf> {
    if let Ok(slot) = BASE_DIR.read() {
        if let Some(dir) = slot.as_ref() {
            return Ok(dir.clone());
        }
    }
    if let Ok(v) = std::env::var(BASE_DIR_ENV) {
        if !v.is_empty() {
            return Ok(PathBuf::from(v));
        }
    }
    let home = home_dir().ok_or_else(|| Error::msg("Could not find home directory"))?;
    Ok(home.join(BASE_DIR_NAME))
}

/// `dirs::home_dir()` replacement (**std departure**).
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("USERPROFILE") {
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
    }
    match std::env::var_os("HOME") {
        Some(h) if !h.is_empty() => Some(PathBuf::from(h)),
        _ => None,
    }
}

/// `<base>/global`.
///
/// `@0x100a8f5b0` — `base_dir()?.join("global")`.
pub fn global_dir() -> Result<PathBuf> {
    Ok(base_dir()?.join(GLOBAL_DIR_NAME))
}

/// `<base>/global/bodies`.
///
/// `@0x100a8f490` — `base_dir()?.join("global").join("bodies")`; the compiler
/// inlined the first join and an extra result check, semantically identical to
/// `global_dir()?.join("bodies")`.
pub fn bodies_dir() -> Result<PathBuf> {
    Ok(global_dir()?.join(BODIES_DIR_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These tests touch process-global state, so they are serialised with a
    /// mutex and restore the previous value.
    fn with_clean_env<T>(f: impl FnOnce() -> T) -> T {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var(BASE_DIR_ENV).ok();
        let prev_home = std::env::var("HOME").ok();
        std::env::remove_var(BASE_DIR_ENV);
        clear_base_dir();
        let out = f();
        clear_base_dir();
        match prev {
            Some(v) => std::env::set_var(BASE_DIR_ENV, v),
            None => std::env::remove_var(BASE_DIR_ENV),
        }
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        out
    }

    #[test]
    fn joins_are_recovered_exactly() {
        with_clean_env(|| {
            std::env::set_var(BASE_DIR_ENV, "/tmp/hydra-base");
            assert_eq!(base_dir().unwrap(), PathBuf::from("/tmp/hydra-base"));
            assert_eq!(
                global_dir().unwrap(),
                PathBuf::from("/tmp/hydra-base/global")
            );
            assert_eq!(
                bodies_dir().unwrap(),
                PathBuf::from("/tmp/hydra-base/global/bodies")
            );
        });
    }

    #[test]
    fn env_name_and_subdir_constants() {
        assert_eq!(BASE_DIR_ENV, "VELLUM_FE_DIR");
        assert_eq!(BASE_DIR_NAME.len(), 10);
        assert_eq!(
            PathBuf::from("/x").join(BASE_DIR_NAME),
            PathBuf::from("/x/.vellum-fe")
        );
        assert_eq!(GLOBAL_DIR_NAME, "global");
        assert_eq!(BODIES_DIR_NAME, "bodies");
    }

    #[test]
    fn home_fallback_and_missing_home_error() {
        with_clean_env(|| {
            std::env::set_var("HOME", "/home/tester");
            assert_eq!(
                base_dir().unwrap(),
                PathBuf::from("/home/tester/.vellum-fe")
            );
            std::env::remove_var("HOME");
            let err = base_dir().unwrap_err();
            assert_eq!(err.message(), "Could not find home directory");
        });
    }

    #[test]
    fn set_base_dir_wins_and_bumps_the_generation() {
        with_clean_env(|| {
            std::env::set_var(BASE_DIR_ENV, "/tmp/env-fallback");
            assert_eq!(base_dir().unwrap(), PathBuf::from("/tmp/env-fallback"));
            let before = base_generation();
            set_base_dir(PathBuf::from("/tmp/from-setter"));
            assert!(base_generation() > before);
            assert_eq!(base_dir().unwrap(), PathBuf::from("/tmp/from-setter"));
            assert_eq!(
                global_dir().unwrap(),
                PathBuf::from("/tmp/from-setter/global")
            );
            set_base_dir(PathBuf::from("/tmp/second"));
            assert_eq!(base_generation(), before + 2);
            assert_eq!(base_dir().unwrap(), PathBuf::from("/tmp/second"));
        });
    }
}
