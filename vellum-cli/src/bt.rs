//! `vellum bt` — Bluetooth over FreeBSD's netgraph stack (EXPERIMENTAL).
//!
//! This is not BlueZ: discovery and baseband connections go through
//! `hccontrol`, authentication/PIN pairing through `hcsecd` and
//! /etc/bluetooth/hcsecd.conf, and audio reaches the desktop via
//! virtual_oss (/dev/bluetooth/*), not a PipeWire profile. BLE support in
//! the stack is minimal — expect classic-BR/EDR devices to pair best.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

use crate::util;

/// First attached USB Bluetooth controller ("ubt0" from /dev/ubt0).
fn controller() -> Option<String> {
    (0..4).map(|n| format!("ubt{n}")).find(|d| Path::new(&format!("/dev/{d}")).exists())
}

/// The HCI node name hccontrol wants ("ubt0hci" for ubt0).
fn hci(dev: &str) -> String {
    format!("{dev}hci")
}

pub fn on() -> Result<()> {
    let _ = util::capture("kldload", &["-n", "ng_ubt"]);
    let Some(dev) = controller() else {
        bail!("no /dev/ubtN — is a USB Bluetooth controller attached? (Bluetooth is USB or UART only on FreeBSD)");
    };
    util::run("sysrc", &["bluetooth_enable=YES", "hcsecd_enable=YES"])?;
    util::run("service", &["bluetooth", "start", &dev])?;
    util::run("service", &["hcsecd", "start"])?;
    Ok(())
}

pub fn off() -> Result<()> {
    if let Some(dev) = controller() {
        let _ = util::run("service", &["bluetooth", "stop", &dev]);
    }
    Ok(())
}

pub fn status() -> Result<()> {
    let Some(dev) = controller() else {
        println!("no controller (/dev/ubtN absent)");
        return Ok(());
    };
    let hci = hci(&dev);
    for cmd in ["Read_Local_Name", "Read_BD_ADDR"] {
        let out = util::capture("hccontrol", &["-n", &hci, cmd]).unwrap_or_default();
        println!("{cmd}: {}", out.trim());
    }
    let svc = util::capture("service", &["bluetooth", "status", &dev])
        .map(|_| "running")
        .unwrap_or("stopped");
    println!("bluetooth service: {svc} ({dev})");
    let secs = util::capture("service", &["hcsecd", "status"])
        .map(|_| "running")
        .unwrap_or("stopped");
    println!("hcsecd (pairing): {secs}");
    Ok(())
}

/// Scan — prints one `bdaddr  class` line per found device.
/// Classic inquiry doesn't return names; use `pair` then check status.
pub fn scan() -> Result<()> {
    let Some(dev) = controller() else {
        bail!("no /dev/ubtN — run `vellum bt on` first");
    };
    let out = util::capture("hccontrol", &["-n", &hci(&dev), "inquiry"])
        .context("inquiry failed — is the bluetooth service running?")?;
    let mut found = false;
    let mut addr = "";
    for line in out.lines() {
        let l = line.trim();
        if let Some(a) = l.strip_prefix("BD_ADDR:").map(str::trim) {
            addr = a;
        } else if l.starts_with("Class of device:") && !addr.is_empty() {
            println!("{addr}  {}", l.trim_start_matches("Class of device:").trim());
            found = true;
            addr = "";
        }
    }
    if !found {
        println!("(nothing found — or raw inquiry output below)");
        print!("{out}");
    }
    Ok(())
}

/// Register a PIN for a device with hcsecd, then open the baseband
/// connection (hcsecd answers the auth request with the PIN).
pub fn pair(bdaddr: &str, pin: &str) -> Result<()> {
    let conf = "/etc/bluetooth/hcsecd.conf";
    let mut text = fs::read_to_string(conf).unwrap_or_default();
    if !text.contains(bdaddr) {
        text.push_str(&format!(
            "device {{\n    bdaddr  {bdaddr};\n    key     nokey;\n    pin     \"{pin}\";\n}}\n"
        ));
        fs::write(conf, text)?;
    }
    let _ = util::run("service", &["hcsecd", "restart"]);
    connect(bdaddr)
}

pub fn connect(bdaddr: &str) -> Result<()> {
    let Some(dev) = controller() else {
        bail!("no /dev/ubtN");
    };
    util::run("hccontrol", &["-n", &hci(&dev), "create_connection", bdaddr])?;
    println!("connected {bdaddr}");
    println!("note: audio devices appear under /dev/bluetooth/ (virtual_oss)");
    Ok(())
}

pub fn disconnect(bdaddr: &str) -> Result<()> {
    let Some(dev) = controller() else {
        bail!("no /dev/ubtN");
    };
    util::run("hccontrol", &["-n", &hci(&dev), "disconnect", bdaddr])
}
