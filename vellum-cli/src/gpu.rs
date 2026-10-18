//! GPU detection and driver wiring for FreeBSD 15.x.
//!
//! Every display adapter pciconf reports is tracked individually (an AMD
//! APU + AMD dGPU laptop is still hybrid). Driver map, verified against
//! the ports tree:
//!   Intel  -> i915kms     (graphics/drm-kmod → drm-612-kmod on 15.1, amd64
//!                          only; no xe.ko, so Lunar Lake/Battlemage fall
//!                          back to scfb)
//!   AMD    -> amdgpu      (GCN 1.2+ / RDNA), radeonkms for SI/CIK and
//!                          TeraScale — amdgpu doesn't claim those
//!   NVIDIA -> proprietary, branch by generation (nouveau doesn't exist):
//!             Turing+ nvidia-driver (595), Maxwell–Volta -580,
//!             Kepler -470, Fermi -390, Tesla -340. 595/580 also get
//!             nvidia-drm (+ hw.nvidiadrm.modeset=1) for PRIME.
//!   VMs    -> no KMS driver in drm-kmod; Xorg uses scfb.
//!
//! Hybrid policy: the firmware's boot display (hw.pci.default_vgapci_unit)
//! drives the session — the iGPU on laptops, whichever card has the monitor
//! on desktops. Other GPUs are left for render offload via prime-run.

use std::fs;

use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Intel,
    Amd,
    Nvidia,
    Virtual,
    Other,
}

/// NVIDIA driver branch needed for a given GPU generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvBranch {
    Current, // Turing, Ampere, Ada, Blackwell, Hopper — nvidia-driver (595)
    L580,    // Maxwell, Pascal, Volta
    L470,    // Kepler
    L390,    // Fermi
    L340,    // Tesla
}

impl NvBranch {
    fn packages(self) -> &'static [&'static str] {
        match self {
            NvBranch::Current => &["nvidia-driver", "nvidia-drm-kmod"],
            NvBranch::L580 => &["nvidia-driver-580", "nvidia-drm-kmod-580"],
            NvBranch::L470 => &["nvidia-driver-470"],
            NvBranch::L390 => &["nvidia-driver-390"],
            NvBranch::L340 => &["nvidia-driver-340"],
        }
    }
    /// nvidia-drm ships only for the 595/580 branches.
    pub fn has_drm(self) -> bool {
        matches!(self, NvBranch::Current | NvBranch::L580)
    }
    /// PRIME render offload needs 435+.
    pub fn can_offload(self) -> bool {
        !matches!(self, NvBranch::L390 | NvBranch::L340)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gpu {
    pub vendor: Vendor,
    pub vendor_id: u16,
    pub device_id: u16,
    /// newbus name, e.g. "vgapci0" (or "none3" when unattached)
    pub dev: String,
    pub domain: u32,
    pub bus: u32,
    pub slot: u32,
    pub func: u32,
    /// pciconf's device string, e.g. "GP108M [GeForce MX150]"
    pub name: String,
}

/// SI/CIK/TeraScale codenames — radeonkms territory, amdgpu won't attach.
const RADEON_CODENAMES: &[&str] = &[
    "Tahiti", "Pitcairn", "Cape Verde", "Oland", "Hainan", "Bonaire", "Hawaii",
    "Kaveri", "Kabini", "Mullins", "Spectre", "Spooky", "Kalindi", "Godavari",
    "Cedar", "Redwood", "Juniper", "Cypress", "Hemlock", "Palm", "Sumo",
    "Caicos", "Turks", "Barts", "Cayman", "Aruba", "Trinity", "Richland",
    "Wrestler", "RV6", "RV7", "RS7", "RS8", "R600", "R700",
];

impl Gpu {
    pub fn unit(&self) -> Option<u32> {
        self.dev.strip_prefix("vgapci")?.parse().ok()
    }

    pub fn nv_branch(&self) -> Option<NvBranch> {
        if self.vendor != Vendor::Nvidia {
            return None;
        }
        // pciids names lead with the chip codename: GA104, GP108M, GK208B…
        let chip = self.name.split_whitespace().next().unwrap_or("");
        let has = |p: &str| {
            chip.strip_prefix(p).is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        };
        let any = |ps: &[&str]| ps.iter().any(|p| has(p));
        Some(if any(&["TU", "GA", "AD", "GB", "GH"]) {
            NvBranch::Current
        } else if any(&["GM", "GP", "GV"]) {
            NvBranch::L580
        } else if has("GK") {
            NvBranch::L470
        } else if has("GF") {
            NvBranch::L390
        } else if any(&["G", "GT", "MCP", "C"]) {
            NvBranch::L340 // G80/G92, GT216, MCP79, C79 (Tesla era)
        } else {
            NvBranch::Current // unknown / newer than the installed pciids
        })
    }

    /// Kernel module that drives this GPU (None: VM / unsupported → scfb).
    pub fn kmods(&self) -> Vec<&'static str> {
        match self.vendor {
            Vendor::Intel => vec!["i915kms"],
            Vendor::Amd if RADEON_CODENAMES.iter().any(|c| self.name.starts_with(c)) => {
                vec!["radeonkms"]
            }
            Vendor::Amd => vec!["amdgpu"],
            Vendor::Nvidia => match self.nv_branch() {
                // 340/390 predate the modeset driver; 470 has it but no drm.
                Some(NvBranch::L340 | NvBranch::L390) => vec!["nvidia"],
                Some(b) if b.has_drm() => vec!["nvidia-modeset", "nvidia-drm"],
                _ => vec!["nvidia-modeset"],
            },
            Vendor::Virtual | Vendor::Other => vec![],
        }
    }

    /// Xorg BusID: decimal PCI:bus:slot:func (bus@domain when domain ≠ 0).
    pub fn xorg_busid(&self) -> String {
        if self.domain == 0 {
            format!("PCI:{}:{}:{}", self.bus, self.slot, self.func)
        } else {
            format!("PCI:{}@{}:{}:{}", self.bus, self.domain, self.slot, self.func)
        }
    }

    /// Mesa's deterministic DRI_PRIME selector, e.g. pci-0000_01_00_0.
    pub fn dri_prime_tag(&self) -> String {
        format!("pci-{:04x}_{:02x}_{:02x}_{:x}", self.domain, self.bus, self.slot, self.func)
    }

    /// drm-kmod's hw.dri.N.busid format, e.g. pci:0000:01:00.0.
    fn drm_busid(&self) -> String {
        format!("pci:{:04x}:{:02x}:{:02x}.{:x}", self.domain, self.bus, self.slot, self.func)
    }

    pub fn label(&self) -> String {
        let driver = match (self.vendor, self.nv_branch()) {
            (Vendor::Nvidia, Some(b)) => b.packages()[0].to_string(),
            (Vendor::Virtual, _) => "virtual, scfb".into(),
            _ => self.kmods().first().map_or("scfb".into(), |k| k.to_string()),
        };
        let name = if self.name.is_empty() {
            format!("{:04x}:{:04x}", self.vendor_id, self.device_id)
        } else {
            self.name.clone()
        };
        format!("{name} [{driver}] @{}", self.xorg_busid())
    }
}

