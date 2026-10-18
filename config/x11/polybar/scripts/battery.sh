#!/bin/sh
# polybar: battery percent from acpiconf(8); "+" while charging.
# Prints nothing on machines without a battery (desktops, VMs).
acpiconf -i0 2>/dev/null | awk -F':[ \t]*' '
    $1 == "Remaining capacity" { pct = $2 }
    $1 == "State"              { state = $2 }
    END {
        if (pct == "" || pct == "unknown") exit
        printf "BAT %s%s\n", (state ~ /charging/ && state !~ /dis/) ? "+" : "", pct
    }'
