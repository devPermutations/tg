//! Resolves ~/.tg/ paths.
//!
//! Resolution order: thread-local override (set by tests) → TG_HOME
//! env var → $HOME/.tg. The thread-local override lets unit tests
//! run in parallel without contending on process-global env vars.

use anyhow::{anyhow, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[cfg(test)]
std::thread_local! {
    /// Per-thread override for `tg_home()`. Set via `set_test_tg_home`
    /// from within a unit test; restored to `None` automatically when
    /// the returned guard is dropped.
    static TEST_HOME: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

pub fn tg_home() -> PathBuf {
    #[cfg(test)]
    {
        if let Some(p) = TEST_HOME.with(|cell| cell.borrow().clone()) {
            return p;
        }
    }
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

#[allow(dead_code)]
pub fn ensure_dir(p: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(p)
}

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
pub mod test_helpers {
    use super::*;

    /// Guard returned by `set_test_tg_home`. Restores the previous
    /// thread-local value (or `None`) on drop, so test teardown is
    /// automatic even on assertion panics.
    pub struct TestHomeGuard {
        previous: Option<PathBuf>,
    }

    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            let prev = self.previous.take();
            TEST_HOME.with(|cell| *cell.borrow_mut() = prev);
        }
    }

    /// Override `tg_home()` for the calling thread for the duration
    /// of the returned guard. Tests should do:
    ///
    ///     let dir = tempdir().unwrap();
    ///     let _home = paths::test_helpers::set_test_tg_home(dir.path());
    ///     // ... test body ...
    ///     // guard drops at end of scope, override restored
    ///
    /// Multiple tests can run in parallel because the override is
    /// per-thread. No mutex needed.
    pub fn set_test_tg_home(p: impl Into<PathBuf>) -> TestHomeGuard {
        let path = p.into();
        let previous = TEST_HOME.with(|cell| {
            let prev = cell.borrow().clone();
            *cell.borrow_mut() = Some(path);
            prev
        });
        TestHomeGuard { previous }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tg_home_uses_thread_local_override() {
        let _h = test_helpers::set_test_tg_home("/tmp/tg-test-xyz");
        assert_eq!(tg_home(), PathBuf::from("/tmp/tg-test-xyz"));
        // _h drops at end of scope, override restored.
    }

    #[test]
    fn config_path_picks_up_override() {
        let _h = test_helpers::set_test_tg_home("/tmp/x");
        assert_eq!(config_path(), PathBuf::from("/tmp/x/config.toml"));
    }

    #[test]
    fn thread_locals_do_not_leak_across_threads() {
        // Set the override on this thread.
        let _h = test_helpers::set_test_tg_home("/tmp/this-thread");

        // Spawn a worker that should see NO override (or whatever the
        // env var / HOME default is, but NOT /tmp/this-thread).
        let worker_value = std::thread::spawn(|| tg_home())
            .join()
            .unwrap();

        assert_ne!(
            worker_value,
            PathBuf::from("/tmp/this-thread"),
            "override leaked to worker thread"
        );
    }
}