fn hex_field(line: &str, key: &str) -> Option<u32> {
    let v = line.split_whitespace().find_map(|t| t.strip_prefix(key))?;
    u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()
}

/// Parse `pciconf -lv` output into display adapters.
pub fn parse_pciconf(text: &str) -> Vec<Gpu> {
    let mut out: Vec<Gpu> = Vec::new();
    let mut in_display = false;
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) {
            in_display = false;
            // vgapci0@pci0:0:2:0:	class=0x030000 … vendor=0x8086 device=0x5917
            let Some((dev, rest)) = line.split_once('@') else { continue };
            let class = hex_field(line, "class=").unwrap_or(0);
            if class >> 16 != 0x03 {
                continue; // not a display controller
            }
            let sel = rest.split_whitespace().next().unwrap_or("").trim_end_matches(':');
            let nums: Vec<u32> = sel
                .trim_start_matches("pci")
                .split(':')
                .filter_map(|n| n.parse().ok())
                .collect();
            let [domain, bus, slot, func] = nums[..] else { continue };
            let vendor_id = hex_field(line, "vendor=").unwrap_or(0) as u16;
            let vendor = match vendor_id {
                0x8086 => Vendor::Intel,
                0x1002 => Vendor::Amd,
                0x10de => Vendor::Nvidia,
                // Parallels, VMware, VirtualBox, QEMU/virtio, Bochs, Hyper-V
                0x1ab8 | 0x15ad | 0x80ee | 0x1af4 | 0x1234 | 0x1414 => Vendor::Virtual,
                _ => Vendor::Other,
            };
            out.push(Gpu {
                vendor,
                vendor_id,
                device_id: hex_field(line, "device=").unwrap_or(0) as u16,
                dev: dev.to_string(),
                domain,
                bus,
                slot,
                func,
                name: String::new(),
            });
            in_display = true;
        } else if in_display {
            let l = line.trim();
            if let Some(v) = l.strip_prefix("device").map(str::trim_start) {
                if let Some(v) = v.strip_prefix('=') {
                    if let Some(g) = out.last_mut() {
                        g.name = v.trim().trim_matches('\'').to_string();
                    }
                }
            }
        }
    }
    out
}

