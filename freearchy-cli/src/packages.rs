use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::util;

/// Package manifests, in install order.
const MANIFESTS: &[&str] = &[
    "base.txt",
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
        let pkgs = read_manifest(&repo.join("packages").join(manifest))?;
        if pkgs.is_empty() {
            continue;
        }
        println!("==> Installing {} ({} packages)", manifest, pkgs.len());
        let args: Vec<&str> = pkgs.iter().map(String::as_str).collect();
        let mut cmd = vec!["install", "-y"];
        cmd.extend(args);
        // TODO: don't fail the whole run on a single missing package —
        // collect failures and report at the end.
        util::run("pkg", &cmd).with_context(|| format!("manifest {manifest}"))?;
    }
    Ok(())
}

/// Parse a manifest: strip comments/blank lines, drop `name?` optional markers
/// (optional packages are attempted individually and ignored on failure).
pub fn read_manifest(path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(text
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.trim_end_matches('?').to_string())
        .collect())
}
