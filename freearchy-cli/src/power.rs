//! `freearchy power` — inspect and set the machine's power profile.
//! Thin Rust front-end over the same knobs the devd hook script uses:
//! HWP EPP hints, wifi powersave, CPU freq + battery readouts.

use anyhow::{bail, Result};

use crate::util;

const EPP_MIBS: [&str; 2] = ["dev.hwpstate_intel.0.epp", "dev.hwpstate_amd.0.epp"];

fn sysctl_get(mib: &str) -> Option<String> {
    util::capture("sysctl", &["-n", mib]).map(|s| s.trim().to_string())
}

fn epp_set(v: &str) {
    for mib in EPP_MIBS {
        if sysctl_get(mib).is_some() {
            let _ = util::capture("sysctl", &[&format!("{mib}={v}")]);
            return;
        }
    }
}

fn wlan_ps(on: bool) {
    let arg = if on { "powersave" } else { "-powersave" };
    if let Some(devs) = sysctl_get("net.wlan.devices") {
        for i in devs.split_whitespace() {
            let _ = util::capture("ifconfig", &[i, arg]);
        }
    }
}

pub fn set(mode: &str) -> Result<()> {
    match mode {
        "performance" => {
            epp_set("performance");
            wlan_ps(false);
        }
        "balanced" => {
            epp_set("balance_performance");
            wlan_ps(false);
        }
        "power" => {
            epp_set("power");
            wlan_ps(true);
        }
        _ => bail!("unknown profile '{mode}' — use performance|balanced|power"),
    }
    println!("power profile: {mode}");
    Ok(())
}

pub fn status() -> Result<()> {
    let ac = sysctl_get("hw.acpi.acline").unwrap_or_else(|| "?".into());
    let on_ac = ac.trim() == "1";
    println!("ac:    {}", if on_ac { "plugged" } else { "battery" });
    if let Some(f) = sysctl_get("dev.cpu.0.freq") {
        println!("freq:  {f} MHz");
    }
    for mib in EPP_MIBS {
        if let Some(v) = sysctl_get(mib) {
            println!("epp:   {v}");
            break;
        }
    }
    if let Some(out) = util::capture("acpiconf", &["-i0"]) {
        for line in out.lines() {
            let l = line.trim();
            if l.starts_with("Remaining capacity")
                || l.starts_with("Present rate")
                || l.starts_with("State:")
            {
                println!("bat:   {l}");
            }
        }
    }
    Ok(())
}