/// All display adapters, in probe order.
pub fn detect() -> Vec<Gpu> {
    parse_pciconf(&util::capture("pciconf", &["-lv"]).unwrap_or_default())
}

fn is_amd64() -> bool {
    util::capture("uname", &["-m"]).is_some_and(|m| m.trim() == "amd64")
}

/// Real GPUs with a FreeBSD KMS/proprietary driver (VMs excluded).
fn drivable(gpus: &[Gpu]) -> Vec<&Gpu> {
    gpus.iter().filter(|g| !g.kmods().is_empty()).collect()
}

/// The GPU that drives the session: the firmware's boot display, else an
/// integrated part (bus 0), else the first adapter.
pub fn primary(gpus: &[Gpu]) -> Option<&Gpu> {
    let boot = util::capture("sysctl", &["-n", "hw.pci.default_vgapci_unit"])
        .and_then(|u| u.trim().parse::<i64>().ok())
        .filter(|u| *u >= 0);
    boot.and_then(|u| gpus.iter().find(|g| g.unit().map(i64::from) == Some(u)))
        .or_else(|| gpus.iter().find(|g| g.bus == 0 && g.vendor != Vendor::Nvidia))
        .or_else(|| gpus.first())
}

/// GPUs available for render offload (everything drivable but the primary).
pub fn offload(gpus: &[Gpu]) -> Vec<&Gpu> {
    let p = primary(gpus);
    drivable(gpus).into_iter().filter(|g| Some(*g) != p).collect()
}

pub fn is_hybrid(gpus: &[Gpu]) -> bool {
    drivable(gpus).len() > 1
}

/// Value for rc.conf kld_list covering every detected GPU (amd64 only —
/// drm-kmod and the NVIDIA blobs don't exist for aarch64).
pub fn kld_list() -> String {
    if !is_amd64() {
        return String::new();
    }
    let mut mods: Vec<&str> = Vec::new();
    for g in detect() {
        for m in g.kmods() {
            if !mods.contains(&m) {
                mods.push(m);
            }
        }
    }
    mods.join(" ")
}

/// GPU-specific packages beyond the manifests: the right NVIDIA branch
/// (drm-kmod for Intel/AMD is in base.txt).
pub fn packages() -> Vec<&'static str> {
    if !is_amd64() {
        return Vec::new();
    }
    let mut pkgs: Vec<&str> = Vec::new();
    for b in detect().iter().filter_map(Gpu::nv_branch) {
        for p in b.packages() {
            if !pkgs.contains(p) {
                pkgs.push(p);
            }
        }
    }
    pkgs
}

/// loader.conf lines the detected hardware needs.
pub fn loader_conf_lines() -> Vec<&'static str> {
    let nv_drm = detect().iter().filter_map(Gpu::nv_branch).any(NvBranch::has_drm);
    if nv_drm && is_amd64() {
        vec!["hw.nvidiadrm.modeset=\"1\""]
    } else {
        Vec::new()
    }
}

fn xorg_driver(g: &Gpu) -> &'static str {
    if g.vendor == Vendor::Nvidia { "nvidia" } else { "modesetting" }
}

/// Render the xorg.conf.d pin for a GPU set (pure, for tests).
pub fn xorg_conf(gpus: &[Gpu]) -> Option<String> {
    let p = primary(gpus).filter(|g| !g.kmods().is_empty())?;
    let nv_offload = offload(gpus)
        .into_iter()
        .find(|g| g.nv_branch().is_some_and(NvBranch::can_offload));
    let kind = if is_hybrid(gpus) {
        "hybrid graphics — boot display drives the session"
    } else {
        "single GPU"
    };
    let mut s = format!("# Vellum GPU pin ({kind}) — written by `vellum install`\n");
    s += "Section \"ServerLayout\"\n    Identifier \"vellum\"\n    Screen 0 \"vellum-primary\"\n";
    if nv_offload.is_some() {
        // lets the NVIDIA dGPU register as a PRIME render-offload GPU screen
        s += "    Option \"AllowNVIDIAGPUScreens\"\n";
    }
    s += "EndSection\n\n";
    s += &format!(
        "Section \"Device\"\n    Identifier \"vellum-primary\"\n    Driver \"{}\"\n    BusID \"{}\"\nEndSection\n\n",
        xorg_driver(p),
        p.xorg_busid()
    );
    s += "Section \"Screen\"\n    Identifier \"vellum-primary\"\n    Device \"vellum-primary\"\nEndSection\n";
    if let Some(nv) = nv_offload {
        s += &format!(
            "\nSection \"Device\"\n    Identifier \"vellum-offload\"\n    Driver \"nvidia\"\n    BusID \"{}\"\nEndSection\n",
            nv.xorg_busid()
        );
    }
    Some(s)
}

