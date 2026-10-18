//! `vellum power` — inspect and set the machine's power profile.
//! Thin Rust front-end over the same knobs the devd hook script uses:
//! HWP EPP hints, wifi powersave, CPU freq + battery readouts.

use anyhow::{bail, Result};

use crate::util;

/// Per-CPU EPP sysctls take an integer 0 (performance) … 100 (efficiency),
/// see hwpstate_intel(4). One OID per core: dev.hwpstate_intel.N.epp.
const EPP_DRIVERS: [&str; 2] = ["dev.hwpstate_intel", "dev.hwpstate_amd"];

/// (profile, EPP) — mirrors Linux's performance / balance_performance /
/// power hints rescaled from 0–255 to FreeBSD's 0–100.
const PROFILES: [(&str, u8); 3] = [("performance", 0), ("balanced", 50), ("power", 100)];

fn epp_mibs() -> Vec<String> {
    EPP_DRIVERS
        .iter()
        .filter_map(|d| util::capture("sysctl", &["-N", d]))
        .flat_map(|out| {
            out.lines()
                .filter(|l| l.ends_with(".epp"))
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn acpi_call(call: &str) -> Result<()> {
    // FreeBSD's acpi_call utility: `acpi_call -i "<method> <arg>"`
    let out = util::capture("acpi_call", &["-i", call])
        .or_else(|| util::capture("acpi_call", &[call]));
    match out {
        Some(v) if v.contains("0x80000000") => {
            bail!("ACPI rejected {call} (call failure status)")
        }
        Some(_) => Ok(()),
        None => bail!(
            "acpi_call unavailable — `pkg install acpi_call` + `kldload acpi_call` (amd64 ThinkPads)"
        ),
    }
}

/// ThinkPad charge thresholds via the embedded controller (tpacpi-bat
/// encoding: arg = percent | battery<<8 against HKEY.BCCS/BCSS).
pub fn battery(start: Option<u8>, stop: Option<u8>) -> Result<()> {
    if start.is_none() && stop.is_none() {
        bail!("usage: vellum power battery --start <pct> --stop <pct>");
    }
    let _ = util::capture("kldload", &["acpi_call"]); // no-op if loaded/absent
    // EC path varies by model: LPC on most, LPCB on newer (S2, 13g2…)
    for base in ["\\_SB.PCI0.LPC.EC.HKEY", "\\_SB.PCI0.LPCB.EC.HKEY"] {
        let probe = util::capture("acpi_call", &["-i", &format!("{base}.BDSS 0x0")])
            .or_else(|| util::capture("acpi_call", &[&format!("{base}.BDSS 0x0")]));
        if probe.is_none() {
            continue;
        }
        if let Some(p) = start {
            acpi_call(&format!("{base}.BCCS 0x{:x}", (p as u32) | 0x100))?;
        }
        if let Some(p) = stop {
            acpi_call(&format!("{base}.BCSS 0x{:x}", (p as u32) | 0x100))?;
        }
        return Ok(());
    }
    bail!("no ThinkPad EC found (not a ThinkPad, or acpi_call not loaded)")
}

fn sysctl_get(mib: &str) -> Option<String> {
    util::capture("sysctl", &["-n", mib]).map(|s| s.trim().to_string())
}

fn epp_set(v: u8) {
    for mib in epp_mibs() {
        let _ = util::capture("sysctl", &[&format!("{mib}={v}")]);
    }
}

fn wlan_ps(on: bool) {
    // powersave is set on the wlanN clone, not the parent (iwlwifi0 …)
    let arg = if on { "powersave" } else { "-powersave" };
    let ifs = util::capture("ifconfig", &["-l"]).unwrap_or_default();
    for i in ifs.split_whitespace().filter(|i| i.starts_with("wlan")) {
        let _ = util::capture("ifconfig", &[i, arg]);
    }
}

pub fn set(mode: &str) -> Result<()> {
    let Some(&(_, epp)) = PROFILES.iter().find(|(p, _)| *p == mode) else {
        bail!("unknown profile '{mode}' — use performance|balanced|power");
    };
    epp_set(epp);
    wlan_ps(mode == "power");
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
    if let Some(v) = epp_mibs().first().and_then(|m| sysctl_get(m)) {
        let name = PROFILES
            .iter()
            .find(|(_, e)| e.to_string() == v)
            .map_or("custom", |(p, _)| p);
        println!("epp:   {v} ({name})");
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
