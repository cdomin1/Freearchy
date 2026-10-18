#!/bin/sh
# polybar: master volume via mixer(8). FreeBSD 14+ prints normalized
# values (`vol.volume=0.75:0.75`), so scale the left channel to percent.
vol=$(mixer -o vol.volume 2>/dev/null | sed -n 's/.*=\([0-9.]*\).*/\1/p')
[ -n "$vol" ] || { echo "VOL --"; exit 0; }
mute=$(mixer -o vol.mute 2>/dev/null | sed -n 's/.*=//p')
if [ "$mute" = "1" ]; then
    echo "VOL muted"
else
    awk -v v="$vol" 'BEGIN { printf "VOL %d%%\n", v * 100 + 0.5 }'
fi
