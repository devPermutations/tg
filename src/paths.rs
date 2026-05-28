//! Resolves ~/.tg/ paths, with TG_HOME override for tests.

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

pub fn ensure_dir(p: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tg_home_uses_override() {
        std::env::set_var("TG_HOME", "/tmp/tg-test-xyz");
        assert_eq!(tg_home(), PathBuf::from("/tmp/tg-test-xyz"));
        std::env::remove_var("TG_HOME");
    }

    #[test]
    fn config_path_is_under_home() {
        std::env::set_var("TG_HOME", "/tmp/x");
        assert_eq!(config_path(), PathBuf::from("/tmp/x/config.toml"));
        std::env::remove_var("TG_HOME");
    }
}
