//! org.bluez.Adapter1 — the object quickshell/panels query for adapter
//! state and drive discovery with.

use tokio::sync::Mutex;

pub struct Adapter {
    node: String,          // e.g. "ubt0"
    powered: Mutex<bool>,
    discovering: Mutex<bool>,
    alias: Mutex<String>,
    // TODO: device map — path → Device, populated by inquiry/LE scan events
}

impl Adapter {
    pub fn new(node: String) -> Self {
        Self {
            alias: Mutex::new(node.clone()),
            node,
            powered: Mutex::new(false),
            discovering: Mutex::new(false),
        }
    }
}

#[zbus::interface(name = "org.bluez.Adapter1")]
impl Adapter {
    async fn start_discovery(&self) -> zbus::fdo::Result<()> {
        // TODO: HCI_Inquiry (OGF_LINK_CONTROL/OCF_INQUIRY) +
        // LE scan for BLE devices; events populate the device map.
        *self.discovering.lock().await = true;
        Ok(())
    }

    async fn stop_discovery(&self) -> zbus::fdo::Result<()> {
        *self.discovering.lock().await = false;
        Ok(())
    }

    async fn set_discovery_filter(
        &self,
        _filter: std::collections::HashMap<String, zbus::zvariant::Value<'_>>,
    ) -> zbus::fdo::Result<()> {
        Ok(()) // accept-and-ignore for now; shells tolerate it
    }

    async fn remove_device(&self, _device: zbus::zvariant::ObjectPath<'_>) -> zbus::fdo::Result<()> {
        Ok(())
    }

    #[zbus(property)]
    async fn address(&self) -> String {
        // TODO: read BD_ADDR via ioctl(HCI_GET_NODE_INFO)
        "00:00:00:00:00:00".into()
    }

    #[zbus(property)]
    async fn name(&self) -> String {
        self.node.clone()
    }

    #[zbus(property)]
    async fn alias(&self) -> String {
        self.alias.lock().await.clone()
    }

    #[zbus(property)]
    async fn set_alias(&self, alias: String) {
        *self.alias.lock().await = alias;
    }

    #[zbus(property)]
    async fn powered(&self) -> bool {
        *self.powered.lock().await
    }

    #[zbus(property)]
    async fn set_powered(&self, on: bool) {
        // TODO: WRITE_SCAN_ENABLE / node up-down via ioctl
        *self.powered.lock().await = on;
    }

    #[zbus(property)]
    async fn discovering(&self) -> bool {
        *self.discovering.lock().await
    }

    #[zbus(property)]
    async fn discoverable(&self) -> bool {
        false // TODO: tie to scan-enable bits
    }

    #[zbus(property)]
    async fn pairable(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn uuids(&self) -> Vec<String> {
        vec![] // TODO: SDP service list — panels mostly don't read this
    }
}
