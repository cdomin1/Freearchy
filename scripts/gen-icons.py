#!/usr/bin/env python3
"""Generate the VELLUM "Split V" icon set from branding/vellum-mark.svg.

Outputs:
  branding/icons/vellum-<size>.png        default accent, 16..256px
  themes/<name>/icon.png                  per-theme accent (128px) — reached
                                          via ~/.config/vellum/current/theme
Regenerate: python3 scripts/gen-icons.py
"""
import importlib.util
import re
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "gen_splash", Path(__file__).parent / "gen-splash.py")
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)

MARK = "branding/vellum-mark.svg"
SIZES = (16, 24, 32, 48, 64, 128, 256)
DEFAULT = (122, 162, 247)  # tokyo-night #7aa2f7


def render(size, rgb, out):
    g.W = g.H = size
    g.FG = rgb
    g.MARGIN_X = g.MARGIN_Y = 0.10   # breathing room for app-icon use
    g.main(MARK, out)


def main():
    out_dir = Path("branding/icons")
    out_dir.mkdir(exist_ok=True)
    for s in SIZES:
        render(s, DEFAULT, str(out_dir / f"vellum-{s}.png"))

    # one 128px icon per theme, tinted by that theme's focused accent
    for theme in sorted(Path("themes").iterdir()):
        wm = theme / "wm.conf"
        if not wm.is_file():
            continue
        m = re.search(r"focused\s*=\s*#([0-9a-fA-F]{6})", wm.read_text())
        if not m:
            continue
        rgb = tuple(int(m.group(1)[i:i + 2], 16) for i in (0, 2, 4))
        render(128, rgb, str(theme / "icon.png"))


if __name__ == "__main__":
    main()
