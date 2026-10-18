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
    install_wm(&repo)?;
    install_picom(&repo)?;
    gen_wallpaper()?;
    theme::set("tokyo-night")?;

    if !no_shell {
        // Run with HOME aimed at the invoking user so QS_DIR/FONTS_DIR land in
        // their ~/.config / ~/.local even though pkg runs as root.
        let script = repo.join("shell/build-shell.sh");
        let home = util::home()?;
        let status = std::process::Command::new("sh")
            .arg(&script)
            .env("HOME", &home)
            .status()?;
        anyhow::ensure!(status.success(), "build-shell.sh exited with {status}");
    }

    // Root-owned dirs created above belong to the desktop user.
    if let Some(user) = util::invoking_user() {
        let home = util::home()?;
        for dir in [home.join(".config"), home.join(".local/share")] {
            if dir.exists() {
                let _ = util::run(
                    "chown",
                    &["-R", &format!("{user}:{user}"), dir.to_str().unwrap()],
                );
            }
        }
    }

    println!("\nfreearchy installed.");
    println!("  X11 session (default): run `startx`  → freearchy-wm");
    println!("  Wayland (shelved):     run `Hyprland` → Caelestia shell");
    println!("Run `freearchy doctor` to verify.");
    Ok(())
}

/// Build + install freearchy-wm (the X11 tiler) from this workspace.
fn install_wm(repo: &Path) -> Result<()> {
    println!("==> Building freearchy-wm");
    if !util::on_path("cargo") {
        eprintln!("    warn: cargo not on PATH — skipping freearchy-wm build");
        return Ok(());
    }
    let path = repo.join("freearchy-wm");
    util::run("cargo", &[
        "install", "--path", &path.to_string_lossy(), "--root", "/usr/local",
    ])?;
    Ok(())
}

/// Build the animated picom fork (pijulius/picom) — not in ports.
fn install_picom(repo: &Path) -> Result<()> {
    println!("==> Building picom (pijulius animation fork)");
    let home = util::home()?;
    let status = std::process::Command::new("sh")
        .arg(repo.join("x11/build-picom.sh"))
        .env("HOME", &home)
        .status()?;
    if !status.success() {
        // Non-fatal: a stock picom pkg or none still leaves a working session.
        eprintln!("    warn: picom build failed — session still works, no animations");
    }
    Ok(())
}

/// Generate the default wallpaper — a Tokyo Night gradient via ImageMagick.
/// Non-fatal: xinitrc falls back to `xsetroot -solid` if it's absent.
fn gen_wallpaper() -> Result<()> {
    let dir = util::home()?.join(".local/share/freearchy");
    fs::create_dir_all(&dir)?;
    let wall = dir.join("wallpaper.png");
    if wall.exists() {
        return Ok(());
    }
    let status = std::process::Command::new("magick")
        .args(["-size", "1920x1080", "gradient:#1a1b26-#24283b"])
        .arg(&wall)
        .status();
    match status {
        Ok(s) if s.success() => {}
        _ => eprintln!("    warn: wallpaper generation skipped (magick missing?)"),
    }
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

    // Seat + GPU access for the user who invoked the install (via doas/sudo).
    if let Some(user) = util::invoking_user() {
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
        (repo.join("config/x11"), cfg.join("freearchy/x11")),
        (repo.join("config/x11/rofi"), cfg.join("rofi")),
        (repo.join("config/x11/alacritty"), cfg.join("alacritty")),
    ];
    for (src, dst) in pairs {
        link(&src, &dst)?;
    }
    // startx entry point
    let home = util::home()?;
    link(&cfg.join("freearchy/x11/xinitrc"), &home.join(".xinitrc"))?;
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
