#!/bin/sh
# inject.sh — shared Vellum injection for remastered FreeBSD images.
# Sourced by build-image.sh / build-iso.sh; expects:
#   REPO       — path to the vellum checkout
#   VELLUM_BIN — optional prebuilt `vellum` binary to embed
#
# inject_vellum <root>  applies to a mounted/copied installer root:
#   - /usr/local/bin/vellum-install  console launcher (replaces bsdinstall)
#   - /usr/local/bin/vellum          prebuilt CLI (if VELLUM_BIN set)
#   - patched /etc/rc.local           runs our launcher via runconsoles
#   - loader.conf quiet boot + VELLUM splash
inject_vellum() {
    ROOT="$1"
    [ -d "$ROOT/etc" ] || { echo "!! $ROOT doesn't look like a FreeBSD root" >&2; return 1; }

    echo "==> Injecting vellum launcher into $ROOT"
    mkdir -p "$ROOT/usr/local/bin"
    cp "$REPO/image/vellum-install" "$ROOT/usr/local/bin/vellum-install"
    chmod 755 "$ROOT/usr/local/bin/vellum-install"

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
