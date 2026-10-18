# Vellum on Apple hardware

Three tiers, honestly graded:

## Apple Silicon (M1–M4) — VM only

FreeBSD/aarch64 **does not boot natively** on Apple Silicon yet (upstream
bring-up is in progress; track the FreeBSD Foundation status reports).
Today, Apple Silicon means virtualization:

- **Parallels** (our dev rig): FreeBSD 15.1 aarch64. No DRM/KMS — X11
  session via `scfb` works; Wayland does not. This is exactly why the X11
  path exists.
- **UTM/QEMU**: use `-device virtio-gpu-pci` + `kldload virtio_gpu` for
  real KMS in the guest; Hyprland becomes testable there.
- **VMware Fusion**: `vmwgfx` KMS via drm-kmod.

aarch64 memstick images are supported by `image/build-image.sh`
(`ARCH=aarch64`) and the CI workflow builds amd64; aarch64 ISO builds can
be added to CI on arm runners when needed.

## Intel Macs (pre-2018) — decent target

amd64 FreeBSD installs via EFI like any PC.

- **GPU**: Intel iGPU → `i915kms`; AMD dGPU (15" MBP, iMac) → `amdgpu`;
  NVIDIA-era Macs → `nvidia-driver` (old models only).
- **Trackpad**: `kldload wsp` (multitouch) or `atp` for older models —
  add to `kld_list` post-install.
- **Keyboard**: Option=Alt, Command=Super — the Omarchy keymap lands on
  Command naturally. Brightness/media keys mostly map to XF86 keysyms.
- **Wifi**: Broadcom — mostly **unsupported** (bwi/bwn cover only ancient
  chips). Plan on a USB wifi dongle, Ethernet, or phone tethering for
  install + daily use.
- **Suspend**: S3 exists on these machines; quality varies by model.

## T2 Macs (2018–2020) — problematic

The T2 security chip sits in front of storage, keyboard, and trackpad.
FreeBSD generally **cannot see the NVMe storage** on these (no T2 support
upstream — even Linux needed out-of-tree patches). `vellum doctor` flags
T2 models. Treat as unsupported until upstream support lands.

## Detection

`vellum doctor` reports `hw.model`; Apple hardware gets the caveat banner
above. Install works the same — wizard, ZFS, power profiles — minus the
hardware gaps noted.
