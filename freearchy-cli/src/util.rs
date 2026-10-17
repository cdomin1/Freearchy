use anyhow::{bail, Context, Result};
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Where the freearchy repo lives on an installed system.
/// Omarchy uses ~/.local/share/omarchy; we mirror that convention.
pub fn repo_root() -> Result<PathBuf> {
    if let Ok(p) = env::var("FREEARCHY_HOME") {
        return Ok(PathBuf::from(p));
    }
    Ok(home()?.join(".local/share/freearchy"))
}

pub fn home() -> Result<PathBuf> {
    env::var("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

pub fn config_home() -> Result<PathBuf> {
    match env::var("XDG_CONFIG_HOME") {
        Ok(p) => Ok(PathBuf::from(p)),
        Err(_) => Ok(home()?.join(".config")),
    }
}

/// Run a command, streaming stdio, failing on non-zero exit.
pub fn run(cmd: &str, args: &[&str]) -> Result<()> {
    println!("==> {} {}", cmd, args.join(" "));
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("failed to spawn {cmd}"))?;
    if !status.success() {
        bail!("{cmd} exited with {status}");
    }
    Ok(())
}

/// Run a command and capture stdout as a string. Returns None on failure.
pub fn capture(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

pub fn on_path(binary: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|d| d.join(binary).is_file()))
        .unwrap_or(false)
}

/// Only meaningful at runtime; keeps the CLI checkable off-FreeBSD.
pub fn ensure_freebsd() -> Result<()> {
    if env::consts::OS != "freebsd" {
        bail!("freearchy must run on FreeBSD (this is {})", env::consts::OS);
    }
    Ok(())
}
