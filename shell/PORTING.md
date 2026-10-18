# Caelestia shell — FreeBSD porting plan

Caelestia is a Quickshell config (QML) plus a small C++ plugin and a Python
helper (`caelestia-cli`). Upstream targets Hyprland only — which is exactly what
we run — so **the only real porting work is replacing Linux-specific system
services**. Strategy:

- Fork `caelestia-dots/shell` → `vellum-shell`, branch `freebsd`
- Fork `caelestia-dots/caelestia` (the installer/dots) for its hyprland config
- Keep changes minimal and behind runtime detection where possible so we can
  keep merging upstream

## Dependency map

| Caelestia expects | FreeBSD equivalent | Status |
|---|---|---|
| `quickshell-git` | `x11/quickshell` **in ports** (0.3.x, maintained). If caelestia needs newer git features, `build-shell.sh` can build it (`QS_GIT=1`, `-DCRASH_REPORTER=OFF -DUSE_JEMALLOC=OFF` per upstream FreeBSD notes) | pkg (fallback: build) |
| `qt6-m3shapes` | build from source (Qt6 plugin, cmake) | **build step needed** |
| `libcava` | **not in ports** (confirmed) — build libcava from source for the visualizer, or hide the widget | **manual build** |
| `brightnessctl` | `sysutils/brightnessctl` — exists | pkg |
| `ddcutil` | `sysutils/ddcutil` — exists | pkg |
| `libqalculate` | `math/libqalculate` — exists | pkg |
| `aubio` | `audio/aubio` — exists | pkg |
| `swappy` | `x11/swappy` — exists | pkg |
| `fish`, `bash` | ports | pkg |
| `NetworkManager` | **not on FreeBSD** (confirmed — no port). Candidates: `net-mgmt/networkmgr` (GhostBSD's tool), or a `Process`-backed `ifconfig`/`wpa_supplicant` wrapper | **patch** |
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

## Build steps for the installer (`vellum install`)

Handled by `shell/build-shell.sh`:

1. `pkg install` everything in `packages/shell.txt` (includes `quickshell`)
2. Optionally rebuild quickshell from git (`QS_GIT=1`) if caelestia requires it
3. Clone `vellum-shell` to `~/.config/quickshell/caelestia`, build its C++ plugin
4. Build qt6-m3shapes; install into Qt6's qml import path
5. Build libcava (visualizer) — or skip with `SKIP_CAVA=1`
6. Install `caelestia-cli`
7. Install fonts that lack ports (material-symbols, rubik) to `~/.local/share/fonts` + `fc-cache`
