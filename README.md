![vellum](branding/vellum-logo.svg)

An Omarchy-inspired, opinionated desktop for **FreeBSD**.

Turn a fresh FreeBSD 15.x install into a keyboard-driven tiling desktop —
our own Rust X11 window manager, animated compositor, themed panel and
launcher — managed by a single `vellum` binary. Wayland/Hyprland + Caelestia
are installed but **shelved** (no KMS on common virtualizers); the X11 path
is the one that ships.

## Stack

| Layer | Choice | Why |
|---|---|---|
| Base OS | FreeBSD 15.x RELEASE | ZFS root + `bectl` boot environments = the Omarchy snapshot story, natively |
| Window manager | `vellum-wm` — Rust on penrose/x11rb | Xorg needs no seatd/KMS handshake; Omarchy keymap, tiling + workspaces + EWMH |
| Compositor | picom 13 (`x11-wm/picom`) | Native open/close + tiling-move animations, rounded corners, shadows — the Hyprland feel on X11, straight from packages |
| Panel | polybar | Tokyo Night bar: EWMH workspaces, window title, FreeBSD-native wifi/vol/bat script modules |
| Launcher | rofi | `Super+Space`, themed from the active palette |
| Terminal | alacritty | `Super+Return`, themed |
| Notifications | xfce4-notifyd | dunst isn't ported |
| Lock / shots | slock · xautolock · maim | lock on suspend/idle · `Print` variants |
| Menus | `vellum-menu` (rofi) | system/theme/power/capture/network/bluetooth/keys |
| Tooling | `vellum` CLI (Rust) | install / update / theme / power / doctor / image / install-system / bt / wifi |
| Wayland shelf | Hyprland + Caelestia (Quickshell) | Installed, still launchable; X11 is the supported session |

## Themes

All 22 Omarchy palettes ported (`themes/`): tokyo-night, catppuccin,
gruvbox, everforest, kanagawa, nord, rose-pine, osaka-jade, matte-black,
vantablack, ristretto, ethereal, hackerman, miasma, lumon, lupine,
last-horizon, retro-82, solitude, flexoki-light, catppuccin-latte, white.

`vellum theme set <name>` | `vellum theme next` — restyles alacritty, rofi,
polybar, wm border colors (next WM start) and wallpaper through the
`~/.config/vellum/current/theme` symlink; bar + wallpaper live-reload.

## Keybindings (Omarchy muscle memory)

| Keys | Action |
|---|---|
| `Super`+Return / `Super`+Space | alacritty · rofi launcher |
| `Super`+Alt+Space · `Super`+/ | vellum-menu · keybinding sheet |
| `Super`+B / `Super`+Shift+B | firefox · private window |
| `Super`+Shift+F / `Super`+Y | thunar · yazi |
| `Super`+N / `Super`+T / `Super`+G | nvim · btop · lazygit |
| `Super`+A / `Super`+C | aichat · rofi calculator |
| `Super`+j/k/↓/↑ | focus · `Super`+Shift swap · `Super`+h/l main-count |
| `Super`+1-9 / `Super`+Shift+1-9 | workspace / move to workspace |
| `Super`+Ctrl+←/→ · `Super`+Tab | prev/next workspace · last workspace |
| `Super`+= / `Super`+- | resize main area |
| `Super`+F / `Super`+V | fullscreen · float (`Super`+Shift+V sink) |
| `Super`+S | scratchpad terminal |
| `Super`+Q · `Super`+W | close window |
| `Super`+Esc / `Super`+Shift+Esc | system menu · suspend (`zzz`) |
| `Ctrl`+`Alt`+L / `Super`+Shift+Q | lock · log out |
| `Print` / `Shift`+Print / `Ctrl`+Print | shot region · window · full |
| `Super`+Print | colour picker → clipboard |
| `XF86` media keys | `mixer` volume, `backlight`, playerctl |

## Power & sleep

- **Suspend**: auto-detects `S3` vs `s2idle` (the S0ix path for modern
  laptops); `hw.acpi.suspend_state`/`lid_switch_state`/`sleep_button_state`
  written into sysctl.conf (lid → laptop suspend, sleep button → desktop).
  `rc.suspend` locks the *session owner's* X session via slock before
  sleeping; `rc.resume` bounces netif/wpa, BT controllers, EPP profile
  and ntpd. `xautolock` gives the idle-lock (xss-lock needs logind).
- **Profiles**: `vellum power set performance|balanced|power` — HWP EPP
  hints + wifi powersave. A `devd` hook auto-switches balanced/power on
  AC plug/unplug (the power-profiles-daemon equivalent). `powerdxx`
  handles adaptive CPU frequency.
- **ThinkPad**: `vellum power battery --start 75 --stop 80` — charge
  thresholds via `acpi_call`.
- X session: screensaver 5m, DPMS standby/off 10/15/20m.

## GPU support

