![vellum](branding/vellum-logo.svg)

An Omarchy-inspired, opinionated desktop for **FreeBSD**.

Turn a fresh FreeBSD 15.x install into a keyboard-driven Hyprland desktop with the
Caelestia shell, curated apps, and a theme system — managed by a single `vellum`
binary written in Rust.

## Stack

| Layer | Choice | Why |
|---|---|---|
| Base OS | FreeBSD 15.x RELEASE | ZFS root + `bectl` boot environments = the Omarchy snapshot story, natively |
| Window manager | `vellum-wm` (Rust, penrose/X11) | Xorg needs no seatd/KMS handshake; inspired by end-4's X11 Hypr fork |
| Compositor | picom | Rounded corners / shadows on X11 |
| Shell | Caelestia (Quickshell), FreeBSD-patched | **Shelved** — Wayland-native; see `shell/PORTING.md` |
| Wayland shelf | Hyprland + `seatd` | Kept installed; `Hyprland` still launches it |
| Tooling | `vellum` CLI (Rust) | Single static binary, no bash-in-base dependency |

## Quick start — installer image (pre-alpha)

Download `FreeBSD-15.1-RELEASE-amd64-memstick.img.xz` from the latest
[release](https://github.com/cdomin1/Vellum/releases), then write it to a
USB stick:

```sh
unxz vellum-*.img.xz
dd if=vellum-*.img of=/dev/daN bs=1m conv=sync   # or balenaEtcher
```

Boot it → the Vellum wizard starts on the console: keymap → user →
hostname → disk → erase-confirm → install → reboot. **Ethernet required**
(no wifi provisioning in the live env yet — the installer fetches base dists
and the vellum payload from the network).

## Quick start — on an existing FreeBSD install

```sh
fetch https://raw.githubusercontent.com/cdomin1/Vellum/main/boot.sh -o - | sh
```

`boot.sh` installs git + rust, clones this repo to `~/.local/share/vellum`,
builds the CLI, and runs `vellum install`.

## Layout

```
vellum-cli/   Rust CLI: install / update / theme / doctor / image / install-system
packages/        pkg manifests (base, hyprland, shell, desktop, dev)
config/hyprland/ Default Hyprland configuration
shell/           Caelestia fork notes + FreeBSD port patches
themes/          Theme packs, applied via `vellum theme set`
image/           Bootable installer-image builder
docs/            Roadmap and notes
```

## Philosophy

Same contract as Omarchy: one command, a fully-working, opinionated desktop.
Where Omarchy leans on Arch packages and bash, vellum leans on `pkg` and a
compiled CLI — everything else (dotfiles, themes, curated defaults) is the same idea.
