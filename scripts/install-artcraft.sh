#!/bin/sh
# install-artcraft — build ArtCraft (storytold/artcraft) on FreeBSD.
#
# EXPERIMENTAL. Builds the pure-Rust `artcraft_native` egui app (x11
# feature — no Tauri/WebKit, which aren't usable on FreeBSD anyway).
# Upstream ships no FreeBSD binaries; this compiles from source.
#
#   sh scripts/install-artcraft.sh
set -eu

REPO=https://github.com/storytold/artcraft
SRC=${ARTCRAFT_SRC:-$HOME/.local/src/artcraft}
BIN=${PREFIX:-$HOME/.local}/bin/artcraft-native

echo "==> deps (needs doas/root for pkg)"
doas pkg install -y rust git-lite pkgconf cmake openssl 2>/dev/null \
    || pkg install -y rust git-lite pkgconf cmake openssl

if [ -d "$SRC/.git" ]; then
    echo "==> updating $SRC"
    git -C "$SRC" pull --ff-only
else
    echo "==> cloning $REPO"
    mkdir -p "$(dirname "$SRC")"
    git clone --depth 1 "$REPO" "$SRC"
fi

cd "$SRC"
# eframe 'glow' needs GL (mesa-dri covers it); 'x11' windowing is the
# default eframe picks when both are compiled in.
echo "==> cargo build --release -p artcraft_native (this takes a while)"
cargo build --release -p artcraft_native

mkdir -p "$(dirname "$BIN")"
install -m 755 target/release/artcraft-native "$BIN"

# desktop entry so it shows in rofi -show drun
apps=${XDG_DATA_HOME:-$HOME/.local/share}/applications
mkdir -p "$apps"
cat > "$apps/artcraft.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=ArtCraft
Comment=AI image/video generation studio
Exec=artcraft-native
Icon=artcraft
Categories=Graphics;
EOF

echo "==> installed $BIN — launch with Super+Space → artcraft"