Per-device pciconf detection at install: Intel→`i915kms`, AMD→`amdgpu`
(SI/CIK → `radeonkms` via a codename table + a runtime probe that retries
and fixes kld_list), NVIDIA→branch-matched proprietary driver
(595/580→`nvidia-modeset`+`nvidia-drm` with `hw.nvidiadrm.modeset=1`;
470→modeset; 390/340→`nvidia`). `kld_list` covers every adapter; the boot
VGA (`hw.pci.default_vgapci_unit`) gets the xorg.conf.d pin.
**Hybrid offload**: `prime-run <cmd>` reads the detected GPU's env
(DRI_PRIME pci tag or `__NV_PRIME_RENDER_OFFLOAD`).
**VMs/no-KMS**: no pin written — Xorg falls back to `scfb` framebuffer.
`vellum doctor` reports every adapter: driver bound, session/offload role.

## Bluetooth & Wi-Fi (experimental)

- `vellum bt on|status|scan|pair <bdaddr> [--pin 0000]|connect|disconnect` —
  netgraph/hccontrol front-end. Intel/Realtek firmware comes from
  `iwmbt-firmware`/`rtlbt-firmware`; audio devices surface via
  `virtual_oss` under `/dev/bluetooth/*`. BLE is limited on FreeBSD.
- `vellum wifi status|scan|connect <ssid> [pass]` — ifconfig/wpa_supplicant
  front-end. LinuxKPI wifi drivers are the usual FreeBSD caveat.
- Both are also reachable via `vellum-menu` → Network/Bluetooth.

## ArtCraft (experimental)

[ArtCraft](https://github.com/storytold/artcraft) — the pure-Rust egui
desktop app (`artcraft_native`) — builds from source on FreeBSD:

```sh
sh scripts/install-artcraft.sh
```

No Tauri/WebKit path — upstream ships no FreeBSD binaries, so expect
rough edges; it's a source build, not a bundled pkg.

## Quick start — installer image (pre-alpha)

Assets on the latest
[release](https://github.com/cdomin1/Vellum-BSD/releases) are named by
install target:

| File | Use |
|---|---|
| `vellum-15.1-amd64-usb-installer.img.xz` | x86-64 PC & Intel Macs — `dd` to USB |
| `vellum-15.1-amd64-vm-installer.iso.xz` | x86-64 VMs (VirtualBox/VMware/QEMU/bhyve) — attach as CD |
| `vellum-15.1-aarch64-usb-installer.img.xz` | arm64 (Apple Silicon) — `dd` to USB |
| `vellum-15.1-aarch64-vm-installer.iso.xz` | **Parallels/UTM on Apple Silicon** — attach as CD |

```sh
unxz vellum-*-installer.*
dd if=vellum-*-usb-installer.img of=/dev/daN bs=1m conv=sync   # USB only
```

Boot → quiet VELLUM splash → the wizard runs on the console: keymap → user
→ hostname → disk → erase-confirm → install → reboot. **Ethernet required**
(live env fetches base dists + payload over the network). The first boot of
the installed system finishes setup (packages, vellum-wm build, configs —
progress in `/var/log/vellum-firstboot.log`); then log in and `startx`.

## Quick start — on an existing FreeBSD install

Fresh FreeBSD has no sudo/doas, so run it as root from your own login:

```sh
su -
fetch https://raw.githubusercontent.com/cdomin1/Vellum-BSD/main/boot.sh -o - | sh
```

`boot.sh` installs git + rust, clones this repo to your
`~/.local/share/vellum`, builds the CLI, and runs `vellum install` for the
user who ran `su` (override with `VELLUM_USER=name`). Then `startx`.

## Layout

```
vellum-cli/   Rust CLI: install / update / theme / power / doctor / image / install-system
vellum-wm/    X11 tiling WM (penrose/x11rb) — the desktop core
vellum-bt/    BlueZ-compatible D-Bus daemon over FreeBSD HCI (scaffold)
packages/     pkg manifests (base, xorg, hyprland*, shell*, desktop, dev, ai†)
config/x11/   xinitrc, picom.conf, rofi, alacritty, polybar (+ scripts), gtk, prime-run
              bin/: vellum-menu (rofi system menus), vellum-shot (screenshots)
config/power/ rc.suspend/rc.resume, devd AC hook, profile script
config/hyprland/  Wayland session (shelved)
themes/       22 Omarchy palettes + wallpapers
image/        image + ISO builders (shared inject.sh), splash, first-boot service
              (CI: .github/workflows/iso.yml)
scripts/      gen-splash.py (boot splash from the logo), screenshot.sh (CI),
              install-artcraft.sh (experimental)
site/         Landing page (GitHub Pages)
shell/        Caelestia FreeBSD port notes (shelved)
docs/         roadmap + testing notes
```

\* hyprland/shell manifests install the shelved Wayland stack — kept, but
X11 is the tested path. † `ai.txt` is kept for a later phase and not
installed.

## Philosophy

Same contract as Omarchy: one command, a fully-working, opinionated desktop.
Where Omarchy leans on Arch packages and bash, vellum leans on `pkg`, Rust,
and FreeBSD-native facilities — devd, sysctl, netgraph — instead of
shimmed Linux subsystems.
