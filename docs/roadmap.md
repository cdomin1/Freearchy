# freearchy roadmap

## Phase 1 — installer skeleton (done)
- [x] Repo layout, package manifests, hyprland config, theme system
- [x] `freearchy` CLI: install / update / theme / doctor / image
- [x] `boot.sh` curl-install bootstrap
- [x] Image builder + bsdinstall `installerconfig` skeleton

## Phase 2 — boots on a real FreeBSD VM
- [ ] Validate every pkg name in `packages/*.txt` (`pkg search`) and trim `?` markers
- [ ] Verify seatd + turnstile give a working session (XDG_RUNTIME_DIR, dbus, portals)
- [ ] GPU matrix: i915kms / amdgpu / vmwgfx in a VM; document `sysrc kld_list` guidance
- [ ] End-to-end `freearchy install` on clean 15.1-RELEASE → Hyprland session

## Phase 3 — shell
- [x] quickshell on FreeBSD — **solved upstream**: `x11/quickshell` in ports since Jan 2026
- [ ] Verify packaged quickshell satisfies caelestia (it tracks -git; `QS_GIT=1` fallback exists in build-shell.sh)
- [ ] Build qt6-m3shapes + libcava; freearchy-shell fork with service patches (see shell/PORTING.md)
- [ ] First degraded milestone: shell runs, network/sensor widgets stubbed or hidden

## Phase 4 — polish + custom installer
- [x] `freearchy install-system` — own installer (gpart→ZFS→dists→loader→payload);
      dialoguer prompts today, ratatui UI is a drop-in later
- [ ] Custom live media that runs install-system instead of bsdinstall
- [ ] Theme coverage across all apps (foot, btop, fastfetch, nvim, shell)
- [ ] Wallpapers; default keybind sheet (`freearchy help`?)
- [ ] ISO build verified end-to-end in CI (QEMU)

## Phase 5 — upstream-friendly + bluetooth
- [ ] freearchy-bt: BlueZ-compatible D-Bus daemon over FreeBSD HCI sockets
      (org.bluez API subset for quickshell/panels; scaffolded, HCI plumbing TODO)
- [ ] Test with USB BT dongle passthrough; A2DP audio deferred to virtual_oss
- [ ] Push generic FreeBSD fixes to quickshell/caelestia where accepted
- [ ] Track Omarchy releases for app-selection/config inspiration
