#!/bin/sh
# polybar: Wi-Fi SSID, else "wired" if any non-loopback NIC has an address.
for w in $(ifconfig -l); do
    case "$w" in
        wlan*)
            ssid=$(ifconfig "$w" 2>/dev/null | awk '$1 == "ssid" {print $2; exit}')
            [ -n "$ssid" ] && { echo "NET $ssid"; exit 0; }
            ;;
    esac
done
if ifconfig -a inet 2>/dev/null | awk '$1 == "inet" && $2 != "127.0.0.1" {f = 1} END {exit !f}'; then
    echo "NET wired"
else
    echo "NET down"
fi
