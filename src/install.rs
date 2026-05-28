//! `tg install` — symlink into ~/.ir/tools/ and install systemd unit.
//!
//! Idempotent. Each step checks current state before mutating.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_BODY: &str = include_str!("../systemd/tg-listen.service");
const SERVICE_NAME: &str = "tg-listen.service";

pub struct InstallOpts {
    pub systemctl_bin: String,
    pub dry_run: bool,
}

pub fn run(opts: InstallOpts) -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let home = PathBuf::from(home);

    let cargo_bin = home.join(".cargo/bin/tg");
    if !cargo_bin.exists() {
        return Err(anyhow!(
            "{} does not exist; run `cargo install --path .` first",
            cargo_bin.display()
        ));
    }

    // Step 1: ~/.ir/tools/tg symlink (if ~/.ir/tools/ exists).
    let tools_dir = home.join(".ir/tools");
    if tools_dir.exists() {
        ensure_symlink(&cargo_bin, &tools_dir.join("tg"), opts.dry_run)?;
    } else {
        println!("(skip) ~/.ir/tools/ does not exist; no ir-tool symlink created");
    }

    // Step 2: copy systemd unit if missing or different.
    let unit_dir = home.join(".config/systemd/user");
    let unit_path = unit_dir.join(SERVICE_NAME);
    ensure_unit(&unit_path, opts.dry_run)?;

    // Step 3-4: daemon-reload + enable (do not start).
    if !opts.dry_run {
        run_cmd(&opts.systemctl_bin, &["--user", "daemon-reload"])?;
        run_cmd(&opts.systemctl_bin, &["--user", "enable", SERVICE_NAME])?;
    }

    println!("install complete. run `tg init` if you haven't, then `systemctl --user start tg-listen`.");
    Ok(())
}

fn ensure_symlink(target: &Path, link: &Path, dry_run: bool) -> Result<()> {
    match std::fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let actual = std::fs::read_link(link)?;
            if actual == target {
                println!("(ok) {} -> {}", link.display(), target.display());
                return Ok(());
            }
            return Err(anyhow!(
                "{} exists and points to {} (not our binary); resolve manually",
                link.display(), actual.display()
            ));
        }
        Ok(_) => {
            return Err(anyhow!(
                "{} exists and is not a symlink; resolve manually",
                link.display()
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Create it.
        }
        Err(e) => return Err(e.into()),
    }
    if dry_run {
        println!("(dry-run) would symlink {} -> {}", link.display(), target.display());
    } else {
        if let Some(parent) = link.parent() { std::fs::create_dir_all(parent)?; }
        std::os::unix::fs::symlink(target, link)?;
        println!("symlinked {} -> {}", link.display(), target.display());
    }
    Ok(())
}

fn ensure_unit(path: &Path, dry_run: bool) -> Result<()> {
    let needs_write = match std::fs::read_to_string(path) {
        Ok(existing) => existing != SERVICE_BODY,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(e) => return Err(e.into()),
    };
    if !needs_write {
        println!("(ok) {} up to date", path.display());
        return Ok(());
    }
    if dry_run {
        println!("(dry-run) would write {}", path.display());
        return Ok(());
    }
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, SERVICE_BODY)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn run_cmd(bin: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(bin).args(args).status()
        .with_context(|| format!("running {bin} {}", args.join(" ")))?;
    if !status.success() {
        return Err(anyhow!("{bin} {} exited {status}", args.join(" ")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ensure_symlink_creates_when_missing() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("bin/tg");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "").unwrap();
        let link = dir.path().join("tools/tg");
        ensure_symlink(&target, &link, false).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
    }

    #[test]
    fn ensure_symlink_refuses_alien_existing() {
        let dir = tempdir().unwrap();
        let link = dir.path().join("tg");
        std::fs::write(&link, "alien").unwrap();
        let target = dir.path().join("ours");
        std::fs::write(&target, "").unwrap();
        let err = ensure_symlink(&target, &link, false).unwrap_err().to_string();
        assert!(err.contains("not a symlink"));
    }

    #[test]
    fn ensure_unit_writes_then_idempotent() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("unit.service");
        ensure_unit(&p, false).unwrap();
        assert!(p.exists());
        // Same body → no rewrite (we can't observe directly, but call again).
        ensure_unit(&p, false).unwrap();
    }
}
