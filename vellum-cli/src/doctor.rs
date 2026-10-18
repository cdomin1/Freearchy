use anyhow::Result;

use crate::util;

struct Check {
    name: &'static str,
    run: fn() -> CheckResult,
}

enum CheckResult {
    Ok(String),
    Warn(String),
    Fail(String),
}

const CHECKS: &[Check] = &[
    Check { name: "vellum-wm installed", run: || bin("vellum-wm") },
    Check { name: "Xorg installed", run: || bin("Xorg") },
    Check { name: "picom installed", run: || bin("picom") },
    Check { name: "hyprland installed (wayland shelf)", run: || optional_bin("hyprland") },
    Check { name: "quickshell installed (wayland shelf)", run: || optional_bin("quickshell") },
    Check { name: "polybar installed", run: || bin("polybar") },
    Check { name: "alacritty installed", run: || bin("alacritty") },
    Check { name: "rofi installed", run: || bin("rofi") },
    Check { name: "thunar installed", run: || bin("thunar") },
    Check { name: "slock installed", run: || bin("slock") },
    Check { name: "seatd enabled", run: || rcvar("seatd_enable") },
    Check { name: "dbus enabled", run: || rcvar("dbus_enable") },
    Check { name: "seatd running", run: || service("seatd") },
    Check { name: "gpu detected / driver", run: gpu_state },
    Check { name: "drm kms module loaded", run: kms_loaded },
    Check { name: "XDG_RUNTIME_DIR set", run: xdg_runtime_dir },
    Check { name: "xinitrc linked", run: xinitrc },
    Check { name: "theme active", run: theme_active },
    Check { name: "hyprland config linked (wayland shelf)", run: hypr_config },
    Check { name: "suspend support", run: sleep_state },
    Check { name: "suspend hooks installed", run: suspend_hooks },
    Check { name: "bluetooth", run: bt_state },
    Check { name: "apple hardware", run: apple_hardware },
];

fn bin(name: &str) -> CheckResult {
    if util::on_path(name) {
        CheckResult::Ok(name.into())
    } else {
        CheckResult::Fail(format!("{name} not on PATH"))
    }
}

/// Shelved components: report presence, never count as a failure.
fn optional_bin(name: &str) -> CheckResult {
    match bin(name) {
        CheckResult::Fail(msg) => CheckResult::Warn(msg),
        ok => ok,
    }
}

fn rcvar(var: &str) -> CheckResult {
    match util::capture("sysrc", &["-n", var]) {
        Some(v) if v.trim() == "YES" => CheckResult::Ok(format!("{var}=YES")),
        Some(v) => CheckResult::Fail(format!("{var}={}", v.trim())),
        None => CheckResult::Warn(format!("{var} unset")),
    }
}

fn service(name: &str) -> CheckResult {
    if util::capture("service", &[name, "status"]).is_some() {
        return CheckResult::Ok("running".into());
    }
    // Some rc.d scripts (seatd) don't implement `status` — check the table.
    if util::capture("pgrep", &["-q", "-x", name]).is_some() {
        return CheckResult::Ok("running".into());
    }
    CheckResult::Fail(format!("{name} not running"))
}

/// One line per adapter: driver bound or why not, plus the hybrid roles.
fn gpu_state() -> CheckResult {
    use crate::gpu::{self, Vendor};
    let gpus = gpu::detect();
    if gpus.is_empty() {
        return CheckResult::Warn("no display adapter found by pciconf".into());
    }
    let loaded = util::capture("kldstat", &["-v"]).unwrap_or_default();
    let primary = gpu::primary(&gpus).cloned();
    let offload: Vec<gpu::Gpu> = gpu::offload(&gpus).into_iter().cloned().collect();
    let mut problems = 0;
    let lines: Vec<String> = gpus
        .iter()
        .map(|g| {
            let role = if Some(g) == primary.as_ref() {
                " (session)"
            } else if offload.contains(g) {
                " (offload: prime-run)"
            } else {
                ""
            };
            let state = match g.vendor {
                Vendor::Virtual => "VM display — scfb, no KMS needed".to_string(),
                Vendor::Other => "unknown vendor — scfb".to_string(),
                // the proprietary driver doesn't register under hw.dri
                Vendor::Nvidia if loaded.contains("nvidia") => "nvidia loaded".to_string(),
                _ => match gpu::attached_driver(g) {
                    Some(d) => format!("{d} attached"),
                    None => {
                        problems += 1;
                        let hint = match g.vendor {
                            Vendor::Intel => " (xe-only chip like Lunar Lake/Battlemage? drm-kmod has no xe)",
                            Vendor::Amd => " (try `kldload radeonkms` for SI/CIK cards)",
                            _ => "",
                        };
                        format!("no driver attached — X uses scfb{hint}")
                    }
                },
            };
            format!("\n         {}{role}: {state}", g.label())
        })
        .collect();
    let text = lines.concat();
    if problems > 0 { CheckResult::Warn(text) } else { CheckResult::Ok(text) }
}

fn kms_loaded() -> CheckResult {
    let out = util::capture("kldstat", &[]).unwrap_or_default();
    let found: Vec<&str> = ["i915kms", "amdgpu", "radeonkms", "nvidia-drm", "nvidia-modeset", "nvidia.ko"]
        .into_iter()
        .filter(|m| out.contains(m))
        .collect();
    if found.is_empty() {
        // X11 still runs on the EFI framebuffer (scfb) — fine in VMs.
        CheckResult::Warn("no kms module loaded — X11 falls back to scfb (sysrc kld_list)".into())
    } else {
        CheckResult::Ok(found.join(", "))
    }
}

