use anyhow::Result;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use crate::{packages, theme, util};

pub fn run(no_packages: bool, dev: bool, no_shell: bool) -> Result<()> {
    util::ensure_freebsd()?;
    let repo = util::repo_root()?;

    if !no_packages {
        packages::install_all(&repo, dev)?;
    }
    system_setup()?;
    link_configs(&repo)?;
    theme::set("tokyo-night")?;

    if !no_shell {
        let script = repo.join("shell/build-shell.sh");
        util::run("sh", &[script.to_str().unwrap()])?;
    }

    println!("\nfreearchy installed. Log in via a getty and run `Hyprland`,");
    println!("or install a display manager. Run `freearchy doctor` to verify.");
    Ok(())
}

/// Re-apply configs + theme after `git pull` on the repo.
pub fn update() -> Result<()> {
    let repo = util::repo_root()?;
    link_configs(&repo)?;
    println!("freearchy updated.");
    Ok(())
}

/// Services, groups and kernel modules needed for a Wayland session on FreeBSD.
fn system_setup() -> Result<()> {
    // Service enables — idempotent.
    for (service, _) in [("seatd", "on"), ("dbus", "on")] {
        util::run("sysrc", &[&format!("{service}_enable=YES")])?;
    }
    // `service X start` fails if the daemon is already running — check status.
    for svc in ["seatd", "dbus"] {
        let running = std::process::Command::new("service")
            .args([svc, "status"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !running {
            util::run("service", &[svc, "start"])?;
        }
    }

    // GPU kernel modules. We can't reliably detect the GPU here; doctor reports
    // what's loaded and docs explain the choice. Intel is the common case.
    println!("==> NOTE: ensure the right kms module loads at boot:");
    println!("    sysrc kld_list+=i915kms    # Intel");
    println!("    sysrc kld_list+=amdgpu     # AMD");

    // Seat + GPU access for the installing user.
    if let Ok(user) = std::env::var("SUDO_USER").or_else(|_| std::env::var("USER")) {
        let _ = util::run("pw", &["groupmod", "video", "-m", &user]);
    }
    Ok(())
}

/// Symlink repo-owned config into ~/.config (stow-style).
fn link_configs(repo: &Path) -> Result<()> {
    let cfg = util::config_home()?;
    fs::create_dir_all(&cfg)?;

    let pairs = [
        (repo.join("config/hyprland"), cfg.join("hypr")),
        (repo.join("config/foot"), cfg.join("foot")),
    ];
    for (src, dst) in pairs {
        link(&src, &dst)?;
    }
    Ok(())
}

fn link(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() || dst.is_symlink() {
        fs::remove_file(dst).ok();
        if dst.is_dir() && !dst.is_symlink() {
            fs::remove_dir_all(dst).ok();
        }
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    symlink(src, dst)?;
    println!("    {} -> {}", dst.display(), src.display());
    Ok(())
}
