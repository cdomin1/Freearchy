#!/bin/sh
# freearchy bootstrap — run on a fresh FreeBSD 15.x install:
#
#   fetch https://raw.githubusercontent.com/cdomin1/Freearchy/main/boot.sh -o - | sh
#
# POSIX sh only — bash is not in FreeBSD base.
set -eu

FREEARCHY_HOME="${XDG_DATA_HOME:-$HOME/.local/share}/freearchy"
REPO_URL="${FREEARCHY_REPO:-https://github.com/cdomin1/Freearchy.git}"

echo "==> Installing bootstrap tools (git, rust)"
pkg install -y git-lite rust

echo "==> Cloning freearchy to $FREEARCHY_HOME"
if [ -d "$FREEARCHY_HOME" ]; then
    git -C "$FREEARCHY_HOME" pull --ff-only
else
    git clone "$REPO_URL" "$FREEARCHY_HOME"
fi

echo "==> Building freearchy CLI"
cargo install --path "$FREEARCHY_HOME/freearchy-cli" --root "$HOME/.local"

echo "==> Running freearchy install"
exec "$HOME/.local/bin/freearchy" install
