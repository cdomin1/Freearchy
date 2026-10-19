#!/bin/sh
# Vellum "try it in your browser" demo backend — Debian/Ubuntu x86 VPS.
#
# Installs: qemu-kvm + websockify + noVNC + Caddy (auto-HTTPS), fetches
# the amd64 ISO, and sets up:
#   vellum-demo-vm.service  qemu -snapshot, built-in VNC on 127.0.0.1:5900
#   vellum-demo-ws.service  websockify 127.0.0.1:6080 -> :5900 (+ noVNC UI)
#   caddy                   HTTPS :443 -> :6080 (auto Let's Encrypt)
#
# Usage:  HOSTNAME=demo.example.com sh demo/setup.sh
#         (no domain? HOSTNAME=<vps-ip>.sslip.io works too — free DNS)
set -eu

RELEASE_TAG="${RELEASE_TAG:-v0.2.0-alpha}"
ISO="vellum-15.1-amd64-vm-installer.iso"

if [ "$(id -u)" -ne 0 ]; then echo "run as root" >&2; exit 1; fi
HOSTNAME="${HOSTNAME:-}"
if [ -z "$HOSTNAME" ]; then
    HOSTNAME="$(curl -s4 https://api.ipify.org).sslip.io"
    echo "no HOSTNAME given — using $HOSTNAME"
fi

echo "==> packages"
apt-get update -qq
apt-get install -y -qq qemu-system-x86 qemu-utils novnc websockify caddy curl xz-utils

echo "==> fetching $ISO"
mkdir -p /opt/vellum-demo
cd /opt/vellum-demo
if [ ! -f "$ISO" ]; then
    curl -fL -o "$ISO.xz" \
        "https://github.com/cdomin1/vellum/releases/download/${RELEASE_TAG}/${ISO}.xz"
    xz -d "$ISO.xz"
fi

cat > /opt/vellum-demo/vm-run.sh <<'EOF'
#!/bin/sh
# -snapshot: writes go to a throwaway overlay — the VM is pristine on
# every restart. VNC binds localhost only; websockify fronts it.
if [ -w /dev/kvm ]; then ACCEL="-enable-kvm -cpu host"; else ACCEL=""; fi
# shellcheck disable=SC2086
exec qemu-system-x86_64 $ACCEL \
    -smp 2 -m 2048 -machine q35 \
    -device virtio-net-pci,netdev=n0 -netdev user,id=n0 \
    -drive file=/opt/vellum-demo/vellum-15.1-amd64-vm-installer.iso,media=cdrom,readonly=on \
    -boot order=d \
    -snapshot \
    -vnc 127.0.0.1:0 \
    -display none
EOF
chmod +x /opt/vellum-demo/vm-run.sh

cat > /etc/systemd/system/vellum-demo-vm.service <<'EOF'
[Unit]
Description=Vellum demo VM (qemu, snapshot mode)
After=network.target

[Service]
ExecStart=/opt/vellum-demo/vm-run.sh
Restart=always
RestartSec=5
OOMScoreAdjust=-500

[Install]
WantedBy=multi-user.target
EOF

cat > /etc/systemd/system/vellum-demo-ws.service <<'EOF'
[Unit]
Description=noVNC websockify bridge for the Vellum demo
After=vellum-demo-vm.service

[Service]
ExecStart=/usr/bin/websockify --web=/usr/share/novnc 127.0.0.1:6080 127.0.0.1:5900
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF

cat > /etc/caddy/Caddyfile <<EOF
$HOSTNAME

reverse_proxy 127.0.0.1:6080
EOF

systemctl daemon-reload
systemctl enable --now caddy vellum-demo-ws
systemctl restart vellum-demo-vm

cat <<DONE

==> demo will be live at:
    https://$HOSTNAME/vnc.html?autoconnect=true&view_only=true&resize=scale
    set DEMO_URL in site/try.html to that URL.
DONE
