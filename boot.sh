#!/bin/sh
# vellum bootstrap — run as root on a fresh FreeBSD 15.x install (fresh
# installs have no sudo/doas yet):
#
#   su -
#   fetch https://raw.githubusercontent.com/cdomin1/vellum/main/boot.sh -o - | sh
#
# The desktop is configured for the user who logged in and ran `su`
# (override with VELLUM_USER=name). POSIX sh only — bash is not in base.
set -eu

[ "$(id -u)" -eq 0 ] || { echo "boot.sh must run as root (su - first)" >&2; exit 1; }

TARGET_USER="${VELLUM_USER:-${SUDO_USER:-${DOAS_USER:-$(logname 2>/dev/null || true)}}}"
if [ -z "$TARGET_USER" ] || [ "$TARGET_USER" = root ]; then
    echo "can't tell which user the desktop is for — rerun as: VELLUM_USER=<name> sh boot.sh" >&2
    exit 1
fi
TARGET_HOME=$(pw usershow -n "$TARGET_USER" | cut -d: -f9)

VELLUM_HOME="$TARGET_HOME/.local/share/vellum"
REPO_URL="${VELLUM_REPO:-https://github.com/cdomin1/vellum.git}"

echo "==> Installing bootstrap tools (git, rust)"
ASSUME_ALWAYS_YES=yes pkg bootstrap -f >/dev/null 2>&1 || true
pkg install -y git-lite rust

echo "==> Cloning vellum to $VELLUM_HOME"
if [ -d "$VELLUM_HOME/.git" ]; then
    git -C "$VELLUM_HOME" pull --ff-only
else
    mkdir -p "$(dirname "$VELLUM_HOME")"
    git clone "$REPO_URL" "$VELLUM_HOME"
fi
chown -R "$TARGET_USER:$TARGET_USER" "$TARGET_HOME/.local"

echo "==> Building vellum CLI"
cargo install --path "$VELLUM_HOME/vellum-cli" --root /usr/local

echo "==> Running vellum install for $TARGET_USER"
exec env VELLUM_HOME="$VELLUM_HOME" SUDO_USER="$TARGET_USER" /usr/local/bin/vellum install
