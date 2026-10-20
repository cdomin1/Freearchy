#!/bin/sh
# inject.sh — shared Vellum injection for remastered FreeBSD images.
# Sourced by build-image.sh / build-iso.sh; expects:
#   REPO       — path to the vellum checkout
#   VELLUM_BIN — optional prebuilt `vellum` binary to embed
#
# inject_vellum <root>  applies to a mounted/copied installer root:
#   - /usr/local/bin/vellum-install  console launcher (replaces bsdinstall)
#   - /usr/local/bin/vellum-setup    inline gum wizard (config → engine)
#   - /usr/local/bin/vellum          prebuilt CLI (if VELLUM_BIN set)
#   - /usr/local/bin/gum             static Charm gum binary for the arch
#   - patched /etc/rc.local           runs our launcher via runconsoles
#   - loader.conf quiet boot + VELLUM splash
inject_vellum() {
    ROOT="$1"
    [ -d "$ROOT/etc" ] || { echo "!! $ROOT doesn't look like a FreeBSD root" >&2; return 1; }

    echo "==> Injecting vellum installer into $ROOT"
    mkdir -p "$ROOT/usr/local/bin"
    cp "$REPO/image/vellum-install" "$ROOT/usr/local/bin/vellum-install"
    cp "$REPO/image/vellum-setup" "$ROOT/usr/local/bin/vellum-setup"
    chmod 755 "$ROOT/usr/local/bin/vellum-install" "$ROOT/usr/local/bin/vellum-setup"

    # Static gum for the wizard's inline prompts. ARCH comes from the build
    # scripts; fall back to the build host's arch. If the fetch fails the
    # wizard still works — it degrades to plain POSIX prompts.
    GUM_VER=2.0.2
    case "${ARCH:-$(uname -m)}" in
        amd64|x86_64) GUM_ARCH=x86_64 ;;
        arm64|aarch64) GUM_ARCH=arm64 ;;
        *) GUM_ARCH="" ;;
    esac
    if [ -n "$GUM_ARCH" ]; then
        GUM_URL="https://github.com/charmbracelet/gum/releases/download/v${GUM_VER}/gum_${GUM_VER}_Freebsd_${GUM_ARCH}.tar.gz"
        GTMP=$(mktemp -d)
        # tarball nests the binary: gum_<ver>_Freebsd_<arch>/gum
        if fetch -qo "$GTMP/gum.tar.gz" "$GUM_URL" && tar -xzf "$GTMP/gum.tar.gz" -C "$GTMP" 2>/dev/null; then
            cp "$GTMP"/*/gum "$ROOT/usr/local/bin/gum"
            chmod 755 "$ROOT/usr/local/bin/gum"
            echo "==> Embedded gum $GUM_VER ($GUM_ARCH)"
        else
            echo "!! gum fetch failed — wizard will use plain prompts" >&2
        fi
        rm -rf "$GTMP"
    fi

    if [ -n "$VELLUM_BIN" ] && [ -f "$VELLUM_BIN" ]; then
        echo "==> Embedding vellum binary ($VELLUM_BIN)"
        cp "$VELLUM_BIN" "$ROOT/usr/local/bin/vellum"
        chmod 755 "$ROOT/usr/local/bin/vellum"
    else
        echo "!! VELLUM_BIN unset — image will fall back to stock bsdinstall"
    fi

    echo "==> Pointing rc.local at the vellum installer"
    if grep -q bsdinstall "$ROOT/etc/rc.local" 2>/dev/null; then
        sed -i '' 's|/usr/libexec/bsdinstall/startbsdinstall|/usr/local/bin/vellum-install|g' \
            "$ROOT/etc/rc.local"
    fi
    if ! grep -q vellum-install "$ROOT/etc/rc.local" 2>/dev/null; then
        echo '/usr/local/bin/vellum-install' >> "$ROOT/etc/rc.local"
    fi

    echo "==> Quiet boot: VELLUM splash, no loader menu (straight to the TUI)"
    # splash.png is generated from branding/vellum-logo.svg (scripts/gen-splash.py)
    mkdir -p "$ROOT/boot/images"
    cp "$REPO/image/splash.png" "$ROOT/boot/images/vellum-splash.png"
    {
        echo 'autoboot_delay="-1"'   # no menu, no countdown
        echo 'beastie_disable="YES"' # no FreeBSD logo/boot menu
        echo 'loader_logo="none"'    # no logo in the loader either
        echo 'boot_mute="YES"'       # hide kernel messages; vt shows the splash
        echo 'splash="/boot/images/vellum-splash.png"'  # replaces the FreeBSD logo
    } >> "$ROOT/boot/loader.conf"
    # silence "Starting foo..." rc chatter; first visible thing is the wizard
    grep -q rc_startmsgs "$ROOT/etc/rc.conf" \
        || echo 'rc_startmsgs="NO"' >> "$ROOT/etc/rc.conf"
}
