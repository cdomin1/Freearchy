//! `vellum wifi` — minimal Wi-Fi over ifconfig + wpa_supplicant.
//!
//! EXPERIMENTAL. FreeBSD wireless on modern chipsets runs through the
//! LinuxKPI compat layer (iwlwifi/rtw88/rtw89) and has real gaps: some
//! cards associate but drop under load, and there is no iwd/NetworkManager
//! equivalent. For anything flaky, run `vellum doctor` and check
//! `dmesg | grep -i firmware`.

use anyhow::{bail, Context, Result};

use crate::util;

/// First wlanN interface, creating one from a net.wlan.devices parent if
/// the clone doesn't exist yet.
fn wlan() -> Option<String> {
    let ifaces = util::capture("ifconfig", &["-l"]).unwrap_or_default();
    if let Some(w) = ifaces.split_whitespace().find(|i| i.starts_with("wlan")) {
        return Some(w.to_string());
    }
    // create from the first parent device (iwlwifi0, rtw880, …)
    let parents = util::capture("sysctl", &["-n", "net.wlan.devices"]).unwrap_or_default();
    let parent = parents.split_whitespace().next()?;
    util::capture("ifconfig", &["wlan0", "create", "wlandev", parent])?;
    Some("wlan0".into())
}

pub fn status() -> Result<()> {
    let Some(w) = wlan() else {
        bail!("no wireless device (net.wlan.devices empty — driver/firmware missing?)");
    };
    let out = util::capture("ifconfig", &[&w]).unwrap_or_default();
    println!("{out}");
    Ok(())
}

/// One SSID per line (deduped, strongest first — ifconfig lists in order).
pub fn scan() -> Result<Vec<String>> {
    let Some(w) = wlan() else {
        bail!("no wireless device");
    };
    let _ = util::capture("ifconfig", &[&w, "up", "scan"]);
    let out = util::capture("ifconfig", &[&w, "list", "scan"])
        .context("scan failed — firmware loaded? see `dmesg`")?;
    let mut ssids: Vec<String> = Vec::new();
    for line in out.lines().skip(1) {
        // SSID may contain spaces; it ends where the BSSID (xx:xx:…) starts.
        let mut end = line.len();
        for tok in line.split_whitespace() {
            if tok.matches(':').count() == 5 {
                end = line.find(tok).unwrap_or(line.len());
                break;
            }
        }
        let ssid = line[..end].trim();
        if !ssid.is_empty() && !ssids.iter().any(|s| s == ssid) {
            ssids.push(ssid.to_string());
        }
    }
    for s in &ssids {
        println!("{s}");
    }
    Ok(ssids)
}

/// Connect: appends a wpa_supplicant network block, sets rc.conf and
/// restarts netif. Open networks get key_mgmt=NONE.
pub fn connect(ssid: &str, pass: Option<&str>) -> Result<()> {
    let Some(w) = wlan() else {
        bail!("no wireless device");
    };
    let conf = "/etc/wpa_supplicant.conf";
    let mut text = std::fs::read_to_string(conf).unwrap_or_default();
    if !text.contains(&format!("ssid=\"{ssid}\"")) {
        let block = match pass {
            Some(p) => format!("network={{\n    ssid=\"{ssid}\"\n    psk=\"{p}\"\n}}\n"),
            None => format!("network={{\n    ssid=\"{ssid}\"\n    key_mgmt=NONE\n}}\n"),
        };
        text.push_str(&block);
        std::fs::write(conf, text)?;
    }
    util::run("sysrc", &[&format!("ifconfig_{w}=WPA SYNCDHCP")])?;
    util::run("service", &["netif", "restart", &w])?;
    println!("connecting {w} to {ssid} — check `ifconfig {w}` in a few seconds");
    Ok(())
}