/// /usr/local/etc/X11/xorg.conf.d/20-vellum.conf — pin the DDX + BusID so
/// Xorg never picks a headless dGPU. No KMS-capable GPU → no pin (Xorg
/// autoconfig then picks scfb; pinning modesetting without KMS kills X).
pub fn write_xorg_conf(root: &str) {
    let dir = format!("{root}/usr/local/etc/X11/xorg.conf.d");
    let path = format!("{dir}/20-vellum.conf");
    let gpus = if is_amd64() { detect() } else { Vec::new() };
    match xorg_conf(&gpus) {
        Some(conf) if fs::create_dir_all(&dir).is_ok() => {
            let _ = fs::write(path, conf);
        }
        _ => {
            let _ = fs::remove_file(&path);
        }
    }
    write_offload_env(root, &gpus);
}

/// /usr/local/etc/vellum/gpu.env — read by prime-run so it only sets the
/// variables for the offload GPU that actually exists.
fn write_offload_env(root: &str, gpus: &[Gpu]) {
    let dir = format!("{root}/usr/local/etc/vellum");
    let _ = fs::create_dir_all(&dir);
    let target = offload(gpus).into_iter().next();
    let body = match target {
        Some(g) if g.nv_branch().is_some_and(NvBranch::can_offload) => "VELLUM_OFFLOAD=nvidia\n".to_string(),
        Some(g) if g.vendor != Vendor::Nvidia => {
            format!("VELLUM_OFFLOAD=dri\nVELLUM_DRI_PRIME={}\n", g.dri_prime_tag())
        }
        _ => "VELLUM_OFFLOAD=none\n".to_string(),
    };
    let _ = fs::write(format!("{dir}/gpu.env"), body);
}

/// /usr/local/etc/X11/xorg.conf.d/00-keyboard.conf — carry the console
/// keymap chosen in the installer into X11 (setxkbmap layout/variant).
pub fn write_xorg_keyboard(root: &str, layout: &str, variant: &str) {
    let dir = format!("{root}/usr/local/etc/X11/xorg.conf.d");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let variant = if variant.is_empty() {
        String::new()
    } else {
        format!("    Option \"XkbVariant\" \"{variant}\"\n")
    };
    let _ = fs::write(
        format!("{dir}/00-keyboard.conf"),
        format!(
            "Section \"InputClass\"\n    Identifier \"vellum-keyboard\"\n    \
             MatchIsKeyboard \"on\"\n    Option \"XkbLayout\" \"{layout}\"\n{variant}EndSection\n"
        ),
    );
}

/// DRM driver currently bound to this GPU (hw.dri.N.busid ↔ name), if any.
pub fn attached_driver(g: &Gpu) -> Option<String> {
    let want = g.drm_busid();
    (0..8).find_map(|n| {
        let busid = util::capture("sysctl", &["-n", &format!("hw.dri.{n}.busid")])?;
        (busid.trim() == want)
            .then(|| util::capture("sysctl", &["-n", &format!("hw.dri.{n}.name")]))
            .flatten()
            .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
    })
}

