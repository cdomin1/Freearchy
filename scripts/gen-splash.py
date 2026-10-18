#!/usr/bin/env python3
"""Generate image/splash.png — the VELLUM boot splash.

Rasterizes the real wordmark from branding/vellum-logo.svg (orthogonal
M/v/h/z polygons) into a 32-bit RGBA PNG. The FreeBSD loader hands it to
vt(4) via loader.conf `splash=` + `boot_mute=YES`; vt centers it on the
framebuffer and requires 4 bytes/pixel (si_depth == 4).
Regenerate: python3 scripts/gen-splash.py
"""
import re, struct, sys, zlib

W, H = 560, 120         # tight canvas; vt centers it on any resolution
FG = (122, 162, 247)    # #7aa2f7 tokyo-night accent (bg is transparent)
MARGIN_Y = 0.0


def polygons(svg):
    """path d strings → polygon vertex lists (M/v/h/z only)."""
    polys = []
    for d in re.findall(r'<path d="([^"]+)"', svg):
        toks = re.findall(r'[a-zA-Z]|-?\d+\.?\d*', d)
        pts, x, y, i = [], 0.0, 0.0, 0
        while i < len(toks):
            cmd = toks[i]; i += 1
            if cmd == 'M':
                x, y = float(toks[i]), float(toks[i + 1]); i += 2
                pts.append((x, y))
            elif cmd == 'v':
                y += float(toks[i]); i += 1
                pts.append((x, y))
            elif cmd == 'h':
                x += float(toks[i]); i += 1
                pts.append((x, y))
            elif cmd == 'V':
                y = float(toks[i]); i += 1
                pts.append((x, y))
            elif cmd == 'H':
                x = float(toks[i]); i += 1
                pts.append((x, y))
            elif cmd.lower() == 'z':
                pass
        polys.append(pts)
    return polys


def main(src="branding/vellum-logo.svg", out="image/splash.png"):
    polys = polygons(open(src).read())
    xs = [p[0] for poly in polys for p in poly]
    ys = [p[1] for poly in polys for p in poly]
    sw, sh = max(xs) - min(xs), max(ys) - min(ys)
    scale = min(W / sw, H * (1 - 2 * MARGIN_Y) / sh)
    ox = (W - sw * scale) / 2 - min(xs) * scale
    oy = (H - sh * scale) / 2 - min(ys) * scale
    polys = [[(x * scale + ox, y * scale + oy) for x, y in poly]
             for poly in polys]

    buf = bytearray(W * H)          # 0 = bg, 1 = fg
    for y in range(H):
        for x in range(W):
            for poly in polys:
                inside = False
                j = len(poly) - 1
                for i in range(len(poly)):
                    xi, yi = poly[i]; xj, yj = poly[j]
                    if (yi > y) != (yj > y) and \
                       x < (xj - xi) * (y - yi) / (yj - yi) + xi:
                        inside = not inside
                    j = i
                if inside:
                    buf[y * W + x] = 1
                    break

    # RGBA8 PNG, non-interlaced (what the loader's pnglite decodes)
    on, off = bytes((*FG, 255)), bytes(4)
    raw = b"".join(b"\x00" + b"".join(on if buf[y * W + x] else off
                                      for x in range(W))
                   for y in range(H))

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data +
                struct.pack(">I", zlib.crc32(tag + data) & 0xffffffff))

    png = (b"\x89PNG\r\n\x1a\n" +
           chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0)) +
           chunk(b"IDAT", zlib.compress(raw, 9)) +
           chunk(b"IEND", b""))
    with open(out, "wb") as f:
        f.write(png)
    print(f"wrote {out} ({len(png)} bytes, {W}x{H} RGBA, "
          f"{sum(buf)} lit px, {len(polys)} letters)")


if __name__ == "__main__":
    main(*sys.argv[1:])
