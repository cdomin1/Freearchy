#!/bin/sh
# vellum bootstrap — run on a fresh FreeBSD 15.x install:
#
#   fetch https://raw.githubusercontent.com/cdomin1/Vellum/main/boot.sh -o - | sh
#
# POSIX sh only — bash is not in FreeBSD base.
set -eu

VELLUM_HOME="${XDG_DATA_HOME:-$HOME/.local/share}/vellum"
REPO_URL="${VELLUM_REPO:-https://github.com/cdomin1/Vellum.git}"

echo "==> Installing bootstrap tools (git, rust)"
pkg install -y git-lite rust

echo "==> Cloning vellum to $VELLUM_HOME"
if [ -d "$VELLUM_HOME" ]; then
    git -C "$VELLUM_HOME" pull --ff-only
else
    git clone "$REPO_URL" "$VELLUM_HOME"
fi

echo "==> Building vellum CLI"
cargo install --path "$VELLUM_HOME/vellum-cli" --root "$HOME/.local"

echo "==> Running vellum install"
exec "$HOME/.local/bin/vellum" install
