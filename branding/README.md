# VELLUM brand assets

| File | Role |
| --- | --- |
| `vellum-mark.svg` | Primary mark — "Split V", solid fill. Icons, favicon, rofi, boot splash. |
| `vellum-mark-outline.svg` | Alternate mark — same geometry, stroke only. Use on light backgrounds, watermarks, print, one-colour contexts where the solid reads too heavy. |
| `vellum-logo.svg` | Wordmark — pairs with the mark; used alone in README/docs headers. |
| `vellum-splash.svg` | Mark + wordmark composite; source for `image/splash.png`. |
| `icons/` | Rendered PNG set (16–256 px), default accent `#7aa2f7`. |

Theme-aware variants live in `themes/<name>/icon.png` and are reachable at
runtime via `~/.config/vellum/current/theme/icon.png`.

Regenerate after editing the mark (dev-side macOS — uses `qlmanage` to
rasterize; swap for `rsvg-convert`/`inkscape` elsewhere):

```sh
scripts/gen-icons.py    # icons/ + themes/*/icon.png
scripts/gen-splash.py   # image/splash.png
```

The accent colour follows the theme (`wm.conf`), so the mark is never
hard-wired to a single palette.
