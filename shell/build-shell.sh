#!/bin/sh
# Build/install the Caelestia shell on FreeBSD.
#
# Runs as part of `freearchy install` (or standalone: sh build-shell.sh).
# POSIX sh only. Everything overridable via env — see below.
set -eu

# --- knobs -----------------------------------------------------------------
SHELL_REPO="${SHELL_REPO:-https://github.com/caelestia-dots/shell.git}"
SHELL_BRANCH="${SHELL_BRANCH:-freebsd}"
QS_GIT="${QS_GIT:-0}"                 # 1 = build quickshell from git instead of pkg
SKIP_CAVA="${SKIP_CAVA:-0}"           # 1 = skip libcava build (hides visualizer)
QS_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/quickshell"
BUILD_DIR="${BUILD_DIR:-/tmp/freearchy-shell-build}"
FONTS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/fonts"

log() { printf '\033[1m==> %s\033[0m\n' "$*"; }

# --- build deps -------------------------------------------------------------
log "Installing build tools"
pkg install -y cmake ninja pkgconf qt6-base qt6-declarative qt6-svg \
    qt6-shadertools qt6-wayland wayland wayland-protocols git-lite

mkdir -p "$BUILD_DIR"

# --- quickshell ---------------------------------------------------------------
if [ "$QS_GIT" = "1" ]; then
    log "Building quickshell from git (QS_GIT=1)"
    [ -d "$BUILD_DIR/quickshell" ] || \
        git clone --depth 1 https://git.outfoxxed.me/quickshell/quickshell \
            "$BUILD_DIR/quickshell"
    cmake -S "$BUILD_DIR/quickshell" -B "$BUILD_DIR/quickshell/build" \
        -G Ninja -DCMAKE_BUILD_TYPE=Release \
        -DCRASH_REPORTER=OFF -DUSE_JEMALLOC=OFF   # required on FreeBSD
    cmake --build "$BUILD_DIR/quickshell/build"
    cmake --install "$BUILD_DIR/quickshell/build"
else
    log "quickshell via pkg (set QS_GIT=1 to build from git)"
    pkg install -y quickshell
fi

# --- caelestia shell ----------------------------------------------------------
log "Installing caelestia shell config"
mkdir -p "$QS_DIR"
if [ -d "$QS_DIR/caelestia/.git" ]; then
    git -C "$QS_DIR/caelestia" pull --ff-only
else
    git clone -b "$SHELL_BRANCH" "$SHELL_REPO" "$QS_DIR/caelestia"
fi

# C++ QML plugin — caelestia builds libs into its own dir.
# TODO: verify upstream's build layout; adjust flags if it installs elsewhere.
if [ -f "$QS_DIR/caelestia/CMakeLists.txt" ]; then
    cmake -S "$QS_DIR/caelestia" -B "$QS_DIR/caelestia/build" \
        -G Ninja -DCMAKE_BUILD_TYPE=Release
    cmake --build "$QS_DIR/caelestia/build"
fi

# --- qt6-m3shapes --------------------------------------------------------------
# Caelestia's Material3 shapes QML plugin.
# TODO: pin upstream repo URL once our fork layout is decided.
if [ "${SKIP_M3SHAPES:-0}" != "1" ]; then
    log "Building qt6-m3shapes"
    M3_REPO="${M3SHAPES_REPO:-https://github.com/caelestia-dots/qt6-m3shapes.git}"
    [ -d "$BUILD_DIR/qt6-m3shapes" ] || git clone --depth 1 "$M3_REPO" \
        "$BUILD_DIR/qt6-m3shapes"
    cmake -S "$BUILD_DIR/qt6-m3shapes" -B "$BUILD_DIR/qt6-m3shapes/build" \
        -G Ninja -DCMAKE_BUILD_TYPE=Release
    cmake --build "$BUILD_DIR/qt6-m3shapes/build"
    cmake --install "$BUILD_DIR/qt6-m3shapes/build"
fi

# --- libcava -------------------------------------------------------------------
if [ "$SKIP_CAVA" != "1" ]; then
    log "Building libcava (set SKIP_CAVA=1 to skip)"
    CAVA_REPO="${CAVA_REPO:-https://github.com/LukashonakV/cava.git}"
    [ -d "$BUILD_DIR/cava" ] || git clone --depth 1 -b libcava "$CAVA_REPO" \
        "$BUILD_DIR/cava"
    # libcava is a library target in this fork; autotools build.
    (cd "$BUILD_DIR/cava" && ./autogen.sh && ./configure --prefix=/usr/local \
        && make && make install)
fi

# --- fonts ---------------------------------------------------------------------
log "Installing fonts not in ports"
mkdir -p "$FONTS_DIR"
fetch_font() {  # name url
    dest="$FONTS_DIR/$1"
    if [ ! -f "$dest" ]; then
        fetch -o "$dest" "$2" || echo "    warn: font $1 fetch failed"
    fi
}
# TODO: pin versions. Material Symbols variable TTF + Rubik from upstream repos.
fetch_font "MaterialSymbolsVariable.ttf" \
    "https://github.com/google/material-design-icons/raw/master/variablefont/MaterialSymbols%5BFILL%2CGRAD%2Copsz%2Cwght%5D.ttf"
fc-cache -f "$FONTS_DIR" >/dev/null 2>&1 || true

# --- caelestia-cli ---------------------------------------------------------------
log "Installing caelestia-cli"
if command -v pipx >/dev/null 2>&1; then
    pipx install --force "git+${SHELL_CLI_REPO:-https://github.com/caelestia-dots/caelestia-cli.git}" || \
        echo "    warn: caelestia-cli install failed (python tooling optional)"
else
    echo "    warn: pipx not installed — skipping caelestia-cli"
fi

log "Shell installed. It starts from hyprland's autostart (quickshell -c caelestia)."
