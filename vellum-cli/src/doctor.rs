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
    Check { name: "hyprland installed (wayland shelf)", run: || bin("hyprland") },
    Check { name: "quickshell installed", run: || bin("quickshell") },
    Check { name: "foot installed", run: || bin("foot") },
    Check { name: "seatd enabled", run: || rcvar("seatd_enable") },
    Check { name: "dbus enabled", run: || rcvar("dbus_enable") },
    Check { name: "seatd running", run: || service("seatd") },
    Check { name: "gpu detected / driver", run: gpu_state },
    Check { name: "drm kms module loaded", run: kms_loaded },
    Check { name: "XDG_RUNTIME_DIR set", run: xdg_runtime_dir },
    Check { name: "hyprland config linked", run: hypr_config },
    Check { name: "suspend support", run: sleep_state },
    Check { name: "apple hardware", run: apple_hardware },
];

fn bin(name: &str) -> CheckResult {
    if util::on_path(name) {
        CheckResult::Ok(name.into())
    } else {
        CheckResult::Fail(format!("{name} not on PATH"))
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

fn gpu_state() -> CheckResult {
    let gpus = crate::gpu::detect();
    if gpus.is_empty() {
        return CheckResult::Warn("no known GPU (virt/framebuffer?)".into());
    }
    let names: Vec<&str> = gpus.iter().map(|g| g.label()).collect();
    // warn if the expected kmod for the detected vendor isn't loaded
    let loaded = util::capture("kldstat", &[]).unwrap_or_default();
    let missing: Vec<&str> = gpus
        .iter()
        .filter_map(|g| match g {
            crate::gpu::Gpu::Intel if !loaded.contains("i915kms") => Some("i915kms"),
            crate::gpu::Gpu::Amd if !loaded.contains("amdgpu") => Some("amdgpu"),
            crate::gpu::Gpu::Nvidia if !loaded.contains("nvidia") => Some("nvidia"),
            _ => None,
        })
        .collect();
    if missing.is_empty() {
        let hybrid = if crate::gpu::is_hybrid() {
            " — hybrid: desktop on iGPU, offload via prime-run"
        } else {
            ""
        };
        CheckResult::Ok(format!("{}{hybrid}", names.join(", ")))
    } else {
        CheckResult::Warn(format!(
            "{} — kmod not loaded: {} (sysrc kld_list)",
            names.join(", "),
            missing.join(", ")
        ))
    }
}

fn kms_loaded() -> CheckResult {
    let out = util::capture("kldstat", &[]).unwrap_or_default();
    for m in ["i915kms", "amdgpu", "nvidia-modeset", "vmwgfx"] {
        if out.contains(m) {
            return CheckResult::Ok(m.into());
        }
    }
    CheckResult::Fail("no kms module loaded — see docs (sysrc kld_list)".into())
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
        Err(_) => return CheckResult::Fail("no config home".into()),
    };
    if path.exists() {
        CheckResult::Ok(path.display().to_string())
    } else {
        CheckResult::Fail(format!("{} missing", path.display()))
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
        println!("\n{failures} check(s) failed.");
    }
    Ok(())
}
