# Caelestia shell — FreeBSD porting plan

Caelestia is a Quickshell config (QML) plus a small C++ plugin and a Python
helper (`caelestia-cli`). Upstream targets Hyprland only — which is exactly what
we run — so **the only real porting work is replacing Linux-specific system
services**. Strategy:

- Fork `caelestia-dots/shell` → `freearchy-shell`, branch `freebsd`
- Fork `caelestia-dots/caelestia` (the installer/dots) for its hyprland config
- Keep changes minimal and behind runtime detection where possible so we can
  keep merging upstream

## Dependency map

| Caelestia expects | FreeBSD equivalent | Status |
|---|---|---|
| `quickshell-git` | build from source (partial FreeBSD support upstream; not in ports) | **build step needed** |
| `qt6-m3shapes` | build from source (Qt6 plugin, cmake) | **build step needed** |
| `libcava` | `audio/cava` exists; libcava may need manual build | verify |
| `brightnessctl` | `sysutils/brightnessctl` — exists | pkg |
| `ddcutil` | `sysutils/ddcutil` — exists | pkg |
| `libqalculate` | `math/libqalculate` — exists | pkg |
| `aubio` | `audio/aubio` — exists | pkg |
| `swappy` | `x11/swappy` — exists | pkg |
| `fish`, `bash` | ports | pkg |
| `NetworkManager` | **not on FreeBSD** — see below | **patch** |
| `lm_sensors` | `sysctl dev.cpu.N.temperature`, `hw.acpi.thermal` | **patch** |
| `power-profiles-daemon` | `powerd` (base) / `sysutils/powerdxx` | **patch** |
| `glibc`, `gcc-libs` | n/a | drop |
| PAM (lock screen) | base OpenPAM — quickshell ignores `configDirectory` on FreeBSD, uses `/etc/pam.d` | verify lock works |
| fonts: material-symbols, rubik, cascadia | cascadia-code in ports; material-symbols + rubik likely manual TTF install | installer fetches |

## Service patches required in the shell

1. **Network** — Caelestia's `Services/Network.qml` talks to NetworkManager.
   Options:
   a. Wrap `ifconfig` + `wpa_supplicant` in a small QML `Process`-backed service (most work, cleanest)
   b. Use `net-mgmt/networkmgr` (GhostBSD's NetworkManager-ish tool) if it exposes a usable CLI/DBus API
   c. Degrade gracefully: hide wifi widgets when NM is absent (ship this first)

2. **Sensors** — `lm_sensors` calls → `sysctl` reads (CPU temp, fan where available).

3. **Power profiles** — power-profiles-daemon → `powerd`/`powerdxx` modes, or hide the widget.

4. **Lock screen** — verify quickshell PAM path on FreeBSD (`/etc/pam.d/` is fixed there);
   likely needs a small `pam.d` file shipped by our installer.

5. **Battery** — `hw.acpi.battery.*` sysctls (Caelestia probably uses UPower — check;
   `sysutils/upower` exists on FreeBSD and may just work).

## Build steps for the installer (`freearchy install`)

1. `pkg install` everything in `packages/shell.txt`
2. Build quickshell: `cmake -B build -DCMAKE_BUILD_TYPE=Release -DCRASH_REPORTER=OFF -DUSE_JEMALLOC=OFF && cmake --build build && cmake --install build`
3. Build qt6-m3shapes similarly; install into Qt6's qml import path
4. Clone `freearchy-shell` to `~/.config/quickshell/caelestia`, build its C++ plugin (cmake)
5. Install `caelestia-cli` (pipx or a small wrapper)
6. Install fonts that lack ports (material-symbols, rubik) to `~/.local/share/fonts`
