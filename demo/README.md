# VELLUM live demo backend

Streams a real VELLUM VM to the site's "Try it" page, distrosea-style:

```
browser ──WSS──> Caddy :443 ──> websockify :6080 ──VNC──> qemu :5900
```

- qemu boots `vellum-15.1-amd64-vm-installer.iso` with `-snapshot`
  (pristine every restart), built-in VNC on localhost only
- websockify bridges VNC→WebSocket and serves the noVNC UI
- Caddy terminates HTTPS automatically (Let's Encrypt)

## Provision a Debian/Ubuntu x86 VPS

```sh
curl -O https://raw.githubusercontent.com/cdomin1/Vellum-BSD/main/demo/setup.sh
HOSTNAME=<vps-ip>.sslip.io sh setup.sh    # or your own domain
```

Then set `DEMO_URL` in `site/try.html` to the URL it prints.

## Notes

- **View-only is UI-level** — QEMU's VNC server can't enforce read-only;
  the noVNC page sends no input. The VM is disposable (`-snapshot`), so
  worst case someone pokes at the installer and the VM gets restarted.
- **KVM required for decent speed** — check `ls /dev/kvm` on the VPS;
  most x86 VPSs expose it. TCG fallback works but is slow.
- **Desktop mode later**: once the ISO's install path is validated, add a
  pre-installed `vellum.qcow2` (autologin → `startx`) so visitors see the
  actual desktop instead of the installer TUI.
- Restart the stream: `systemctl restart vellum-demo-vm` (fresh ISO boot).