fn xdg_runtime_dir() -> CheckResult {
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(v) if !v.is_empty() => CheckResult::Ok(v),
        _ => CheckResult::Warn("unset — install/enable turnstile".into()),
    }
}

fn hypr_config() -> CheckResult {
    let path = match util::config_home() {
        Ok(p) => p.join("hypr/hyprland.conf"),
        Err(_) => return CheckResult::Warn("no config home".into()),
    };
    if path.exists() {
        CheckResult::Ok(path.display().to_string())
    } else {
        CheckResult::Warn(format!("{} missing", path.display()))
    }
}

fn xinitrc() -> CheckResult {
    match util::home() {
        Ok(h) if h.join(".xinitrc").exists() => CheckResult::Ok("~/.xinitrc → vellum session".into()),
        Ok(_) => CheckResult::Fail("~/.xinitrc missing — run `vellum update`".into()),
        Err(_) => CheckResult::Fail("HOME unset".into()),
    }
}

fn theme_active() -> CheckResult {
    let link = match util::config_home() {
        Ok(c) => c.join("vellum/current/theme"),
        Err(_) => return CheckResult::Fail("no config home".into()),
    };
    match std::fs::read_link(&link) {
        Ok(t) if t.is_dir() => CheckResult::Ok(
            t.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        ),
        Ok(t) => CheckResult::Fail(format!("theme link points at missing {}", t.display())),
        Err(_) => CheckResult::Fail("no theme set — `vellum theme set tokyo-night`".into()),
    }
}

fn sleep_state() -> CheckResult {
    let states = ["kern.power.supported_stype", "hw.acpi.supported_sleep_states"]
        .iter()
        .find_map(|m| util::capture("sysctl", &["-n", m]));
    let Some(s) = states else {
        return CheckResult::Warn("no sleep-state sysctl (VM?)".into());
    };
    let supported: Vec<&str> = s.split_whitespace().collect();
    let configured = util::capture("sysctl", &["-n", "hw.acpi.lid_switch_state"])
        .unwrap_or_else(|| "?".into());
    let detail = |st: &str| format!("{st} (lid={})", configured.trim());
    if supported.contains(&"S3") {
        CheckResult::Ok(detail("S3"))
    } else if supported.iter().any(|t| t.eq_ignore_ascii_case("s2idle")) {
        CheckResult::Ok(detail("s2idle"))
    } else {
        CheckResult::Fail(format!("no usable sleep state: {}", s.trim()))
    }
}

fn bt_state() -> CheckResult {
    let ubt = (0..4)
        .map(|n| format!("ubt{n}"))
        .find(|d| std::path::Path::new(&format!("/dev/{d}")).exists());
    let Some(dev) = ubt else {
        return CheckResult::Ok("no controller attached".into());
    };
    // FreeBSD only drives Bluetooth over USB (ng_ubt) — firmware for
    // Intel/Realtek comes from iwmbt-firmware / rtlbt-firmware.
    let loaded = util::capture("kldstat", &["-v"]).unwrap_or_default();
    let mut notes = vec![dev];
    if !loaded.contains("ng_ubt") {
        notes.push("ng_ubt not loaded — `vellum bt on`".into());
    }
    if util::capture("service", &["bluetooth", "status"]).is_none() {
        notes.push("service stopped".into());
    }
    CheckResult::Ok(notes.join(", "))
}

fn suspend_hooks() -> CheckResult {
    let missing: Vec<&str> = ["/etc/rc.suspend", "/etc/rc.resume"]
        .into_iter()
        .filter(|p| !std::path::Path::new(p).exists())
        .collect();
    if !missing.is_empty() {
        return CheckResult::Warn(format!("{} missing — resume won't be cleaned up", missing.join(", ")));
    }
    // lock-before-sleep only works if a locker exists
    if !util::on_path("slock") {
        return CheckResult::Warn("hooks installed but slock missing — resume lands unlocked".into());
    }
    CheckResult::Ok("rc.suspend/rc.resume + slock".into())
}

fn apple_hardware() -> CheckResult {
    let model = util::capture("sysctl", &["-n", "hw.model"]).unwrap_or_default();
    let vendor = util::capture("sysctl", &["-n", "hw.vendor"]).unwrap_or_default();
    let hay = format!("{model} {vendor}");
    if !hay.to_lowercase().contains("apple") && !model.starts_with("Mac") {
        return CheckResult::Ok("n/a".into());
    }
    // T2-era Macs (2018+) bury storage/input behind the security chip.
    let t2 = ["MacBookPro15", "MacBookPro16", "MacBookAir8", "MacBookAir9",
              "iMacPro", "Macmini8", "iMac20", "MacPro7", "iMac21"]
        .iter().any(|m| model.contains(m));
    if t2 {
        CheckResult::Warn(format!(
            "{} — T2 chip: storage/keyboard may not be visible; Broadcom wifi unsupported. Prefer USB NIC. See docs/apple.md",
            model.trim()
        ))
    } else {
        CheckResult::Warn(format!(
            "{} — Intel Mac: use wsp/atp for the trackpad, expect Broadcom wifi gaps. See docs/apple.md",
            model.trim()
        ))
    }
}

pub fn run() -> Result<()> {
    let mut failures = 0;
    for check in CHECKS {
        match (check.run)() {
            CheckResult::Ok(msg) => println!("  ok   {} — {msg}", check.name),
            CheckResult::Warn(msg) => println!("  warn {} — {msg}", check.name),
            CheckResult::Fail(msg) => {
                failures += 1;
                println!("  FAIL {} — {msg}", check.name);
            }
        }
    }
    if failures > 0 {
        anyhow::bail!("{failures} check(s) failed");
    }
    Ok(())
}
