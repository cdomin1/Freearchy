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
    // Under doas/sudo/su, HOME points at /root but the desktop config belongs
    // to the invoking user — resolve their home from the passwd db.
    if is_root() {
        if let Some(u) = invoking_user() {
            if let Some(h) = capture("pw", &["usershow", "-n", &u])
                .and_then(|l| l.split(':').nth(8).map(String::from))
            {
                return Ok(PathBuf::from(h));
            }
        }
    }
    env::var("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

fn is_root() -> bool {
    capture("id", &["-u"]).is_some_and(|s| s.trim() == "0")
}

/// The human who invoked us, even through doas/sudo/su. None for a real
/// root login — there's no desktop user to configure.
pub fn invoking_user() -> Option<String> {
    for var in ["SUDO_USER", "DOAS_USER"] {
        if let Ok(u) = env::var(var) {
            if u != "root" {
                return Some(u);
            }
        }
    }
    capture("logname", &[])
        .map(|s| s.trim().to_string())
        .filter(|u| !u.is_empty() && u != "root")
        .or_else(|| env::var("USER").ok().filter(|u| u != "root"))
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
