#!/bin/sh
# Build/install the Caelestia shell on FreeBSD.
#
# Runs as part of `freearchy install` (or standalone: sh build-shell.sh).
# POSIX sh only. Everything overridable via env — see below.
set -eu

# --- knobs -----------------------------------------------------------------
SHELL_REPO="${SHELL_REPO:-https://github.com/caelestia-dots/shell.git}"
SHELL_BRANCH="${SHELL_BRANCH:-}"      # set to 'freebsd' once our fork exists
QS_GIT="${QS_GIT:-0}"                 # 1 = build quickshell from git instead of pkg
SKIP_CAVA="${SKIP_CAVA:-0}"           # 1 = skip libcava build (hides visualizer)
QS_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/quickshell"
BUILD_DIR="${BUILD_DIR:-/tmp/freearchy-shell-build}"
FONTS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/fonts"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)

log() { printf '\033[1m==> %s\033[0m\n' "$*"; }

# Runs under `freearchy install` (root) or standalone (needs doas for pkg/make install)
SUDO=""
if [ "$(id -u)" != "0" ] && command -v doas >/dev/null 2>&1; then
    SUDO=doas
fi

# --- build deps -------------------------------------------------------------
log "Installing build tools"
$SUDO pkg install -y cmake ninja pkgconf qt6-base qt6-declarative qt6-svg \
    qt6-shadertools qt6-wayland wayland wayland-protocols git-lite \
    autoconf automake libtool gmake

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
    $SUDO cmake --install "$BUILD_DIR/quickshell/build"
else
    log "quickshell via pkg (set QS_GIT=1 to build from git)"
    $SUDO pkg install -y quickshell
fi

# --- libcava -------------------------------------------------------------------
# Must precede caelestia — its QML plugin pkg-configs for cava.
if [ "$SKIP_CAVA" != "1" ]; then
    log "Building libcava (set SKIP_CAVA=1 to skip)"
    CAVA_REPO="${CAVA_REPO:-https://github.com/LukashonakV/cava.git}"
    [ -d "$BUILD_DIR/cava" ] || git clone --depth 1 "$CAVA_REPO" \
        "$BUILD_DIR/cava"
    # strict-ANSI/POSIX feature macros hide BSD symbols (ENOTSUP,
    # CLOCK_MONOTONIC); oss.c has an upstream '#inlcude' typo
    (cd "$BUILD_DIR/cava" && ./autogen.sh && ./configure --prefix=/usr/local \
        && sed -i '' -e 's/-std=c99/-std=gnu99/' \
            -e 's/-D_POSIX_C_SOURCE=200809L//g' \
            -e 's/-D_POSIX_SOURCE//g' \
            -e 's/-D_XOPEN_SOURCE_EXTENDED//g' Makefile \
        && sed -i '' 's/^#inlcude/#include/' input/oss.c \
        && gmake \
        && $SUDO gmake install pkgconfigdir=/usr/local/libdata/pkgconfig) 2>&1 || \
        echo "    warn: libcava build failed — visualizer disabled"
fi

# --- libsensors shim -----------------------------------------------------------
# lm_sensors doesn't exist on FreeBSD — our shim answers the API from sysctl
# thermal nodes so caelestia's plugin can link and show real temps.
if ! pkg-config --exists libsensors 2>/dev/null && \
   ! ldconfig -r 2>/dev/null | grep -q libsensors; then
    log "Installing libsensors shim (FreeBSD sysctl backend)"
    COMPAT="$SCRIPT_DIR/compat/libsensors"
    $SUDO cc -shared -fPIC -I"$COMPAT" -o /usr/local/lib/libsensors.so.5 \
        "$COMPAT/sensors.c"
    $SUDO ln -sf libsensors.so.5 /usr/local/lib/libsensors.so
    $SUDO mkdir -p /usr/local/include/sensors
    $SUDO cp "$COMPAT/sensors/sensors.h" /usr/local/include/sensors/sensors.h
fi

# --- caelestia shell ----------------------------------------------------------
log "Installing caelestia shell config"
mkdir -p "$QS_DIR"
if [ -d "$QS_DIR/caelestia/.git" ]; then
    git -C "$QS_DIR/caelestia" pull --ff-only
else
    if [ -n "$SHELL_BRANCH" ]; then
        git clone -b "$SHELL_BRANCH" "$SHELL_REPO" "$QS_DIR/caelestia"
    else
        git clone --depth 1 "$SHELL_REPO" "$QS_DIR/caelestia"
    fi
fi

# C++ QML plugin — caelestia builds libs into its own dir.
# TODO: verify upstream's build layout; adjust flags if it installs elsewhere.
if [ -f "$QS_DIR/caelestia/CMakeLists.txt" ]; then
    # shallow clones lack tags — upstream derives VERSION from git describe.
    # FreeBSD libc++ gates jthread/stop_token behind EXPERIMENTAL + -lc++experimental.
    VER=$(git -C "$QS_DIR/caelestia" describe --tags 2>/dev/null || echo "0.0.0")
    cmake -S "$QS_DIR/caelestia" -B "$QS_DIR/caelestia/build" \
        -G Ninja -DCMAKE_BUILD_TYPE=Release -DVERSION="$VER" \
        -DCMAKE_CXX_FLAGS="-D_LIBCPP_ENABLE_EXPERIMENTAL -I$SCRIPT_DIR/compat/include" \
        -DCMAKE_SHARED_LINKER_FLAGS="-lc++experimental" \
        -DCMAKE_EXE_LINKER_FLAGS="-lc++experimental"
    cmake --build "$QS_DIR/caelestia/build"
fi

# --- qt6-m3shapes --------------------------------------------------------------
# Caelestia's Material3 shapes QML plugin.
# TODO: pin upstream repo URL once our fork layout is decided.
if [ "${SKIP_M3SHAPES:-0}" != "1" ]; then
    log "Building qt6-m3shapes"
    M3_REPO="${M3SHAPES_REPO:-https://github.com/soramanew/m3shapes.git}"
    [ -d "$BUILD_DIR/qt6-m3shapes" ] || git clone --depth 1 "$M3_REPO" \
        "$BUILD_DIR/qt6-m3shapes"
    cmake -S "$BUILD_DIR/qt6-m3shapes" -B "$BUILD_DIR/qt6-m3shapes/build" \
        -G Ninja -DCMAKE_BUILD_TYPE=Release
    cmake --build "$BUILD_DIR/qt6-m3shapes/build"
    $SUDO cmake --install "$BUILD_DIR/qt6-m3shapes/build"
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
# Caelestia's icon font is Material Symbols Rounded (variable).
fetch_font "MaterialSymbolsRounded.ttf" \
    "https://github.com/google/material-design-icons/raw/master/variablefont/MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D.ttf"
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
