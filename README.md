![freearchy](branding/logo.png)

An Omarchy-inspired, opinionated desktop for **FreeBSD**.

Turn a fresh FreeBSD 15.x install into a keyboard-driven Hyprland desktop with the
Caelestia shell, curated apps, and a theme system — managed by a single `freearchy`
binary written in Rust.

## Stack

| Layer | Choice | Why |
|---|---|---|
| Base OS | FreeBSD 15.x RELEASE | ZFS root + `bectl` boot environments = the Omarchy snapshot story, natively |
| Compositor | Hyprland (`x11-wm/hyprland`) | Caelestia is Hyprland-native; Omarchy fidelity |
| Shell | Caelestia (Quickshell), FreeBSD-patched | See `shell/PORTING.md` |
| Session | `seatd` + `turnstile` | `XDG_RUNTIME_DIR` + seat management without systemd/elogind |
| Tooling | `freearchy` CLI (Rust) | Single static binary, no bash-in-base dependency |

## Quick start — installer image (pre-alpha)

Download `FreeBSD-15.1-RELEASE-amd64-memstick.img.xz` from the latest
[release](https://github.com/cdomin1/Freearchy/releases), then write it to a
USB stick:

```sh
unxz freearchy-*.img.xz
dd if=freearchy-*.img of=/dev/daN bs=1m conv=sync   # or balenaEtcher
```

Boot it → the Freearchy wizard starts on the console: keymap → user →
hostname → disk → erase-confirm → install → reboot. **Ethernet required**
(no wifi provisioning in the live env yet — the installer fetches base dists
and the freearchy payload from the network).

## Quick start — on an existing FreeBSD install

```sh
fetch https://raw.githubusercontent.com/cdomin1/Freearchy/main/boot.sh -o - | sh
```

`boot.sh` installs git + rust, clones this repo to `~/.local/share/freearchy`,
builds the CLI, and runs `freearchy install`.

## Layout

```
freearchy-cli/   Rust CLI: install / update / theme / doctor / image / install-system
packages/        pkg manifests (base, hyprland, shell, desktop, dev)
config/hyprland/ Default Hyprland configuration
shell/           Caelestia fork notes + FreeBSD port patches
themes/          Theme packs, applied via `freearchy theme set`
image/           Bootable installer-image builder
docs/            Roadmap and notes
```

## Philosophy

Same contract as Omarchy: one command, a fully-working, opinionated desktop.
Where Omarchy leans on Arch packages and bash, freearchy leans on `pkg` and a
compiled CLI — everything else (dotfiles, themes, curated defaults) is the same idea.