/// Runtime check after drm-kmod is installed: an AMD card the codename
/// table missed won't be claimed by amdgpu — retry with radeonkms and fix
/// kld_list. Returns the modules that ended up loaded.
pub fn probe_amd_fallback() -> Vec<String> {
    let mut fixed = Vec::new();
    for g in detect().iter().filter(|g| g.vendor == Vendor::Amd) {
        let want = g.kmods()[0];
        let _ = util::capture("kldload", &["-n", want]);
        if attached_driver(g).is_some() {
            fixed.push(want.to_string());
            continue;
        }
        if want == "amdgpu" {
            let _ = util::capture("kldload", &["-n", "radeonkms"]);
            if attached_driver(g).is_some() {
                let _ = util::capture("sysrc", &["kld_list-=amdgpu"]);
                let _ = util::capture("sysrc", &["kld_list+=radeonkms"]);
                fixed.push("radeonkms".into());
            }
        }
    }
    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    const HYBRID: &str = "\
hostb0@pci0:0:0:0:\tclass=0x060000 rev=0x08 hdr=0x00 vendor=0x8086 device=0x5914
    vendor     = 'Intel Corporation'
vgapci0@pci0:0:2:0:\tclass=0x030000 rev=0x07 hdr=0x00 vendor=0x8086 device=0x5917
    vendor     = 'Intel Corporation'
    device     = 'UHD Graphics 620'
    class      = display
    subclass   = VGA
vgapci1@pci0:1:0:0:\tclass=0x030200 rev=0xa1 hdr=0x00 vendor=0x10de device=0x1d10
    vendor     = 'NVIDIA Corporation'
    device     = 'GP108M [GeForce MX150]'
    class      = display
    subclass   = 3D
";

    fn gpu(vendor: Vendor, name: &str, bus: u32) -> Gpu {
        Gpu {
            vendor,
            vendor_id: 0,
            device_id: 0,
            dev: format!("vgapci{bus}"),
            domain: 0,
            bus,
            slot: 0,
            func: 0,
            name: name.into(),
        }
    }

    #[test]
    fn parses_hybrid_laptop() {
        let g = parse_pciconf(HYBRID);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].vendor, Vendor::Intel);
        assert_eq!(g[0].xorg_busid(), "PCI:0:2:0");
        assert_eq!(g[1].name, "GP108M [GeForce MX150]");
        assert_eq!(g[1].dri_prime_tag(), "pci-0000_01_00_0");
        assert_eq!(g[1].drm_busid(), "pci:0000:01:00.0");
        assert!(is_hybrid(&g));
    }

    #[test]
    fn nvidia_branches() {
        let b = |n| gpu(Vendor::Nvidia, n, 1).nv_branch().unwrap();
        assert_eq!(b("AD107M [GeForce RTX 4060 Max-Q]"), NvBranch::Current);
        assert_eq!(b("GA104 [GeForce RTX 3070]"), NvBranch::Current);
        assert_eq!(b("TU117M [GeForce GTX 1650 Mobile]"), NvBranch::Current);
        assert_eq!(b("GB206 [GeForce RTX 5060]"), NvBranch::Current);
        assert_eq!(b("GP108M [GeForce MX150]"), NvBranch::L580);
        assert_eq!(b("GM107 [GeForce GTX 750 Ti]"), NvBranch::L580);
        assert_eq!(b("GK208B [GeForce GT 710]"), NvBranch::L470);
        assert_eq!(b("GF108 [GeForce GT 430]"), NvBranch::L390);
        assert_eq!(b("G92 [GeForce 9800 GT]"), NvBranch::L340);
        assert_eq!(b("GT216 [GeForce GT 220]"), NvBranch::L340);
        assert_eq!(b(""), NvBranch::Current);
    }

    #[test]
    fn amd_driver_choice() {
        assert_eq!(gpu(Vendor::Amd, "Ellesmere [Radeon RX 470/480]", 1).kmods(), ["amdgpu"]);
        assert_eq!(gpu(Vendor::Amd, "Pitcairn PRO [Radeon HD 7850]", 1).kmods(), ["radeonkms"]);
        assert_eq!(gpu(Vendor::Amd, "Kaveri [Radeon R7 Graphics]", 0).kmods(), ["radeonkms"]);
        assert_eq!(gpu(Vendor::Amd, "Rembrandt [Radeon 680M]", 0).kmods(), ["amdgpu"]);
    }

    #[test]
    fn xorg_conf_hybrid_nvidia_offload() {
        let gpus = vec![gpu(Vendor::Intel, "UHD 620", 0), gpu(Vendor::Nvidia, "GA107M", 1)];
        let conf = xorg_conf(&gpus).unwrap();
        assert!(conf.contains("Driver \"modesetting\"\n    BusID \"PCI:0:0:0\""));
        assert!(conf.contains("AllowNVIDIAGPUScreens"));
        assert!(conf.contains("Identifier \"vellum-offload\"\n    Driver \"nvidia\""));
    }

    #[test]
    fn xorg_conf_vm_has_no_pin() {
        assert!(xorg_conf(&[gpu(Vendor::Virtual, "", 0)]).is_none());
        assert!(xorg_conf(&[]).is_none());
    }

    #[test]
    fn legacy_nvidia_cannot_offload() {
        let gpus = vec![gpu(Vendor::Intel, "HD 4000", 0), gpu(Vendor::Nvidia, "GF108M", 1)];
        assert!(!xorg_conf(&gpus).unwrap().contains("AllowNVIDIAGPUScreens"));
    }
}
