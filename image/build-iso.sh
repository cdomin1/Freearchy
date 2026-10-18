#!/bin/sh
# Build a real bootable Vellum ISO (CD/VM format) — the Parallels/UTM path
# that prl_convert can't give you from a raw memstick image.
#
# Remasters the official FreeBSD disc1.iso: mount → copy tree → shared
# injection (image/inject.sh) → rebuild ISO9660 with makefs, keeping the
# original boot catalog (cdboot for BIOS + efiboot.img for UEFI).
#
# MUST be run on FreeBSD. Default target: amd64 (override ARCH=aarch64).
set -eu

RELEASE="${1:-15.1}"
ARCH="${ARCH:-amd64}"
VELLUM_BIN="${VELLUM_BIN:-}"
OUT="${OUT:-vellum-${RELEASE}-${ARCH}-vm-installer.iso}"

case "$ARCH" in
    amd64)   SRC="FreeBSD-${RELEASE}-RELEASE-amd64-disc1.iso" ;;
    aarch64) SRC="FreeBSD-${RELEASE}-RELEASE-arm64-aarch64-disc1.iso" ;;
    *)       echo "unsupported ARCH: $ARCH" >&2; exit 1 ;;
esac
BASE_URL="https://download.freebsd.org/releases/ISO-IMAGES/${RELEASE}"
WORK="$(mktemp -d /tmp/vellum-iso.XXXXXX)"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
. "$REPO/image/inject.sh"

cleanup() {
    umount "$WORK/mnt" 2>/dev/null || true
    [ -n "${MD:-}" ] && mdconfig -d -u "$MD" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT

echo "==> Fetching $SRC"
[ -f "$SRC" ] || fetch -o "$SRC.xz" "${BASE_URL}/${SRC}.xz"
[ -f "$SRC" ] || unxz "$SRC.xz"

echo "==> Extracting installer tree"
MD=$(mdconfig -a -t vnode -f "$SRC")
mkdir -p "$WORK/mnt" "$WORK/tree"
mount -o ro -t cd9660 "/dev/$MD" "$WORK/mnt"
# ISOs are read-only — copy the whole tree to a writable staging dir
cp -a "$WORK/mnt/" "$WORK/tree/"
umount "$WORK/mnt"
mdconfig -d -u "$MD"
MD=""

inject_vellum "$WORK/tree"

echo "==> Rebuilding ISO ($OUT)"
# Same boot catalog the release toolchains emit:
#   amd64  — BIOS El Torito via cdboot + UEFI via efiboot.img
#   aarch64 — UEFI only
BOOT_ARGS="-o bootimage=efi;$WORK/tree/boot/efiboot.img -o platformid=efi -o no-emul-boot"
[ -f "$WORK/tree/boot/efiboot.img" ] || { echo "!! no efiboot.img in tree" >&2; exit 1; }
if [ "$ARCH" = "amd64" ]; then
    BOOT_ARGS="-o bootimage=i386;$WORK/tree/boot/cdboot -o no-emul-boot $BOOT_ARGS"
fi
# shellcheck disable=SC2086 # makefs takes -o key=value pairs unquoted
makefs -t cd9660 -o rockridge -o label="VELLUM_${RELEASE}_${ARCH}" \
    $BOOT_ARGS "$OUT" "$WORK/tree"

echo "==> Done: $OUT"
echo "    attach as a CD/DVD in Parallels, UTM, VirtualBox, QEMU, bhyve"
