#!/bin/sh
# Build a real bootable Vellum ISO (CD/VM format) — the Parallels/UTM path
# that prl_convert can't give you from a raw memstick image.
#
# Remasters the official FreeBSD disc1.iso: mount → copy tree → shared
# injection (image/inject.sh) → rebuild ISO9660 with makefs, regenerating
# the UEFI ESP from boot/loader.efi exactly like release mkisoimages.sh
# (cdboot added for BIOS on amd64).
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
# ISOs are read-only — copy the whole tree to a staging dir, then undo
# the 0555/0444 bits cp -a carried over (injection writes into it)
cp -a "$WORK/mnt/" "$WORK/tree/"
chmod -R u+w "$WORK/tree"
umount "$WORK/mnt"
mdconfig -d -u "$MD"
MD=""

inject_vellum "$WORK/tree"

echo "==> Rebuilding ISO ($OUT)"
# disc1 trees don't ship an ESP image — the release toolchain builds it
# on the fly from boot/loader.efi (see release/*/mkisoimages.sh:
# make_esp_file). Do the same: FAT12 image + EFI/BOOT/BOOT<ARCH>.EFI.
EFI_NAME=""
case "$ARCH" in
    amd64)   EFI_NAME="BOOTX64.EFI" ;;
    aarch64) EFI_NAME="BOOTAA64.EFI" ;;
esac
ESP="$WORK/efiboot.img"
[ -f "$WORK/tree/boot/loader.efi" ] || { echo "!! no boot/loader.efi in tree" >&2; exit 1; }
dd if=/dev/zero of="$ESP" bs=1k count=2048 2>/dev/null
ESPMD=$(mdconfig -a -t vnode -f "$ESP")
newfs_msdos -F 12 -L EFISYS "/dev/$ESPMD" >/dev/null
mkdir -p "$WORK/esp-mnt"
mount -t msdosfs "/dev/$ESPMD" "$WORK/esp-mnt"
mkdir -p "$WORK/esp-mnt/efi/boot"
cp "$WORK/tree/boot/loader.efi" "$WORK/esp-mnt/efi/boot/$EFI_NAME"
umount "$WORK/esp-mnt"
mdconfig -d -u "$ESPMD"

# Same boot catalog the release toolchains emit:
#   amd64   — BIOS El Torito via cdboot + UEFI via the ESP above
#   aarch64 — UEFI only
BOOT_ARGS="-o bootimage=efi;$ESP -o platformid=efi -o no-emul-boot"
if [ "$ARCH" = "amd64" ]; then
    [ -f "$WORK/tree/boot/cdboot" ] || { echo "!! no cdboot in tree" >&2; exit 1; }
    BOOT_ARGS="-o bootimage=i386;$WORK/tree/boot/cdboot -o no-emul-boot $BOOT_ARGS"
fi
# shellcheck disable=SC2086 # makefs takes -o key=value pairs unquoted
LABEL="VELLUM_$(echo "$RELEASE" | tr -d .)_$(echo "$ARCH" | tr '[:lower:]' '[:upper:]')"
makefs -t cd9660 -o rockridge -o label="$LABEL" \
    $BOOT_ARGS "$OUT" "$WORK/tree"

echo "==> Done: $OUT"
echo "    attach as a CD/DVD in Parallels, UTM, VirtualBox, QEMU, bhyve"
