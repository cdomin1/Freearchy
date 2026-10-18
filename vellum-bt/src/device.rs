//! org.bluez.Device1 — one object per discovered/paired peer.

#[allow(dead_code)]
pub struct Device {
    pub address: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    pub rssi: Option<i16>,
    pub battery: Option<u8>,
}

#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    async fn connect(&self) -> zbus::fdo::Result<()> {
        // TODO: HCI_Create_Connection (ACL) — pairing state via link key
        Err(zbus::fdo::Error::NotSupported("not yet".into()))
    }

    async fn disconnect(&self) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported("not yet".into()))
    }

    async fn pair(&self) -> zbus::fdo::Result<()> {
        // TODO: PIN/link-key exchange — FreeBSD hcsecd handles part of this
        // via its own daemon; decide whether to drive it or speak HCI direct.
        Err(zbus::fdo::Error::NotSupported("not yet".into()))
    }

    async fn cancel_pairing(&self) -> zbus::fdo::Result<()> {
        Ok(())
    }

    #[zbus(property)]
    async fn address(&self) -> String {
        self.address.clone()
    }

    #[zbus(property)]
    async fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    async fn alias(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    async fn paired(&self) -> bool {
        self.paired
    }

    #[zbus(property)]
    async fn connected(&self) -> bool {
        self.connected
    }

    #[zbus(property)]
    async fn trusted(&self) -> bool {
        self.paired
    }

    #[zbus(property)]
    async fn set_trusted(&self, _v: bool) {}

    #[zbus(property)]
    async fn blocked(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn set_blocked(&self, _v: bool) {}

    #[zbus(property)]
    async fn battery_percentage(&self) -> u8 {
        self.battery.unwrap_or(0) // TODO: BAS over GATT for BLE devices
    }

    #[zbus(property)]
    async fn rssi(&self) -> i16 {
        self.rssi.unwrap_or(0)
    }
}
