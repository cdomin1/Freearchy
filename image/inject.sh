#!/bin/sh
# inject.sh — shared Vellum injection for remastered FreeBSD images.
# Sourced by build-image.sh / build-iso.sh; expects:
#   REPO       — path to the vellum checkout
#   ARCH       — amd64 | aarch64 (defaults to build host arch)
#   RELEASE    — FreeBSD release, e.g. 15.1 (defaults to 15.1)
#   VELLUM_BIN — optional prebuilt `vellum` binary to embed
#
# inject_vellum <root>  applies to a mounted/copied installer root:
#   - /usr/local/bin/vellum-install  console launcher (replaces bsdinstall)
#   - /usr/local/bin/vellum-setup    inline gum wizard (config → engine)
#   - /usr/local/bin/vellum          prebuilt CLI (if VELLUM_BIN set)
#   - /usr/local/bin/gum             static Charm gum binary for the arch
#   - /usr/local/vellum              repo payload (offline installs)
#   - /usr/freebsd-dist/*.txz        dist sets when they fit (offline install)
#   - patched /etc/rc.local           runs our launcher via runconsoles
#   - loader.conf quiet boot + VELLUM splash + Tokyo Night vt palette
inject_vellum() {
    ROOT="$1"
    [ -d "$ROOT/etc" ] || { echo "!! $ROOT doesn't look like a FreeBSD root" >&2; return 1; }
    RELEASE="${RELEASE:-15.1}"

    echo "==> Injecting vellum installer into $ROOT"
    mkdir -p "$ROOT/usr/local/bin"
    cp "$REPO/image/vellum-install" "$ROOT/usr/local/bin/vellum-install"
    cp "$REPO/image/vellum-setup" "$ROOT/usr/local/bin/vellum-setup"
    chmod 755 "$ROOT/usr/local/bin/vellum-install" "$ROOT/usr/local/bin/vellum-setup"

    # Static gum for the wizard's inline prompts. Pinned to 0.17: bubbletea
    # v1 — the 2.x series emits kitty-keyboard escapes that FreeBSD's vt(4)
    # renders as literal "1u" garbage.
    GUM_VER=0.17.0
    case "${ARCH:-$(uname -m)}" in
        amd64|x86_64)  GUM_ARCH=x86_64; FBSD_ARCH=amd64/amd64 ;;
        arm64|aarch64) GUM_ARCH=arm64;  FBSD_ARCH=arm64/aarch64 ;;
        *) GUM_ARCH=""; FBSD_ARCH="" ;;
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

    # Repo payload on the media → the engine copies it instead of fetching
    # GitHub mid-install (repo_root finds /usr/local/vellum via themes/).
    echo "==> Embedding vellum payload"
    mkdir -p "$ROOT/usr/local/vellum"
    if git -C "$REPO" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        git -C "$REPO" archive HEAD | tar -xf - -C "$ROOT/usr/local/vellum"
    else
        tar -cf - --exclude target --exclude .git -C "$REPO" . | tar -xf - -C "$ROOT/usr/local/vellum"
    fi

    # Dist sets on the media → fully offline base install. Best-effort:
    # the memstick UFS may not have the ~200MB spare — engine falls back
    # to download.freebsd.org when they're absent.
    if [ -n "$FBSD_ARCH" ]; then
        mkdir -p "$ROOT/usr/freebsd-dist"
        for set in kernel.txz base.txz MANIFEST; do
            if [ ! -f "$ROOT/usr/freebsd-dist/$set" ]; then
                fetch -qo "$ROOT/usr/freebsd-dist/$set" \
                    "https://download.freebsd.org/releases/$FBSD_ARCH/${RELEASE}-RELEASE/$set" \
                    || echo "!! dist fetch $set failed — installer will download at runtime" >&2
            fi
        done
        if [ -f "$ROOT/usr/freebsd-dist/kernel.txz" ] && [ -f "$ROOT/usr/freebsd-dist/base.txz" ]; then
            echo "==> Embedded FreeBSD dist sets (offline install)"
        fi
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

    echo "==> Quiet boot: VELLUM splash, no loader menu, themed console"
    # splash.png is generated from branding/vellum-logo.svg (scripts/gen-splash.py)
    mkdir -p "$ROOT/boot/images"
    cp "$REPO/image/splash.png" "$ROOT/boot/images/vellum-splash.png"
    {
        echo 'autoboot_delay="-1"'   # no menu, no countdown
        echo 'beastie_disable="YES"' # no FreeBSD logo/boot menu
        echo 'loader_logo="none"'    # no logo in the loader either
        echo 'boot_mute="YES"'       # hide kernel messages; vt shows the splash
        echo 'splash="/boot/images/vellum-splash.png"'  # replaces the FreeBSD logo
        # Tokyo Night palette for vt(4) — kern.vt.color.N.rgb is the
        # supported mechanism (teken can't parse OSC palette escapes).
        # gum's TERM=screen-16color ANSI indices render through these.
        echo 'kern.vt.color.0.rgb="26,27,38"'    # bg
        echo 'kern.vt.color.1.rgb="247,118,142"' # red
        echo 'kern.vt.color.2.rgb="158,206,106"' # green
        echo 'kern.vt.color.3.rgb="224,175,104"' # yellow
        echo 'kern.vt.color.4.rgb="122,162,247"' # accent blue
        echo 'kern.vt.color.5.rgb="187,154,247"' # magenta
        echo 'kern.vt.color.6.rgb="125,207,255"' # cyan
        echo 'kern.vt.color.7.rgb="169,177,214"' # fg
        echo 'kern.vt.color.8.rgb="65,72,104"'   # dim
        echo 'kern.vt.color.9.rgb="255,122,147"'
        echo 'kern.vt.color.10.rgb="159,224,68"'
        echo 'kern.vt.color.11.rgb="250,186,74"'
        echo 'kern.vt.color.12.rgb="141,176,255"' # bright accent
        echo 'kern.vt.color.13.rgb="199,169,255"'
        echo 'kern.vt.color.14.rgb="164,218,255"'
        echo 'kern.vt.color.15.rgb="192,202,245"' # bright fg
    } >> "$ROOT/boot/loader.conf"
    # silence "Starting foo..." rc chatter; first visible thing is the wizard
    grep -q rc_startmsgs "$ROOT/etc/rc.conf" \
        || echo 'rc_startmsgs="NO"' >> "$ROOT/etc/rc.conf"
}
