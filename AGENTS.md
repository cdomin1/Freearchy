# VELLUM — agent notes

Omarchy-style desktop on FreeBSD 15.1. Rust workspace: `vellum-cli`
(package/binary name `vellum`), `vellum-wm` (penrose X11 WM), `vellum-bt`
(Bluetooth scaffold). X11 is the primary, VM-testable session; Hyprland /
Caelestia is a shelved Wayland path.

## Building / checking locally (macOS)

The default Command Line Tools SDK (MacOSX27) ships `.tbd` stubs the
installed linker can't parse, so build scripts / proc-macros fail to link.
Point at the 15.4 SDK instead:

```sh
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk
cargo clippy --workspace --all-targets   # must be warning-free
cargo test --workspace
```

Everything compiles on macOS (x11rb is pure Rust); runtime needs FreeBSD.
Shell scripts: `sh -n <file>` (no shellcheck installed).

## CI

- `.github/workflows/iso.yml` — on `v*` tags: builds `vellum` in a FreeBSD
  VM (vmactions, `sync: no`, `envs:` for env passthrough), remasters the
  official memstick, uploads `vellum-15.1-{amd64,aarch64}-memstick.img.xz`
  to the release (stale assets deleted first, so retagging works).
- `screenshots.yml` — Xvfb session captures as artifacts.
- `pages.yml` — deploys `site/` to https://cdomin1.github.io/vellum/.

## Conventions / gotchas

- penrose `spawn` splits on whitespace without a shell — use `spawn_sh`
  in vellum-wm for `$VAR` / `$(...)`.
- FreeBSD `mixer(8)` relative steps need `%` (`vol=+5%`); `backlight(8)`
  takes `incr|decr N`; hwpstate EPP sysctls are integers 0–100 per CPU.
- install-system runs under the ratatui TUI: pipeline commands go through
  `install_system::sh` (captured output), never `util::run` (prints).
- First boot only runs because the installer writes `/firstboot`; the rc
  script configures `vellum_firstboot_user`'s home via `SUDO_USER`.
- Boot splash: regenerate `image/splash.png` with `scripts/gen-splash.py`
  (RGBA PNG; vt(4) splash only exists in the amd64 GENERIC kernel).
