# Brand assets

| File | Use |
|---|---|
| `logo.svg`, `logo-dark.svg` | The mark alone, for light and dark backgrounds |
| `icon.svg` | The mark on a dark tile: favicon and app icon |
| `wordmark.svg`, `wordmark-dark.svg` | The mark and the name, for light and dark backgrounds |
| `banner-light.svg`, `banner-dark.svg` | The name and tagline, as at the top of the README |
| `social-preview.svg`, `social-preview.png` | 1280 × 640 repository preview for GitHub and link cards |

The mark is a ring of ten solid petals around a graph of five ringed nodes, all in one gradient running
from deep to pale slate blue (`#1A3455` to `#2F5F9E` on light backgrounds, `#8DB4EA` to `#C9D7EA` on dark). The name follows the mark in bold. Slate blue is the one accent; links on light backgrounds
take `#2F5F9E`, which reads at body size. Text is converted to outlines from
[Space Grotesk](https://github.com/floriankarsten/space-grotesk) (the name) and
[Geist](https://github.com/vercel/geist-font) (the tagline), both under the SIL Open Font
License, so the files need no fonts. The graph is adapted from the [Phosphor Icons](https://phosphoricons.com)
`graph` glyph (MIT).

Regenerate everything with `python assets/src/build.py <font dir>` (needs `fonttools`, and resvg or ImageMagick
for the PNGs), where the directory holds the variable fonts `SpaceGrotesk.ttf` and `Geist.ttf`. It
also refreshes the copies in `docs/assets/` and `ui/public/`.
