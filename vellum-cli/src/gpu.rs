//! GPU detection and driver wiring for FreeBSD.
//!
//! Vendor map (pciconf class 0x030000 VGA / 0x030200 3D):
//!   Intel  0x8086 -> i915kms   (drm-kmod, modesetting DDX)
//!   AMD    0x1002/0x1022 -> amdgpu (drm-kmod, modesetting DDX)
//!   NVIDIA 0x10de -> nvidia-modeset + nvidia (proprietary driver pkg;
//!                  nouveau doesn't exist on FreeBSD)
//!
//! Hybrid (Intel/AMD iGPU + NVIDIA dGPU, common on ThinkPads): run the
//! session on the iGPU via modesetting; nvidia modules still load so the
//! dGPU is usable, but X renders on the integrated part by default.

use std::fs;

use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gpu {
    Intel,
    Amd,
    Nvidia,
}

impl Gpu {
    pub fn label(self) -> &'static str {
        match self {
            Gpu::Intel => "intel (i915kms)",
            Gpu::Amd => "amd (amdgpu)",
            Gpu::Nvidia => "nvidia (proprietary)",
        }
    }
}

/// All display adapters seen by pciconf, in probe order.
pub fn detect() -> Vec<Gpu> {
    let pci = util::capture("pciconf", &["-lv"]).unwrap_or_default();
    let mut out = Vec::new();
    let mut vga = false;
    for line in pci.lines() {
        // VGA, 3D controller, and "display other" (some hybrid dGPUs)
        if ["class=0x030000", "class=0x030200", "class=0x038000"]
            .iter()
            .any(|c| line.contains(c))
        {
            vga = true;
        }
        if line.contains("vendor=") && vga {
            vga = false;
            let gpu = if line.contains("vendor=0x8086") {
                Gpu::Intel
            } else if line.contains("vendor=0x1002") || line.contains("vendor=0x1022") {
                Gpu::Amd
            } else if line.contains("vendor=0x10de") {
                Gpu::Nvidia
            } else {
                continue;
            };
            if !out.contains(&gpu) {
                out.push(gpu);
            }
        }
    }
    out
}

/// Value for rc.conf kld_list covering every detected GPU.
pub fn kld_list() -> String {
    detect()
        .iter()
        .flat_map(|g| match g {
            Gpu::Intel => vec!["i915kms"],
            Gpu::Amd => vec!["amdgpu"],
            Gpu::Nvidia => vec!["nvidia-modeset", "nvidia"],
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// GPU-specific packages beyond the manifests (proprietary NVIDIA is the
/// only conditional one — drm-kmod covers intel/amd and is in base.txt).
pub fn packages() -> Vec<&'static str> {
    let amd64 = util::capture("uname", &["-m"])
        .map(|m| m.trim() == "amd64")
        .unwrap_or(false);
    let mut pkgs = Vec::new();
    if amd64 && detect().contains(&Gpu::Nvidia) {
        pkgs.push("nvidia-driver");
    }
    pkgs
}

/// True when more than one known GPU is present (iGPU + dGPU laptop).
pub fn is_hybrid() -> bool {
    detect().len() > 1
}

/// X11 driver for the primary adapter: modesetting on KMS hardware,
/// "nvidia" only when NVIDIA is the sole GPU. None when no known GPU is
/// present (VMs, KMS-less boots) — Xorg autoconfig then picks scfb.
pub fn xorg_driver() -> Option<&'static str> {
    match detect().as_slice() {
        [] => None,
        [Gpu::Nvidia] => Some("nvidia"),
        _ => Some("modesetting"),
    }
}

/// /usr/local/etc/X11/xorg.conf.d/20-vellum.conf — pin the DDX driver
/// so Xorg doesn't guess wrong on hybrid hardware. The dGPU is left for
/// PRIME render offload (see /usr/local/bin/prime-run). Pinning
/// modesetting without KMS would kill X, so no GPU → no pin.
pub fn write_xorg_conf(root: &str) {
    let dir = format!("{root}/usr/local/etc/X11/xorg.conf.d");
    let path = format!("{dir}/20-vellum.conf");
    let Some(driver) = xorg_driver() else {
        let _ = fs::remove_file(&path);
        return;
    };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = fs::write(
        path,
        format!(
            "# Vellum GPU pin — {} detected\n\
             Section \"Device\"\n    Identifier \"vellum-gpu\"\n    Driver \"{driver}\"\nEndSection\n",
            if is_hybrid() { "hybrid graphics (iGPU drives the session)" } else { "single GPU" },
        ),
    );
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
