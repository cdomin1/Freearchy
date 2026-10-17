# Testing freearchy — UTM (Apple Silicon)

The fast loop is **stock FreeBSD VM → run `boot.sh` → fix what breaks**.
Only bake the custom image once that path is green.

## 1. Create the VM

- Install **UTM**: `brew install --cask utm`, or the free dmg from
  getutm.app / GitHub releases (the paid App Store build is identical;
  it just funds the project). No account needed.
- Download: `FreeBSD-15.0-RELEASE-arm64-aarch64-disc1.iso` from
  `download.freebsd.org/releases/ISO-IMAGES/15.0/` — **aarch64**, not amd64
  (Apple Silicon; x86 means full emulation, too slow to be useful)
- UTM → New → **Virtualize** → **Other** → pick the ISO
- 2+ vCPU, 4+ GB RAM, 20+ GB disk, shared (NAT) networking — pkg needs internet
- Display device: leave the default **virtio-gpu** — FreeBSD's drm-kmod ships
  a `virtio_gpu` driver; this is our best shot at a real Wayland session in-VM

## 2. Install FreeBSD

Stock `bsdinstall`, with these choices:

- **ZFS root** (matches our image builder's `installerconfig`)
- Create a normal user; add it to `wheel` and `video`
- **Enable `sshd`** — copy/paste between macOS and the guest is unreliable;
  plan to `ssh` in from the Mac and paste `boot.sh`'s one-liner there
- Take a UTM **snapshot** right after install, before touching anything

## 3. Run freearchy

```sh
# inside the guest (or over ssh)
fetch https://raw.githubusercontent.com/USER/freearchy/main/boot.sh -o - | sh
```

Or clone the repo by hand and run pieces:

```sh
pkg install git-lite rust
git clone <repo> ~/.local/share/freearchy
cargo install --path ~/.local/share/freearchy/freearchy-cli --root ~/.local
freearchy install          # add --no-packages / --no-shell to bisect failures
freearchy doctor
```

## 4. Getting a graphical session

This is the risky step in any VM:

```sh
sysrc kld_list+=virtio_gpu     # from drm-kmod
kldload virtio_gpu             # or reboot
ls /dev/drm                    # want card0/renderD128 — no nodes = no Wayland
service seatd start            # installer does this; verify anyway
freearchy doctor
Hyprland                       # from a getty, as your user
```

- If `/dev/drm` nodes exist, Hyprland should come up (software GL is fine to start)
- Whether FreeBSD's `virtio_gpu` attaches to the UTM/QEMU device on aarch64 is
  the least-certain link in the chain — if no DRM nodes appear, everything
  *except* the live session is still testable (packages, configs, themes,
  doctor, the quickshell build), and compositor validation moves to real hw
- Escape hatch: run Hyprland on its headless backend to smoke-test config
  parse (`Hyprland` with a headless-capable backend env — verify current
  variable name against the hyprland wiki)

## 5. What "green" looks like (checklist)

- [ ] `pkg` installs all required manifests with zero failures (trim strays)
- [ ] `freearchy doctor` — all checks ok after install
- [ ] `quickshell -c caelestia` launches (even with stubbed services)
- [ ] Hyprland session starts; foot + launcher binds work
- [ ] `freearchy theme set catppuccin-mocha` visibly re-themes borders/foot
- [ ] Reboot → session still comes up clean

## 6. Then the ISO

`freearchy image build --release 15.0` (ARCH=aarch64 by default) produces a
memstick `.img` — boot the *built* image in UTM the same way; the bsdinstall
`installerconfig` auto-installs and first-boot runs freearchy.

## Alternatives

- **tart** (`brew install tart`) — free, CLI-only, Apple Virtualization.framework;
  nice for scripted/headless testing, graphics window support is thinner
- **qemu via brew** — full control, but you reimplement half of UTM by hand
- **Real hardware** — still required for the last 10% (actual GPU) validation
