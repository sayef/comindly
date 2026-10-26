"""Brand assets with text as outlines: python assets/src/build.py <font dir> [out dir]

The font dir holds the variable fonts SpaceGrotesk.ttf and Geist.ttf (SIL Open Font License)."""

import math
import shutil
import subprocess
import sys
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

FONTS = Path(sys.argv[1])
OUT = Path(sys.argv[2] if len(sys.argv) > 2 else "assets")
OUT.mkdir(parents=True, exist_ok=True)


def instance(name, weight):
    return instantiateVariableFont(TTFont(FONTS / name), {"wght": weight})


BOLD = instance("SpaceGrotesk.ttf", 700)
MEDIUM = instance("Geist.ttf", 500)

INK, PAPER = "#131315", "#F2F2F2"
THEMES = {
    "light": {"text": INK, "tagline": "#646466", "petals": ("#0F604A", "#1F9A74"), "back": PAPER},
    "dark": {"text": PAPER, "tagline": "#9A9A9C", "petals": ("#6FDBB9", "#C2F1E3"), "back": INK},
}
TAGLINE = "Your team’s engineering memory for coding agents"


def text_path(font, text, x, baseline, size, tracking=0.0):
    """SVG path data for `text` and its advance width, in user units."""
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    scale = size / font["head"].unitsPerEm
    pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:.1f}".rstrip("0").rstrip("."))
    cursor = x
    for ch in text:
        name = cmap[ord(ch)]
        glyphs[name].draw(TransformPen(pen, (scale, 0, 0, -scale, cursor, baseline)))
        cursor += glyphs[name].width * scale + tracking * size
    return pen.getCommands(), cursor - x - tracking * size


def blend(a, b, t):
    a, b = (tuple(int(c[i : i + 2], 16) for i in (1, 3, 5)) for c in (a, b))
    return "#" + "".join(f"{round(x + (y - x) * t):02X}" for x, y in zip(a, b))


def mark(theme, solid=False):
    """The petal ring around the graph glyph (adapted from Phosphor Icons "graph", MIT), on a 256 grid."""
    start, end = THEMES[theme]["petals"]
    petals = []
    for i in range(10):
        angle = i * 36
        cx = 128 + 102 * math.sin(math.radians(angle))
        cy = 128 - 102 * math.cos(math.radians(angle))
        colour = blend(start, end, i / 9)
        paint = f'fill="{colour}"' if solid else f'fill="none" stroke="{colour}" stroke-width="9"'
        petals.append(
            f'<ellipse cx="{cx:.1f}" cy="{cy:.1f}" rx="11" ry="22" transform="rotate({angle} {cx:.1f} {cy:.1f})" {paint}/>'
        )
    graph = (
        f'<g transform="translate(128 128) scale(0.56) translate(-128 -124)" fill="none" stroke="{THEMES[theme]["text"]}" '
        'stroke-linecap="round" stroke-linejoin="round" stroke-width="20">'
        '<line x1="177.23" y1="111.59" x2="150.77" y2="120.41"/><line x1="181.06" y1="169.27" x2="146.94" y2="142.73"/>'
        '<line x1="110.06" y1="143.94" x2="73.94" y2="176.06"/><line x1="118.25" y1="106.07" x2="105.75" y2="77.93"/>'
        '<circle cx="128" cy="128" r="24"/><circle cx="200" cy="104" r="24"/><circle cx="200" cy="184" r="24"/>'
        '<circle cx="56" cy="192" r="24"/><circle cx="96" cy="56" r="24"/></g>'
    )
    return "".join(petals) + graph


def lockup(x, top, size, theme):
    """The mark, then `comindly` in bold; `size` is the cap height of the lockup. Returns body and width."""
    scale = size / 256
    body = f'<g transform="translate({x} {top}) scale({scale:.4f})">{mark(theme)}</g>'
    text_size = size * 0.78
    # Optical centre of lowercase text on the mark: between the x-height's middle and the ascenders'.
    baseline = top + size * 0.5 + text_size * 0.30
    name, name_w = text_path(BOLD, "comindly", x + size * 1.12, baseline, text_size, -0.035)
    body += f'<path d="{name}" fill="{THEMES[theme]["text"]}"/>'
    return body, size * 1.12 + name_w


