# vellum roadmap

Direction: **X11-first** desktop that emulates the Omarchy experience on
FreeBSD. Parallels/other VMs expose no usable DRM/KMS for Wayland, so the
primary tested path is `vellum-wm` (penrose) + animated picom + polybar.
Hyprland + Quickshell/Caelestia is kept as a shelved Wayland session for
real hardware with working KMS.

## Phase 1 — skeleton (done)
- [x] Repo layout, package manifests, theme system
- [x] `vellum` CLI: install / install-system / theme / power / doctor / image
- [x] `boot.sh` curl-install bootstrap
- [x] Image builder (memstick remaster) + `vellum install-system` wizard
- [x] Rename Freearchy → VELLUM (crates, paths, env vars, branding, GitHub repo)

## Phase 2 — X11 desktop (done)
- [x] `vellum-wm`: penrose/x11rb tiling WM, Omarchy keymap, EWMH, gaps,
      theme-driven border colors
- [x] picom 13 from packages — native open/close/tiling-move animations
      (replaced the pijulius fork build)
- [x] polybar Tokyo Night panel (EWMH workspaces, FreeBSD-native net/vol/bat
      via ifconfig/mixer/acpiconf — no Linux `/proc`/`sys` modules)
- [x] rofi launcher, alacritty, feh wallpaper, xfce4-notifyd, slock, maim
- [x] thunar + Materia-dark GTK3 theme + Papirus-Dark icons (GTK2/3 configs)
- [x] 22 Omarchy theme packs ported (alacritty/rofi/polybar/wm/wallpaper
      fragments under `themes/<name>/`, `vellum theme` switcher)

## Phase 3 — power & hardware (done)
- [x] Suspend: S3/s2idle detection, rc.suspend/rc.resume hooks, lid/button
      sysctls, `doas zzz` keybind
- [x] AC/battery profile switching via devd + `vellum-power`
      (performance/balanced/power; EPP hints, wifi powersave)
- [x] powerdxx adaptive CPU freq; X DPMS idle blanking
- [x] ThinkPad charge thresholds via acpi_call (`vellum power battery`)
- [x] GPU detection → kld_list: i915kms / amdgpu / nvidia(-modeset);
      vmwgfx fallback in doctor
- [x] Hybrid graphics: iGPU drives session (modesetting pin), `prime-run`
      for on-demand offload
- [x] Intel Mac detection: wsp/atp trackpads, webcamd, T2/Broadcom caveats

## Phase 4 — boots in a VM (in progress)
- [ ] Working memstick image from CI (amd64 + aarch64 artifacts)
- [ ] Boot image in Parallels aarch64 → run TUI installer → `startx`
- [ ] Verify `vellum-wm` compiles on FreeBSD (first real build — penrose
      API fix pass expected)
- [ ] Validate every pkg name in `packages/*.txt` (`pkg search`), trim `?`
- [ ] `vellum doctor` green in-VM; theme switching end-to-end
- [ ] Live-media auto-run of `vellum install-system` instead of bsdinstall
- [ ] Wi-Fi provisioning in the installer (Ethernet-only today)

## Phase 5 — real hardware
- [ ] ThinkPad: suspend/resume matrix (S3 vs s2idle), wifi resume, DPMS,
      charge thresholds
- [ ] Hybrid laptop: prime-run verification, external-output provider wiring
      (`xrandr --setprovideroutputsource`)
- [ ] Intel Mac (pre-2018): trackpad/webcam/graphics validation; document
      Broadcom wifi workarounds (USB NIC)
- [ ] Keybind help sheet (`vellum help`?)

## Phase 6 — shelved / later
- [ ] Wayland shelf revive: Hyprland + Caelestia on KMS-capable hardware
      (seatd/turnstile session, qt6-m3shapes/libcava — see shell/PORTING.md)
- [ ] vellum-bt: BlueZ-compatible D-Bus daemon over FreeBSD HCI/netgraph
- [ ] Agentic AI re-activation (`packages/ai.txt` kept; aichat/claude-code/
      fabric; possibly a rofi mode)
- [ ] Per-theme GTK accents (build adw-gtk3/Colloid or generate Materia
      variants)
- [ ] Track Omarchy releases for app/config inspiration
