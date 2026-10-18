#!/usr/bin/env python3
"""Generate image/splash.png — the VELLUM boot splash.

Rasterizes branding/vellum-splash.svg (mark + wordmark) via qlmanage.
The FreeBSD loader hands the PNG to vt(4) via loader.conf `splash=` +
`boot_mute=YES`; vt centers it on the framebuffer. Dev-side macOS step —
on FreeBSD swap qlmanage for rsvg-convert/inkscape.
Regenerate: python3 scripts/gen-splash.py
"""
import subprocess
import sys
import tempfile
from pathlib import Path

SRC = "branding/vellum-splash.svg"
OUT = "image/splash.png"
WIDTH = 560  # px; vt scales/centres on any resolution


def main(src=SRC, out=OUT, width=WIDTH):
    with tempfile.TemporaryDirectory() as td:
        subprocess.run(
            ["qlmanage", "-t", "-s", str(width), src, "-o", td],
            check=True, capture_output=True)
        Path(td, Path(src).name + ".png").rename(out)
    print(f"wrote {out}")


if __name__ == "__main__":
    main(*sys.argv[1:])
