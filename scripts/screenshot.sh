#!/bin/sh
# Stage a VELLUM session on Xvfb and capture screenshots to ~/shots/.
# Runs inside a real FreeBSD (CI vmactions VM, or a local test VM).
# Requires: repo cloned at $VELLUM_REPO (default ~/vellum), root or doas.
set -e

REPO=${VELLUM_REPO:-$HOME/vellum}
SHOTS=$HOME/shots
D=:99
THEME=${VELLUM_SHOT_THEME:-tokyo-night}

if [ "$(id -u)" -eq 0 ]; then SUDO=""; else SUDO=doas; fi

$SUDO pkg install -y rust xorg-vfbserver xset xrandr xmodmap xinit xauth \
    polybar alacritty rofi feh maim dejavu xterm fastfetch btop xdotool

cd "$REPO"
cargo build --release -p vellum -p vellum-wm
export PATH="$REPO/target/release:$PATH"

# ---- configs (same layout `vellum install` links) ----
mkdir -p "$HOME/.config/vellum/current" "$HOME/.local/share/vellum" "$SHOTS"
ln -sfn "$REPO/config/x11"         "$HOME/.config/vellum/x11"
ln -sfn "$REPO/config/x11/polybar"    "$HOME/.config/polybar"
ln -sfn "$REPO/config/x11/alacritty"  "$HOME/.config/alacritty"
ln -sfn "$REPO/config/x11/rofi"       "$HOME/.config/rofi"
ln -sfn "$REPO/config/x11/gtk-3.0"    "$HOME/.config/gtk-3.0"
ln -sfn "$REPO/themes/$THEME"      "$HOME/.config/vellum/current/theme"

WALL=$(ls "$REPO/themes/$THEME"/wallpaper.* 2>/dev/null | head -1)

# ---- session ----
Xvfb $D -screen 0 1600x900x24 &
sleep 2
export DISPLAY=$D
export LIBGL_ALWAYS_SOFTWARE=1   # alacritty under Xvfb → llvmpipe

feh --bg-fill ${WALL:-"$REPO/themes/tokyo-night/wallpaper.jpg"} 2>/dev/null \
    || xsetroot -solid '#1a1b26'
xsetroot -cursor_name left_ptr 2>/dev/null

# picom skipped: VFB has no GLX worth compositing; shadows add nothing here

vellum-wm &
sleep 3
polybar -c "$HOME/.config/polybar/config.ini" vellum &
sleep 3

shot() { maim "$SHOTS/$1.png"; echo "captured $1"; }

spawn_term() {
    alacritty -e "$@" 2>/dev/null & TPID=$!
    sleep 3
    kill -0 $TPID 2>/dev/null || { xterm -e "$@" & sleep 2; }
}

# shot 1 — clean desktop: bar + wallpaper
shot 01-desktop

# shot 2 — tiled terminals: fastfetch + btop (shows gaps/borders/tiling)
spawn_term fastfetch
spawn_term btop
sleep 3
shot 02-tiled

# shot 3 — rofi launcher overlay
rofi -show drun -modi drun &
sleep 2
shot 03-launcher
kill %rofi 2>/dev/null; killall rofi 2>/dev/null || true
sleep 1

# shot 4 — second theme applied live (polybar + wallpaper swap)
ALT=${VELLUM_SHOT_ALT:-gruvbox}
if [ -d "$REPO/themes/$ALT" ]; then
    ln -sfn "$REPO/themes/$ALT" "$HOME/.config/vellum/current/theme"
    ALTWALL=$(ls "$REPO/themes/$ALT"/wallpaper.* 2>/dev/null | head -1)
    [ -n "$ALTWALL" ] && feh --bg-fill "$ALTWALL"
    killall polybar; sleep 1
    polybar -c "$HOME/.config/polybar/config.ini" vellum &
    sleep 3
    shot 04-theme-$ALT
fi

killall vellum-wm polybar Xvfb 2>/dev/null || true
echo "== shots in $SHOTS:"
ls -la "$SHOTS"
