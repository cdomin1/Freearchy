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

# ---------------------------------------------------------------------------
# inject_live <root> — live-desktop variant (used for the VM ISO).
# Everything inject_vellum does (payload, gum, dist sets, quiet boot) plus a
# bootable desktop: package manifests installed into the image, a live user,
# ttyv0 autologin → startx → vellum-wm, and the installer wizard running in an
# Alacritty window (gum behaves perfectly under a real terminal — none of the
# bare-vt weirdness).
#   WM_BIN — optional prebuilt vellum-wm binary
inject_live() {
    ROOT="$1"
    inject_vellum "$ROOT" || return 1
    live_packages "$ROOT"
    live_user "$ROOT"
    live_boot "$ROOT"
    if [ -n "$WM_BIN" ] && [ -f "$WM_BIN" ]; then
        echo "==> Embedding vellum-wm binary"
        cp "$WM_BIN" "$ROOT/usr/local/bin/vellum-wm"
        chmod 755 "$ROOT/usr/local/bin/vellum-wm"
    else
        echo "!! WM_BIN unset — live desktop will have no window manager" >&2
    fi
}

# Package manifests → pkg -r into the image root (chroot-less install).
# Same parse as install.rs: '#' comments, trailing '?' = optional.
live_packages() {
    ROOT="$1"
    echo "==> Installing desktop packages into live image"
    # pkg -r chroots into ROOT for scriptlets — needs DNS + dev nodes.
    # Stock media symlinks resolv.conf into /tmp/bsdinstall_etc (writable
    # runtime area) — unlink before copying or cp writes through the link.
    if [ -f /etc/resolv.conf ]; then
        rm -f "$ROOT/etc/resolv.conf"
        cp /etc/resolv.conf "$ROOT/etc/resolv.conf"
    fi
    mkdir -p "$ROOT/dev"
    mount -t devfs devfs "$ROOT/dev" 2>/dev/null

    REQ=""; OPT=""
    for m in base xorg shell desktop; do
        MF="$REPO/packages/$m.txt"
        [ -f "$MF" ] || continue
        while IFS= read -r _line; do
            _p=$(printf '%s' "$_line" | sed 's/#.*//' | tr -d '[:space:]')
            [ -z "$_p" ] && continue
            case "$_p" in
                *\?) OPT="$OPT ${_p%\?}" ;;
                *)   REQ="$REQ $_p" ;;
            esac
        done < "$MF"
    done

    # shellcheck disable=SC2086
    env ASSUME_ALWAYS_YES=yes pkg -r "$ROOT" update 2>/dev/null
    env ASSUME_ALWAYS_YES=yes pkg -r "$ROOT" install -y $REQ \
        || echo "!! some required packages failed to install" >&2
    for _p in $OPT; do
        env ASSUME_ALWAYS_YES=yes pkg -r "$ROOT" install -y "$_p" >/dev/null 2>&1 \
            || echo "!! optional package $_p unavailable — skipping" >&2
    done
    # downloaded .pkg blobs in var/cache/pkg are dead weight on the image
    env ASSUME_ALWAYS_YES=yes pkg -r "$ROOT" clean -a 2>/dev/null
    umount "$ROOT/dev" 2>/dev/null
    rm -f "$ROOT/etc/resolv.conf"
    # restore the stock symlink — live runtime resolv.conf lands in
    # /tmp/bsdinstall_etc via dhclient/resolvconf (created in rc.local)
    ln -sf /tmp/bsdinstall_etc/resolv.conf "$ROOT/etc/resolv.conf"
}

# Live user 'vellum' (uid 1001) + passwd db regen on the image.
live_user() {
    ROOT="$1"
    echo "==> Creating live user 'vellum'"
    grep -q '^vellum:' "$ROOT/etc/master.passwd" 2>/dev/null || \
        echo 'vellum:*:1001:1001::0:0:Vellum Live:/usr/home/vellum:/bin/sh' \
            >> "$ROOT/etc/master.passwd"
    pwd_mkdb -d "$ROOT/etc" "$ROOT/etc/master.passwd"
    grep -q '^vellum:' "$ROOT/etc/group" 2>/dev/null \
        || echo 'vellum:*:1001:' >> "$ROOT/etc/group"
    # video group for GPU device access when KMS drivers exist
    sed -i '' 's|^video:\*:\([0-9]*\):$|video:*:\1:vellum|' "$ROOT/etc/group" 2>/dev/null
    mkdir -p "$ROOT/usr/home/vellum"
}

