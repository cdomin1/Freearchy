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
    Check { name: "hyprland installed", run: || bin("hyprland") },
    Check { name: "quickshell installed", run: || bin("quickshell") },
    Check { name: "foot installed", run: || bin("foot") },
    Check { name: "seatd enabled", run: || rcvar("seatd_enable") },
    Check { name: "dbus enabled", run: || rcvar("dbus_enable") },
    Check { name: "seatd running", run: || service("seatd") },
    Check { name: "drm kms module loaded", run: kms_loaded },
    Check { name: "XDG_RUNTIME_DIR set", run: xdg_runtime_dir },
    Check { name: "hyprland config linked", run: hypr_config },
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
    match util::capture("service", &[name, "status"]) {
        Some(_) => CheckResult::Ok("running".into()),
        None => CheckResult::Fail(format!("{name} not running")),
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
