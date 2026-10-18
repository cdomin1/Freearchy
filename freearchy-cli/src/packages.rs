use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::util;

/// Package manifests, in install order.
const MANIFESTS: &[&str] = &[
    "base.txt",
    "xorg.txt",
    "hyprland.txt",
    "shell.txt",
    "desktop.txt",
];

pub fn install_all(repo: &Path, include_dev: bool) -> Result<()> {
    let mut manifests = MANIFESTS.to_vec();
    if include_dev {
        manifests.push("dev.txt");
    }

    util::run("pkg", &["update"])?;

    for manifest in manifests {
        let (required, optional) = read_manifest(&repo.join("packages").join(manifest))?;

        if !required.is_empty() {
            println!("==> Installing {} ({} packages)", manifest, required.len());
            let mut cmd = vec!["install", "-y"];
            cmd.extend(required.iter().map(String::as_str));
            util::run("pkg", &cmd).with_context(|| format!("manifest {manifest}"))?;
        }

        for pkg in optional {
            if util::run("pkg", &["install", "-y", &pkg]).is_err() {
                eprintln!("    warn: optional package {pkg} unavailable, skipping");
            }
        }
    }
    Ok(())
}

/// Parse a manifest into (required, optional) package lists. Lines are
/// `name [# comment]`; a trailing `?` marks the package optional.
fn read_manifest(path: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let mut required = Vec::new();
    let mut optional = Vec::new();
    for line in text.lines() {
        let entry = line.split('#').next().unwrap_or("").trim();
        if entry.is_empty() {
            continue;
        }
        match entry.strip_suffix('?') {
            Some(name) => optional.push(name.trim().to_string()),
            None => required.push(entry.to_string()),
        }
    }
    Ok((required, optional))
}
