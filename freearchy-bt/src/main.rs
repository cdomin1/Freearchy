//! freearchy-bt — BlueZ-compatible Bluetooth daemon for FreeBSD.
//!
//! Exposes the subset of org.bluez D-Bus API that desktop shells
//! (quickshell, panels, tray) consume, backed by FreeBSD's native HCI
//! socket layer instead of the Linux kernel stack BlueZ requires.
//!
//! Scope: adapter power/discovery, device list, pair/connect/disconnect,
//! battery. A2DP audio is explicitly out of scope for now.

mod adapter;
mod device;
mod hci;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("freearchy-bt starting");

    let adapters = hci::adapters();
    if adapters.is_empty() {
        eprintln!("warn: no HCI devices found (no ubt* nodes?) — bus will be empty");
    }

    let mut builder = zbus::connection::Builder::system()?
        .name("org.bluez")?;

    for (i, name) in adapters.iter().enumerate() {
        let path = format!("/org/bluez/hci{i}");
        let adapter = adapter::Adapter::new(name.clone());
        builder = builder.serve_at(path, adapter)?;
    }

    let _conn = builder.build().await?;
    eprintln!("org.bluez claimed; {} adapter(s) registered", adapters.len());

    // Park forever; zbus drives object dispatch on the connection's task.
    std::future::pending::<()>().await;
    Ok(())
}
