#!/bin/sh
# Build a bootable Vellum installer image.
#
# Fetches the official FreeBSD memstick .img, attaches it via md(4), then
# injects:
#   - /usr/local/bin/vellum          prebuilt CLI (if VELLUM_BIN set)
#   - /usr/local/bin/vellum-install  console launcher (replaces bsdinstall)
#   - patched /etc/rc.local             runs our launcher via runconsoles
#
# Boot flow: rc.local → runconsoles → vellum-install → vellum
# install-system (the ratatui wizard). If the binary is absent the launcher
# falls back to stock bsdinstall.
#
# MUST be run on FreeBSD. Default target: amd64 (override ARCH=aarch64).
set -eu

RELEASE="${1:-15.1}"
ARCH="${ARCH:-amd64}"
VELLUM_BIN="${VELLUM_BIN:-}"

case "$ARCH" in
    amd64)   IMG="FreeBSD-${RELEASE}-RELEASE-amd64-memstick.img" ;;
    aarch64) IMG="FreeBSD-${RELEASE}-RELEASE-arm64-aarch64-memstick.img" ;;
    *)       echo "unsupported ARCH: $ARCH" >&2; exit 1 ;;
esac
BASE_URL="https://download.freebsd.org/releases/ISO-IMAGES/${RELEASE}"
WORK="$(mktemp -d /tmp/vellum-image.XXXXXX)"
REPO="$(cd "$(dirname "$0")/.." && pwd)"

cleanup() {
    umount "$WORK/mnt" 2>/dev/null || true
    [ -n "${MD:-}" ] && mdconfig -d -u "$MD" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT

echo "==> Fetching $IMG"
[ -f "$IMG" ] || fetch -o "$IMG.xz" "${BASE_URL}/${IMG}.xz"
[ -f "$IMG" ] || unxz "$IMG.xz"

echo "==> Attaching image"
MD=$(mdconfig -a -t vnode -f "$IMG")
mkdir -p "$WORK/mnt"

# amd64 memstick: MBR → freebsd → s2a ufs root
# aarch64 memstick: GPT → p2 freebsd-ufs root
UFS_PART=$(gpart show -p "/dev/$MD" | awk '$4 == "freebsd-ufs" {print $3; exit}')
if [ -z "$UFS_PART" ]; then
    # MBR: ufs lives inside the freebsd slice (mdNs2 → mdNs2a)
    SLICE=$(gpart show -p "/dev/$MD" | awk '$4 == "freebsd" {print $3; exit}')
    [ -n "$SLICE" ] && \
        UFS_PART=$(gpart show -p "/dev/$SLICE" | awk '$4 == "freebsd-ufs" {print $3; exit}')
fi
[ -n "$UFS_PART" ] || { echo "!! no freebsd-ufs partition on $MD" >&2; exit 1; }
mount -o rw "/dev/$UFS_PART" "$WORK/mnt"

echo "==> Injecting vellum launcher"
mkdir -p "$WORK/mnt/usr/local/bin"
cp "$REPO/image/vellum-install" "$WORK/mnt/usr/local/bin/vellum-install"
chmod 755 "$WORK/mnt/usr/local/bin/vellum-install"

if [ -n "$VELLUM_BIN" ] && [ -f "$VELLUM_BIN" ]; then
    echo "==> Embedding vellum binary ($VELLUM_BIN)"
    cp "$VELLUM_BIN" "$WORK/mnt/usr/local/bin/vellum"
    chmod 755 "$WORK/mnt/usr/local/bin/vellum"
else
    echo "!! VELLUM_BIN unset — image will fall back to stock bsdinstall"
fi

echo "==> Pointing rc.local at the vellum installer"
sed -i '' 's|/usr/libexec/bsdinstall/startbsdinstall|/usr/local/bin/vellum-install|g' \
    "$WORK/mnt/etc/rc.local"
grep -q vellum-install "$WORK/mnt/etc/rc.local" \
    || { echo "!! rc.local patch did not apply" >&2; exit 1; }

sync
umount "$WORK/mnt"
mdconfig -d -u "$MD"
MD=""

echo "==> Done: $IMG"
echo "    write to USB:  dd if=$IMG of=/dev/daN bs=1m conv=sync"
