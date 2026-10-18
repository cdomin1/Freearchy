use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use crate::util;

/// ~/.config/vellum/current/theme -> <repo>/themes/<name>
/// App configs include files out of this symlinked dir, so a theme swap
/// is one relink + a compositor reload.
fn theme_link() -> Result<PathBuf> {
    Ok(util::config_home()?.join("vellum/current/theme"))
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
        bail!("no such theme: {name} (run `vellum theme list`)");
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
    apply_wallpaper(&src, name);
    // Live-apply what we can; missing commands are fine outside a session.
    let _ = util::capture("hyprctl", &["reload"]);
    let _ = util::capture("polybar-msg", &["cmd", "restart"]);
    Ok(())
}

/// Read a key from a theme's theme.toml (flat `key = "value"` lines).
fn theme_key(src: &PathBuf, key: &str) -> Option<String> {
    let text = fs::read_to_string(src.join("theme.toml")).ok()?;
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Apply the theme's wallpaper: convert the shipped file to PNG via
/// ImageMagick (feh/imlib2 webp support is unreliable), else render a
/// gradient from the palette. Applies with feh if a session is up.
fn apply_wallpaper(src: &PathBuf, name: &str) {
    let Ok(home) = util::home() else { return };
    let dir = home.join(".local/share/vellum");
    if fs::create_dir_all(&dir).is_err() { return }
    let out = dir.join("wallpaper.png");

    let shipped = theme_key(src, "wallpaper")
        .filter(|w| !w.is_empty())
        .map(|w| src.join(w))
        .filter(|p| p.exists());

    let ok = if let Some(file) = shipped {
        std::process::Command::new("magick")
            .arg(&file).arg(&out)
            .status().map(|s| s.success()).unwrap_or(false)
    } else {
        let (from, to) = (
            theme_key(src, "background").unwrap_or_else(|| "#1a1b26".into()),
            theme_key(src, "lighter_background").unwrap_or_else(|| "#24283b".into()),
        );
        std::process::Command::new("magick")
            .args(["-size", "1920x1080"])
            .arg(format!("gradient:{from}-{to}"))
            .arg(&out)
            .status().map(|s| s.success()).unwrap_or(false)
    };
    if ok {
        let _ = util::capture("feh", &["--bg-fill", &out.to_string_lossy()]);
    } else {
        eprintln!("    warn: wallpaper for {name} not applied (magick missing?)");
    }
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
