#!/bin/sh
# Build pijulius/picom — the animated picom fork (spring-physics open/close/
# move animations, dual-kawase blur) that gives X11 the Hyprland-ish feel.
# Not in ports, so we build it. Idempotent; runs as root.
set -eu

PICOM_REPO="${PICOM_REPO:-https://github.com/pijulius/picom.git}"
PICOM_BRANCH="${PICOM_BRANCH:-next}"
BUILD="${PICOM_BUILD:-/tmp/picom-build}"

log() { echo "==> $*"; }

if [ "$(id -u)" -ne 0 ]; then
    SUDO=doas
else
    SUDO=
fi

if [ -d "$BUILD/src" ]; then
    git -C "$BUILD/src" fetch -q --depth 1 origin "$PICOM_BRANCH" || true
    git -C "$BUILD/src" checkout -q FETCH_HEAD || true
else
    mkdir -p "$BUILD"
    git clone -q --depth 1 -b "$PICOM_BRANCH" "$PICOM_REPO" "$BUILD/src"
fi

log "meson setup"
rm -rf "$BUILD/build"
meson setup --buildtype=release -Dwith_docs=false "$BUILD/build" "$BUILD/src"

log "ninja"
ninja -C "$BUILD/build"

log "install"
$SUDO install -m 755 "$BUILD/build/src/picom" /usr/local/bin/picom
# assets (icon/desktop) are cosmetic; binary is all xinitrc needs
log "picom (pijulius, $PICOM_BRANCH) installed to /usr/local/bin/picom"
