#!/usr/bin/env python3
"""Generate the VELLUM icon set from branding/vellum-mark.svg.

Outputs:
  branding/icons/vellum-<size>.png        default accent, 16..256px
  themes/<name>/icon.png                  per-theme accent (128px) — reached
                                          via ~/.config/vellum/current/theme

Rasterizes via qlmanage (macOS Quick Look): the mark uses cubic curves,
so this is a dev-side macOS step. Run on FreeBSD? Swap qlmanage for
rsvg-convert/inkscape in render().
Regenerate: python3 scripts/gen-icons.py
"""
import re
import subprocess
import tempfile
from pathlib import Path

MARK = Path("branding/vellum-mark.svg")
SIZES = (16, 24, 32, 48, 64, 128, 256)
DEFAULT = "#7aa2f7"  # tokyo-night accent


def tinted(rgb_hex):
    """Mark SVG with every fill recoloured."""
    return re.sub(r'fill="[^"]*"', f'fill="{rgb_hex}"', MARK.read_text())


def render(size, rgb_hex, out):
    with tempfile.TemporaryDirectory() as td:
        src = Path(td) / "mark.svg"
        src.write_text(tinted(rgb_hex))
        subprocess.run(
            ["qlmanage", "-t", "-s", str(size), str(src), "-o", td],
            check=True, capture_output=True)
        Path(str(src) + ".png").rename(out)


def main():
    out_dir = Path("branding/icons")
    out_dir.mkdir(exist_ok=True)
    for s in SIZES:
        render(s, DEFAULT, str(out_dir / f"vellum-{s}.png"))
        print(f"  {out_dir}/vellum-{s}.png")

    # one 128px icon per theme, tinted by that theme's focused accent
    for theme in sorted(Path("themes").iterdir()):
        wm = theme / "wm.conf"
        if not wm.is_file():
            continue
        m = re.search(r"focused\s*=\s*#([0-9a-fA-F]{6})", wm.read_text())
        if not m:
            continue
        render(128, f"#{m.group(1)}", str(theme / "icon.png"))
        print(f"  {theme}/icon.png")


if __name__ == "__main__":
    main()