# ttyv0 autologin → live user's .profile → startx. skel lands on tmpfs at boot
# (image/vellum-live rc.d). doas nopass so the desktop can run the installer.
live_boot() {
    ROOT="$1"
    echo "==> Live boot: autologin ttyv0 → startx → desktop"

    # autologin getty entry + ttyv0 pointing at it. tc before al so Pc's
    # settings merge first and our al wins.
    grep -q 'vellum-auto' "$ROOT/etc/gettytab" 2>/dev/null || \
        printf 'vellum-auto:al=vellum:tc=Pc:\n' >> "$ROOT/etc/gettytab"
    sed -i '' 's|^ttyv0.*|ttyv0\t"/usr/libexec/getty vellum-auto"\txterm\tonifexists secure|' \
        "$ROOT/etc/ttys"

    # nopass doas for the live user — installer + power ops from the desktop
    mkdir -p "$ROOT/usr/local/etc"
    echo 'permit nopass vellum as root' > "$ROOT/usr/local/etc/doas.conf"

    # dbus for notifyd/GTK; seatd not needed (X has direct device access)
    grep -q dbus_enable "$ROOT/etc/rc.conf" 2>/dev/null \
        || echo 'dbus_enable="YES"' >> "$ROOT/etc/rc.conf"
    echo 'vellum_live_enable="YES"' >> "$ROOT/etc/rc.conf"

    # rc.d script: tmpfs home + skel seeding (ISO9660 root is read-only)
    cp "$REPO/image/vellum-live" "$ROOT/etc/rc.d/vellum_live"
    chmod 555 "$ROOT/etc/rc.d/vellum_live"

    # skel — seeded into the tmpfs home each boot
    SKEL="$ROOT/usr/local/share/vellum-skel"
    mkdir -p "$SKEL/.config/vellum" "$SKEL/.local/share/applications"
    # .profile: ttyv0 → startx once (fall back to shell if X fails twice)
    cat > "$SKEL/.profile" <<'EOF'
export PATH="$HOME/bin:/usr/local/bin:/usr/local/sbin:$PATH"
if [ -z "$DISPLAY" ] && [ "$(tty)" = "/dev/ttyv0" ] && [ ! -f "$HOME/.vellum-nox" ]; then
    startx || touch "$HOME/.vellum-nox"
    exit
fi
EOF
    # .xinitrc: canonical session + the installer window autostart
    awk '/^exec vellum-wm/ {
        print "( sleep 3 && alacritty -e doas /usr/local/bin/vellum-setup ) &"
        print ""
    } { print }' "$REPO/config/x11/xinitrc" > "$SKEL/.xinitrc"
    # config symlinks into the read-only payload on the media
    ln -sf /usr/local/vellum/config/x11           "$SKEL/.config/vellum/x11"
    ln -sf /usr/local/vellum/config/x11/rofi      "$SKEL/.config/rofi"
    ln -sf /usr/local/vellum/config/x11/alacritty "$SKEL/.config/alacritty"
    ln -sf /usr/local/vellum/config/x11/polybar   "$SKEL/.config/polybar"
    ln -sf /usr/local/vellum/config/x11/gtk-3.0   "$SKEL/.config/gtk-3.0"
    ln -sf /usr/local/vellum/config/x11/gtkrc-2.0 "$SKEL/.gtkrc-2.0"
    ln -sf /usr/local/vellum/config/x11/applications/vellum-menu.desktop \
        "$SKEL/.local/share/applications/vellum-menu.desktop"

    # relaunchable installer entry (rofi drun / any desktop menu)
    cat > "$SKEL/.local/share/applications/vellum-install.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Install VELLUM
Comment=Install VELLUM to this disk
Exec=alacritty -e doas /usr/local/bin/vellum-setup
Icon=drive-harddisk
Terminal=false
Categories=System;Settings;
EOF

    # rc.local: unattended config still wins — a vellum-install.conf drive
    # runs the console engine before X starts; otherwise the desktop boots.
    cat > "$ROOT/etc/rc.local" <<'EOF'
#!/bin/sh
# Vellum live ISO — unattended install takes precedence over the desktop.
mkdir -p /tmp/bsdinstall_etc

# DHCP on plausible NICs — desktop + installer both want it. -b backgrounds.
for i in $(ifconfig -l); do
    case "$i" in lo*|ipfw*|pflog*|pfsync*|wlan*) continue ;; esac
    ifconfig "$i" up 2>/dev/null
    dhclient -b "$i" >/dev/null 2>&1
done

for dev in $(sysctl -n kern.disks 2>/dev/null); do
    for part in "/dev/${dev}"*; do
        [ "$part" = "/dev/${dev}" ] && continue
        [ -c "$part" ] || continue
        fs=$(fstyp "$part" 2>/dev/null) || continue
        case "$fs" in ufs|msdosfs|cd9660|ext2fs|ntfs) ;; *) continue ;; esac
        mkdir -p /tmp/vmnt
        mount -r -t "$fs" "$part" /tmp/vmnt 2>/dev/null || continue
        if [ -f /tmp/vmnt/vellum-install.conf ]; then
            umount /tmp/vmnt
            exec /usr/local/bin/vellum-install
        fi
        umount /tmp/vmnt 2>/dev/null
    done
done
# no config → the ttyv0 autologin takes it from here (desktop + installer window)
EOF
    chmod 555 "$ROOT/etc/rc.local"
}
