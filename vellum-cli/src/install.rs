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
        // GPU-conditional packages (nvidia-driver when detected on amd64)
        for pkg in crate::gpu::packages() {
            println!("==> Installing {pkg} (detected GPU)");
            if util::run("pkg", &["install", "-y", pkg]).is_err() {
                eprintln!("    warn: {pkg} failed to install");
            }
        }
    }
    system_setup(&repo)?;
    link_configs(&repo)?;
    install_wm(&repo)?;
    theme::set("tokyo-night")?;

    if !no_shell {
        // Shelved Wayland shell — never let it sink the X11 install.
        // Run with HOME aimed at the invoking user so QS_DIR/FONTS_DIR land in
        // their ~/.config / ~/.local even though pkg runs as root.
        let script = repo.join("shell/build-shell.sh");
        let home = util::home()?;
        let ok = std::process::Command::new("sh")
            .arg(&script)
            .env("HOME", &home)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            eprintln!("    warn: Caelestia shell build failed — X11 session unaffected");
        }
    }

    // Root-owned dirs created above belong to the desktop user.
    if let Some(user) = util::invoking_user() {
        let home = util::home()?;
        for dir in [home.join(".config"), home.join(".local")] {
            if dir.exists() {
                let _ = util::run(
                    "chown",
                    &["-R", &format!("{user}:{user}"), &dir.to_string_lossy()],
                );
            }
        }
    }

    println!("\nvellum installed.");
    println!("  X11 session (default): run `startx`  → vellum-wm");
    println!("  Wayland (shelved):     run `Hyprland` → Caelestia shell");
    println!("Run `vellum doctor` to verify.");
    Ok(())
}

/// Build + install vellum-wm (the X11 tiler) from this workspace.
fn install_wm(repo: &Path) -> Result<()> {
    println!("==> Building vellum-wm");
    if !util::on_path("cargo") {
        eprintln!("    warn: cargo not on PATH — skipping vellum-wm build");
        return Ok(());
    }
    let path = repo.join("vellum-wm");
    util::run("cargo", &[
        "install", "--path", &path.to_string_lossy(), "--root", "/usr/local",
    ])?;
    Ok(())
}

/// Re-apply configs + theme after `git pull` on the repo.
pub fn update() -> Result<()> {
    let repo = util::repo_root()?;
    link_configs(&repo)?;
    println!("vellum updated.");
    Ok(())
}

/// Services, groups and kernel modules for the desktop session.
fn system_setup(repo: &Path) -> Result<()> {
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

    // GPU kernel modules for whatever pciconf reports (sysrc += dedups).
    let klds = crate::gpu::kld_list();
    if !klds.is_empty() {
        util::run("sysrc", &[&format!("kld_list+={klds}")])?;
        for m in klds.split_whitespace() {
            let _ = util::capture("kldload", &["-n", m]);
        }
        // AMD cards the codename table missed: amdgpu won't claim SI/CIK
        let amd = crate::gpu::probe_amd_fallback();
        if !amd.is_empty() {
            println!("==> AMD KMS: {}", amd.join(", "));
        }
    }
    // nvidia-drm needs modeset enabled before it loads
    let lc = "/boot/loader.conf";
    let mut loader = fs::read_to_string(lc).unwrap_or_default();
    for line in crate::gpu::loader_conf_lines() {
        let key = line.split('=').next().unwrap_or(line);
        if !loader.contains(key) {
            loader.push_str(&format!("{line}\n"));
        }
    }
    fs::write(lc, loader)?;

    // Seat + GPU access for the user who invoked the install (via doas/sudo).
    if let Some(user) = util::invoking_user() {
        let _ = util::run("pw", &["groupmod", "video", "-m", &user]);
    }

    // Pin the right X11 driver for detected hardware (hybrid-safe).
    crate::gpu::write_xorg_conf("");
    // Session helpers on PATH: prime-run (dGPU offload), vellum-menu +
    // vellum-shot (rofi menus, screenshot keys).
    use std::os::unix::fs::PermissionsExt;
    for src in [
        repo.join("config/x11/prime-run"),
        repo.join("config/x11/bin/vellum-menu"),
        repo.join("config/x11/bin/vellum-shot"),
    ] {
        let dst = format!("/usr/local/bin/{}", src.file_name().unwrap().to_string_lossy());
        let _ = fs::copy(&src, &dst);
        let _ = fs::set_permissions(&dst, fs::Permissions::from_mode(0o755));
    }

    power_setup(repo)?;
    Ok(())
}

