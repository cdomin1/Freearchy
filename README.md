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
| Compositor | pijulius/picom (`next`) | Spring-physics window animations — the Hyprland feel on X11. Built from source (`x11/build-picom.sh`) |
| Panel | polybar | Tokyo Night bar: EWMH workspaces, window title, FreeBSD-native wifi/vol/bat script modules |
| Launcher | rofi | `Super+Space`, themed from the active palette |
| Terminal | alacritty | `Super+Return`, themed |
| Notifications | xfce4-notifyd | dunst isn't ported |
| Lock / shots | slock · maim | `Super+Esc` / `Print` |
| Tooling | `vellum` CLI (Rust) | install / update / theme / power / doctor / image / install-system |
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
| `Super`+Return / `Super`+T | alacritty |
| `Super`+Space | rofi launcher |
| `Super`+B / `Super`+F | firefox / nemo |
| `Super`+j/k/↓/↑ | focus · `Super`+Shift swap |
| `Super`+1-9 / `Super`+Shift+1-9 | workspace / move to workspace |
| `Super`+= / `Super`+- | resize main area |
| `Super`+Q · `Super`+V · `Super`+Shift+F | close · float · fullscreen |
| `Super`+Esc / `Super`+Shift+Esc | lock (slock) / suspend (`zzz`) |
| `XF86` media keys | `mixer` volume, `backlight` brightness |
| `Print` | screenshot via maim |

## Power & sleep

- **Suspend**: auto-detects `S3` vs `s2idle` (the S0ix path for modern
  laptops); lid/sleep-button wired in sysctl.conf. `rc.suspend` grabs
  slock before sleeping, `rc.resume` bounces netif/wpa/ntpd.
- **Profiles**: `vellum power set performance|balanced|power` — HWP EPP
  hints + wifi powersave. A `devd` hook auto-switches balanced/power on
  AC plug/unplug (the power-profiles-daemon equivalent). `powerdxx`
  handles adaptive CPU frequency.
- **ThinkPad**: `vellum power battery --start 75 --stop 80` — charge
  thresholds via `acpi_call`.
- X session: screensaver 5m, DPMS standby/off 10/15/20m.

## GPU support

pciconf-based detection at install: Intel→`i915kms`, AMD→`amdgpu`,
NVIDIA→`nvidia-driver` (proprietary; amd64). `kld_list` covers everything
found; an xorg.conf.d pin keeps hybrid laptops on the iGPU.
**Hybrid offload**: `prime-run <cmd>` (DRI_PRIME + `__NV_PRIME_RENDER_OFFLOAD`).
`vellum doctor` reports detected GPUs vs. loaded kmods.

## Quick start — installer image (pre-alpha)

Download `vellum-15.1-amd64-memstick.img.xz` from the latest
[release](https://github.com/cdomin1/Vellum/releases), write to USB:

```sh
unxz vellum-*.img.xz
dd if=vellum-*.img of=/dev/daN bs=1m conv=sync   # or balenaEtcher
```

Boot → the Vellum wizard runs on the console: keymap → user → hostname →
disk → erase-confirm → install → reboot. **Ethernet required** (live env
fetches base dists + payload over the network).

## Quick start — on an existing FreeBSD install

```sh
fetch https://raw.githubusercontent.com/cdomin1/Vellum/main/boot.sh -o - | sh
```

`boot.sh` installs git + rust, clones this repo to `~/.local/share/vellum`,
builds the CLI, and runs `vellum install`. Then `startx`.

## Layout

```
vellum-cli/   Rust CLI: install / update / theme / power / doctor / image / install-system
vellum-wm/    X11 tiling WM (penrose/x11rb) — the desktop core
vellum-bt/    BlueZ-compatible D-Bus daemon over FreeBSD HCI (scaffold)
packages/     pkg manifests (base, xorg, hyprland*, shell*, desktop, dev, ai*)
config/x11/   xinitrc, picom.conf, rofi, alacritty, polybar, prime-run
config/power/ rc.suspend/rc.resume, devd AC hook, profile script
config/hyprland/  Wayland session (shelved)
themes/       22 Omarchy palettes + wallpapers
image/        Bootable memstick-image builder (CI: .github/workflows/iso.yml)
x11/          build-picom.sh (animated picom fork)
shell/        Caelestia FreeBSD port notes (shelved)
docs/         roadmap + testing notes
```

\* hyprland/shell/ai manifests install the shelved Wayland stack + optional
AI CLIs — kept, but X11 is the tested path.

## Philosophy

Same contract as Omarchy: one command, a fully-working, opinionated desktop.
Where Omarchy leans on Arch packages and bash, vellum leans on `pkg`, Rust,
and FreeBSD-native facilities — devd, sysctl, netgraph — instead of
shimmed Linux subsystems.
