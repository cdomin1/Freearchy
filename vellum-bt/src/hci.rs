//! FreeBSD raw HCI socket plumbing.
//!
//! FreeBSD exposes Bluetooth controllers through a raw HCI socket:
//!     socket(AF_BLUETOOTH, SOCK_RAW, BLUETOOTH_PROTO_HCI)
//! defined in <netgraph/bluetooth/include/ng_hci.h>. Command packets are
//! written directly; events read back in the same socket. This module
//! owns that framing so adapter.rs/device.rs stay D-Bus-shaped.

// Scaffold: nothing drives the socket yet (adapter.rs still stubs HCI).
#![allow(dead_code)]

use anyhow::Result;
use std::os::unix::io::RawFd;

// FreeBSD socket constants, verified against releng/15.0 headers:
// sys/socket.h, netgraph/bluetooth/include/ng_btsocket.h, ng_hci.h.
const AF_BLUETOOTH: libc::c_int = 36;          // 32 is AF_NETGRAPH
const SOCK_RAW: libc::c_int = 3;
const BLUETOOTH_PROTO_HCI: libc::c_int = 134;
const NG_HCI_CMD_PKT: u8 = 0x01;               // ng_hci_cmd_pkt_t.type

// OGF/OCF for the commands we need.
pub const OGF_LINK_CONTROL: u16 = 0x01;
pub const OCF_INQUIRY: u16 = 0x0001;
pub const OCF_INQUIRY_CANCEL: u16 = 0x0002;
pub const OCF_REMOTE_NAME_REQ: u16 = 0x0019;
pub const OGF_HOST_CTL: u16 = 0x03;
pub const OCF_RESET: u16 = 0x0003;
pub const OCF_WRITE_SCAN_ENABLE: u16 = 0x001a;
pub const OGF_LE_CTL: u16 = 0x08;
pub const OCF_LE_SET_SCAN_ENABLE: u16 = 0x000c;

/// Discover adapters — ubt* device nodes under /dev.
/// TODO: also probe ng_ubt state (devd attaches late; firmware-loaded
/// Intel parts appear after iwmbtfw runs).
pub fn adapters() -> Vec<String> {
    match std::fs::read_dir("/dev") {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with("ubt"))
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub struct HciSocket {
    fd: RawFd,
    pub node: String,
}

impl HciSocket {
    /// Open the HCI socket bound to a ubt node.
    /// TODO: fill sockaddr_hci (struct sockaddr_hci { hci_len, hci_family,
    /// hci_node[NG_NODESIZ] }) — see ng_btsocket.h on the target.
    pub fn open(node: &str) -> Result<Self> {
        let fd = unsafe { libc::socket(AF_BLUETOOTH, SOCK_RAW, BLUETOOTH_PROTO_HCI) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // TODO: bind(fd, sockaddr_hci{node}) — without bind the socket
        // receives events from all HCI nodes.
        Ok(Self { fd, node: node.to_string() })
    }

    /// Send an HCI command packet: type(0x01) + opcode(2 LE) + plen(1) + params.
    pub fn command(&self, ogf: u16, ocf: u16, params: &[u8]) -> Result<()> {
        let opcode = (ogf << 10) | ocf;
        let mut pkt = Vec::with_capacity(4 + params.len());
        pkt.push(NG_HCI_CMD_PKT);
        pkt.extend_from_slice(&opcode.to_le_bytes());
        pkt.push(params.len() as u8);
        pkt.extend_from_slice(params);
        let n = unsafe { libc::write(self.fd, pkt.as_ptr() as _, pkt.len()) };
        if n < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    // TODO: event reader — recv() and parse HCI event header
    // (code:1, plen:1, params) + demux per-pending-command.
}

impl Drop for HciSocket {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}