def svg(width, height, body, label):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:g} {height:g}" width="{width:g}" height="{height:g}" '
        f'role="img" aria-label="{label}">\n{body}\n</svg>\n'
    )


if __name__ == "__main__":
    (OUT / "logo.svg").write_text(svg(256, 256, mark("light"), "comindly"))
    (OUT / "logo-dark.svg").write_text(svg(256, 256, mark("dark"), "comindly"))
    # The app icon and favicon: solid petals on a dark tile read at 16 px.
    tile = f'<rect width="256" height="256" rx="58" fill="{INK}"/><g transform="translate(128 128) scale(0.84) translate(-128 -128)">{mark("dark", solid=True)}</g>'
    (OUT / "icon.svg").write_text(svg(256, 256, tile, "comindly"))

    for theme, suffix in (("light", ""), ("dark", "-dark")):
        body, w = lockup(8, 8, 96, theme)
        (OUT / f"wordmark{suffix}.svg").write_text(svg(round(w + 20), 112, body, "comindly"))

        _, w = lockup(0, 0, 96, theme)
        _, tag_w = text_path(MEDIUM, TAGLINE, 0, 0, 25)
        width = round(max(w, tag_w) + 56)
        body, _ = lockup((width - w) / 2, 22, 96, theme)
        tag, _ = text_path(MEDIUM, TAGLINE, (width - tag_w) / 2, 160, 25)
        # A background of its own keeps the banner legible where the page's theme differs from it.
        panel = f'<rect width="{width}" height="184" rx="16" fill="{THEMES[theme]["back"]}"/>'
        (OUT / f"banner-{theme}.svg").write_text(
            svg(width, 184, f'{panel}\n{body}\n<path d="{tag}" fill="{THEMES[theme]["tagline"]}"/>', f"comindly: {TAGLINE[0].lower()}{TAGLINE[1:]}")
        )

    body, w = lockup(0, 0, 150, "dark")
    tag, tag_w = text_path(MEDIUM, TAGLINE, 0, 0, 40)
    points, points_w = text_path(MEDIUM, "call graph · service contracts · team ledger · MCP · your own storage", 0, 0, 25)
    social = f"""<rect width="1280" height="640" fill="{INK}"/>
<g transform="translate({(1280 - w) / 2:.1f} 190)">{body}</g>
<g transform="translate({(1280 - tag_w) / 2:.1f} 420)"><path d="{tag}" fill="#9A9A9C"/></g>
<g transform="translate({(1280 - points_w) / 2:.1f} 478)"><path d="{points}" fill="#8BE3C9"/></g>"""
    (OUT / "social-preview.svg").write_text(svg(1280, 640, social, "comindly"))

    # PNGs for places that take no SVG (GitHub social preview, app bundles); ImageMagick's own renderer drops the mark, so resvg first.
    resvg, magick = shutil.which("resvg"), shutil.which("magick")
    if resvg or magick:
        png = OUT / "png"
        png.mkdir(exist_ok=True)
        for size in (16, 32, 48, 64, 128, 256, 512):
            src, dst = str(OUT / "icon.svg"), str(png / f"icon-{size}.png")
            subprocess.run([resvg, "-w", str(size), "-h", str(size), src, dst] if resvg else [magick, "-background", "none", "-density", "300", src, "-resize", f"{size}x{size}", dst], check=True)
        src, dst = str(OUT / "social-preview.svg"), str(OUT / "social-preview.png")
        subprocess.run([resvg, src, dst] if resvg else [magick, "-density", "96", src, dst], check=True)

    # The docs site and the web UI serve their own copies.
    for target, names in (
        (Path("docs/assets"), ("logo.svg", "logo-dark.svg", "icon.svg", "wordmark.svg", "wordmark-dark.svg", "banner-light.svg", "banner-dark.svg")),
        (Path("ui/public"), ("logo.svg", "logo-dark.svg", "icon.svg")),
    ):
        if target.is_dir():
            for name in names:
                (target / name).write_text((OUT / name).read_text())
    print("written to", OUT)
