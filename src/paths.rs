//! Resolves ~/.tg/ paths, with TG_HOME override for tests.

use anyhow::{anyhow, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn tg_home() -> PathBuf {
    if let Ok(p) = std::env::var("TG_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".tg")
}

pub fn config_path() -> PathBuf { tg_home().join("config.toml") }
pub fn pending_path() -> PathBuf { tg_home().join("pending.json") }
pub fn state_path() -> PathBuf { tg_home().join("state") }
pub fn inbox_dir() -> PathBuf { tg_home().join("inbox") }

/// Refuses to read paths whose mode is wider than 0600. Used by any
/// module that loads a secret/token-bearing file.
pub fn check_mode_strict(path: &Path) -> Result<()> {
    let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(anyhow!(
            "{} mode is {:o}; refusing to read (must be 0600)",
            path.display(),
            mode
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_lock;

    #[test]
    fn tg_home_uses_override() {
        let _g = test_lock::acquire();
        std::env::set_var("TG_HOME", "/tmp/tg-test-xyz");
        let got = tg_home();
        std::env::remove_var("TG_HOME");
        assert_eq!(got, PathBuf::from("/tmp/tg-test-xyz"));
    }

    #[test]
    fn config_path_is_under_home() {
        let _g = test_lock::acquire();
        std::env::set_var("TG_HOME", "/tmp/x");
        let got = config_path();
        std::env::remove_var("TG_HOME");
        assert_eq!(got, PathBuf::from("/tmp/x/config.toml"));
    }
}

#[cfg(test)]
pub mod test_lock {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    /// Hold this guard for the duration of any test that sets TG_HOME.
    /// All such tests across all modules acquire the same lock, so they
    /// execute serially with respect to TG_HOME mutation even when cargo
    /// runs them in parallel threads.
    pub fn acquire() -> MutexGuard<'static, ()> {
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
    }
}
