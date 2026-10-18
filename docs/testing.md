# Testing vellum — Parallels (Apple Silicon)

The fast loop is **stock FreeBSD VM → run `boot.sh` → fix what breaks**.
Only bake the custom image once that path is green.

Current rig: a Parallels FreeBSD 15.1 aarch64 VM, driven over SSH from macOS
Terminal (`ssh carlos@<vm-ip>` — Parallels' console can't share the
clipboard; new Terminal tabs land on the Mac, ssh'd tabs on the VM).
SSH key at `~/.ssh/vellum_vm`, `doas` configured (`permit nopass carlos`).

## install-system test hooks

The wizard's disk list and pool name can be overridden to exercise the full
pipeline on a RAM disk without touching real storage:

```sh
mdconfig -a -t malloc -s 3g          # creates e.g. md3
VELLUM_DISKS='md3:RAM Test Disk' VELLUM_POOL=zmd3 \
    vellum install-system         # as root
```

`VELLUM_POOL` also prevents colliding with a host that already imports a
pool named `zroot` (any ZFS-rooted machine). The pipeline refuses to destroy
a pool whose vdev isn't the selected disk.

## 1. Create the VM

- Install **UTM**: `brew install --cask utm`, or the free dmg from
  getutm.app / GitHub releases (the paid App Store build is identical;
  it just funds the project). No account needed.
- Download: `FreeBSD-15.1-RELEASE-arm64-aarch64-disc1.iso` from
  `download.freebsd.org/releases/ISO-IMAGES/15.1/` — **aarch64**, not amd64
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

## 3. Run vellum

```sh
# inside the guest (or over ssh)
fetch https://raw.githubusercontent.com/cdomin1/Vellum/main/boot.sh -o - | sh
```

Or push the repo from a Mac checkout (COPYFILE_DISABLE avoids macOS
`._*` AppleDouble files polluting the tree):

```sh
cd ~/Developer && COPYFILE_DISABLE=1 tar czf - --exclude target vellum | \
    ssh <user>@<vm-ip> 'tar xzf -'
```

Then inside the guest (repo already at ~/vellum after the tar copy):

```sh
pkg install -y git-lite rust          # as root
cargo install --path ~/vellum/vellum-cli --root /usr/local
export VELLUM_HOME=~/vellum     # repo isn't at the default path here
vellum doctor                      # baseline — expect failures
vellum install                     # as root; add --no-packages / --no-shell to bisect
```

## 3b. Or: skip bsdinstall entirely (GUI installer)

Boot the ISO → pick **"Live CD"** → you get a root shell. Then:

```sh
pkg bootstrap -f && pkg install -y git-lite rust
git clone <repo> /root/vellum
cargo build --release --manifest-path /root/vellum/vellum-cli/Cargo.toml
/root/vellum/target/release/vellum install-system --web
```

Then open `http://<vm-ip>:8080` **in a browser on your Mac** — the GUI renders
there while the install runs in the VM. No display stack needed in the guest.
(`--yes` + flags runs the same pipeline with no UI at all; plain
`install-system` uses terminal prompts.)

## 4. Getting a graphical session

This is the risky step in any VM:

```sh
sysrc kld_list+=virtio_gpu     # from drm-kmod
kldload virtio_gpu             # or reboot
ls /dev/drm                    # want card0/renderD128 — no nodes = no Wayland
service seatd start            # installer does this; verify anyway
vellum doctor
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
- [ ] `vellum doctor` — all checks ok after install
- [ ] `quickshell -c caelestia` launches (even with stubbed services)
- [ ] Hyprland session starts; foot + launcher binds work
- [ ] `vellum theme set catppuccin-mocha` visibly re-themes borders/foot
- [ ] Reboot → session still comes up clean

## 6. The installer image

`image/build-image.sh` (default ARCH=amd64; the amd64 binary is built by the
`iso` GitHub workflow on tag/dispatch) remasters the official memstick:
`rc.local` runs `vellum-install` per console → DHCP on all NICs →
`vellum install-system` TUI. Falls back to stock bsdinstall if the
binary is absent. Live env needs **ethernet** (dists + payload are fetched;
no wifi provisioning yet).

Boot the built image the same way (UTM/Parallels from the .img, or dd to
USB for real hardware).

## Alternatives

- **Parallels** — easiest UX; virtio disk/NIC usually just work (`vtbd0`,
  `vtnet0`). Caveat: its display isn't a KMS-capable device, so expect no
  `/dev/drm` nodes → everything except the Hyprland session is testable.
- **tart** (`brew install tart`) — free, CLI-only, Apple Virtualization.framework;
  nice for scripted/headless testing, graphics window support is thinner
- **qemu via brew** — full control, but you reimplement half of UTM by hand
- **Real hardware** — still required for the last 10% (actual GPU) validation
