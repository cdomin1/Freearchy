use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use crate::util;

/// ~/.config/freearchy/current/theme -> <repo>/themes/<name>
/// App configs include files out of this symlinked dir, so a theme swap
/// is one relink + a compositor reload.
fn theme_link() -> Result<PathBuf> {
    Ok(util::config_home()?.join("freearchy/current/theme"))
}

fn themes_dir() -> Result<PathBuf> {
    Ok(util::repo_root()?.join("themes"))
}

fn available() -> Result<Vec<String>> {
    let dir = themes_dir()?;
    let mut names: Vec<String> = fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(names)
}

pub fn list() -> Result<()> {
    let current = current_name().unwrap_or_default();
    for name in available()? {
        let marker = if name == current { "*" } else { " " };
        println!("{marker} {name}");
    }
    Ok(())
}

pub fn set(name: &str) -> Result<()> {
    let src = themes_dir()?.join(name);
    if !src.is_dir() {
        bail!("no such theme: {name} (run `freearchy theme list`)");
    }
    let link = theme_link()?;
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    if link.exists() || link.is_symlink() {
        fs::remove_file(&link)?;
    }
    symlink(&src, &link)?;
    println!("theme: {name}");
    // Live-apply what we can; missing commands are fine outside a session.
    let _ = util::capture("hyprctl", &["reload"]);
    Ok(())
}

pub fn next() -> Result<()> {
    let themes = available()?;
    if themes.is_empty() {
        bail!("no themes found in {}", themes_dir()?.display());
    }
    let current = current_name().unwrap_or_default();
    let idx = themes.iter().position(|t| *t == current).unwrap_or(0);
    set(&themes[(idx + 1) % themes.len()])
}

fn current_name() -> Option<String> {
    fs::read_link(theme_link().ok()?)
        .ok()?
        .file_name()?
        .to_str()
        .map(String::from)
}
