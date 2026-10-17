#!/bin/sh
# Build a bootable FreeBSD installer image with freearchy baked in.
#
# Approach: fetch the official memstick .img, attach it via md(4), inject a
# bsdinstall `installerconfig` (auto-install: ZFS root, freearchy firstboot
# script) plus the freearchy rc.d script, detach, done.
#
# MUST be run on FreeBSD. Tested target: amd64.
set -eu

RELEASE="${1:-15.0}"
ARCH="${ARCH:-aarch64}"      # Apple Silicon dev host default; override for amd64

# FreeBSD names images differently per arch.
case "$ARCH" in
    amd64)   IMG="FreeBSD-${RELEASE}-RELEASE-amd64-memstick.img" ;;
    aarch64) IMG="FreeBSD-${RELEASE}-RELEASE-arm64-aarch64-memstick.img" ;;
    *)       echo "unsupported ARCH: $ARCH" >&2; exit 1 ;;
esac
BASE_URL="https://download.freebsd.org/releases/ISO-IMAGES/${RELEASE}"
WORK="$(mktemp -d /tmp/freearchy-image.XXXXXX)"
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

# TODO: verify partition index on the 15.x memstick layout (root is
# typically md0s2a; EFI images may differ). `gpart show -p /dev/$MD` first.
mount "/dev/${MD}s2a" "$WORK/mnt"

echo "==> Injecting installerconfig + firstboot script"
cp "$REPO/image/installerconfig" "$WORK/mnt/etc/installerconfig"
mkdir -p "$WORK/mnt/usr/local/etc/rc.d"
cp "$REPO/image/freearchy-firstboot" "$WORK/mnt/usr/local/etc/rc.d/freearchy-firstboot"
chmod +x "$WORK/mnt/usr/local/etc/rc.d/freearchy-firstboot"

sync
umount "$WORK/mnt"
mdconfig -d -u "$MD"
MD=""

echo "==> Done: $IMG — write to USB with:"
echo "    dd if=$IMG of=/dev/daN bs=1m conv=sync"