/// Suspend config for the live system: best sleep state into
/// /etc/sysctl.conf (applied live too), rc.suspend/rc.resume hooks,
/// and a nopass doas rule so Super+Shift+Esc can `zzz` unprivileged.
fn power_setup(repo: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    match crate::install_system::best_sleep_state() {
        None => println!("==> no ACPI sleep states detected — skipping suspend config"),
        Some(state) => {
            println!("==> suspend: {state} (lid + sleep button)");
            // sysctl.conf — append missing lines rather than clobber.
            let sc = "/etc/sysctl.conf";
            let mut text = fs::read_to_string(sc).unwrap_or_default();
            for (k, v) in [
                ("hw.acpi.suspend_state", state.as_str()),
                ("hw.acpi.lid_switch_state", state.as_str()),
                ("hw.acpi.sleep_button_state", state.as_str()),
                ("hw.acpi.power_button_state", "S5"),
            ] {
                if !text.contains(k) {
                    text.push_str(&format!("{k}={v}\n"));
                }
                let _ = util::run("sysctl", &[&format!("{k}={v}")]);
            }
            fs::write(sc, text)?;

            // suspend/resume hooks
            for f in ["rc.suspend", "rc.resume"] {
                let dst = format!("/etc/{f}");
                fs::copy(repo.join(format!("config/power/{f}")), &dst)?;
                fs::set_permissions(&dst, fs::Permissions::from_mode(0o755))?;
            }
        }
    }

    // wheel can suspend without a password (Omarchy-style convenience)
    let doas = "/usr/local/etc/doas.conf";
    let mut d = fs::read_to_string(doas).unwrap_or_default();
    if !d.contains("cmd zzz") {
        d.push_str("permit nopass :wheel cmd zzz\npermit nopass :wheel cmd acpiconf\n");
        fs::write(doas, d)?;
    }

    // profile script + devd AC hook + adaptive CPU freq daemon
    fs::create_dir_all("/usr/local/libexec")?;
    fs::copy(repo.join("config/power/vellum-power"), "/usr/local/libexec/vellum-power")?;
    fs::set_permissions("/usr/local/libexec/vellum-power", fs::Permissions::from_mode(0o755))?;
    fs::create_dir_all("/usr/local/etc/devd")?;
    fs::copy(repo.join("config/power/devd.conf"), "/usr/local/etc/devd/vellum-power.conf")?;
    util::run("sysrc", &["powerdxx_enable=YES"])?;
    let _ = util::run("service", &["powerdxx", "start"]);
    let _ = util::run("/usr/local/libexec/vellum-power", &["ac"]);

    // driverless PCI devices → D3 (the knob that exists; FreeBSD has no
    // Linux-style ASPM policy manager)
    let lc = "/boot/loader.conf";
    let mut l = fs::read_to_string(lc).unwrap_or_default();
    if !l.contains("do_power_nodriver") {
        l.push_str("hw.pci.do_power_nodriver=\"3\"\n");
        let _ = fs::write(lc, l);
    }
    Ok(())
}

/// Symlink repo-owned config into ~/.config (stow-style).
fn link_configs(repo: &Path) -> Result<()> {
    let cfg = util::config_home()?;
    fs::create_dir_all(&cfg)?;

    let pairs = [
        (repo.join("config/hyprland"), cfg.join("hypr")),
        (repo.join("config/x11"), cfg.join("vellum/x11")),
        (repo.join("config/x11/rofi"), cfg.join("rofi")),
        (repo.join("config/x11/alacritty"), cfg.join("alacritty")),
        (repo.join("config/x11/polybar"), cfg.join("polybar")),
        (repo.join("config/x11/gtk-3.0"), cfg.join("gtk-3.0")),
    ];
    for (src, dst) in pairs {
        link(&src, &dst)?;
    }
    // startx entry point + GTK2 rc for stragglers
    let home = util::home()?;
    link(&cfg.join("vellum/x11/xinitrc"), &home.join(".xinitrc"))?;
    link(&cfg.join("vellum/x11/gtkrc-2.0"), &home.join(".gtkrc-2.0"))?;

    // hicolor icon set + desktop entry so `vellum-menu` shows in rofi drun
    let icons = home.join(".local/share/icons/hicolor");
    for size in [16, 24, 32, 48, 64, 128, 256] {
        let src = repo.join(format!("branding/icons/vellum-{size}.png"));
        let dst = icons.join(format!("{size}x{size}/apps/vellum.png"));
        if src.exists() {
            link(&src, &dst)?;
        }
    }
    link(
        &repo.join("config/x11/applications/vellum-menu.desktop"),
        &home.join(".local/share/applications/vellum-menu.desktop"),
    )?;
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
