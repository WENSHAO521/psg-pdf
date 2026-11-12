#!/usr/bin/env python3
"""Generate the PSG PDF and PSG Academic PDF app icons.

Original geometric artwork made for this repository (no fonts, no third-party graphics):
a rounded tile in the app colour, a sheet of paper with a folded corner, and a gold sun
rising over a horizon (the panorama) for Panorama Scholarly Group. The academic edition adds text lines and a
formula bracket. Licensed MIT OR Apache-2.0, like the repository.

Usage: python3 packaging/psg-icons.py   (needs Pillow)
Writes assets/app-icon/ and assets/app-icon-academic/.
"""

import os
from PIL import Image, ImageDraw

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

INK = (31, 58, 104)          # #1f3a68 PSG navy
PAPER = (247, 244, 236)      # #f7f4ec
FOLD = (214, 208, 194)       # #d6d0c2
GOLD = (217, 164, 65)        # #d9a441

EDITIONS = {
    "psg-pdf": {
        "dir": "assets/app-icon",
        "id": "com.panoramascholarly.psgpdf",
        "field": INK,
        "lines": 3,
        "formula": False,
        "hicolor": [16, 24, 32, 48, 64, 128, 256, 512],
    },
    "psg-academic-pdf": {
        "dir": "assets/app-icon-academic",
        "id": "com.panoramascholarly.psgacademicpdf",
        "field": (110, 32, 52),  # #6e2034 academic burgundy
        "lines": 2,
        "formula": True,
        "hicolor": [16, 32, 48, 64, 128, 256],
    },
}

# Geometry on a 512 x 512 grid.
TILE_R = 112
PAGE = (136, 88, 376, 424)   # left, top, right, bottom
FOLD_SIZE = 64


def hexc(c):
    return "#%02x%02x%02x" % c


def page_polygon():
    l, t, r, b = PAGE
    return [(l, t), (r - FOLD_SIZE, t), (r, t + FOLD_SIZE), (r, b), (l, b)]


def fold_polygon():
    l, t, r, b = PAGE
    return [(r - FOLD_SIZE, t), (r - FOLD_SIZE, t + FOLD_SIZE), (r, t + FOLD_SIZE)]


def text_lines(n):
    l, t, r, b = PAGE
    out = []
    y = t + 100
    for i in range(n):
        w = (r - l - 64) - (40 if i == n - 1 else 0)
        out.append((l + 32, y, l + 32 + w, y + 18))
        y += 40
    return out


def arc_box():
    # The panorama: a sun rising over the horizon (the top half of this circle).
    l, t, r, b = PAGE
    cx, cy = (l + r) / 2, b - 58
    return (cx - 58, cy - 58, cx + 58, cy + 58)


def horizon():
    l, t, r, b = PAGE
    return (l + 32, b - 58, r - 32, b - 46)


def formula(field):
    # A square bracket pair with a bar: [ — ], drawn as rectangles.
    l, t, r, b = PAGE
    x0, y0 = l + 35, t + 150
    rects = [
        (x0, y0, x0 + 10, y0 + 56), (x0, y0, x0 + 28, y0 + 10), (x0, y0 + 46, x0 + 28, y0 + 56),
        (x0 + 50, y0 + 23, x0 + 120, y0 + 33),
        (x0 + 160, y0, x0 + 170, y0 + 56), (x0 + 142, y0, x0 + 170, y0 + 10), (x0 + 142, y0 + 46, x0 + 170, y0 + 56),
    ]
    return rects


def draw(size, ed):
    s = 4  # supersampling
    big = 512 * s
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, big - 1, big - 1), radius=TILE_R * s, fill=ed["field"] + (255,))
    scale = lambda pts: [(x * s, y * s) for x, y in pts]
    d.polygon(scale(page_polygon()), fill=PAPER + (255,))
    d.polygon(scale(fold_polygon()), fill=FOLD + (255,))
    for (x0, y0, x1, y1) in text_lines(ed["lines"]):
        d.rounded_rectangle((x0 * s, y0 * s, x1 * s, y1 * s), radius=9 * s, fill=ed["field"] + (255,))
    if ed["formula"]:
        for (x0, y0, x1, y1) in formula(ed["field"]):
            d.rectangle((x0 * s, y0 * s, x1 * s, y1 * s), fill=GOLD + (255,))
    ax0, ay0, ax1, ay1 = arc_box()
    d.pieslice((ax0 * s, ay0 * s, ax1 * s, ay1 * s), start=180, end=360, fill=GOLD + (255,))
    hx0, hy0, hx1, hy1 = horizon()
    d.rounded_rectangle((hx0 * s, hy0 * s, hx1 * s, hy1 * s), radius=6 * s, fill=GOLD + (255,))
    return img.resize((size, size), Image.LANCZOS)


def svg(ed):
    parts = [
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">',
        f'<rect width="512" height="512" rx="{TILE_R}" fill="{hexc(ed["field"])}"/>',
        '<polygon points="%s" fill="%s"/>' % (" ".join(f"{x},{y}" for x, y in page_polygon()), hexc(PAPER)),
        '<polygon points="%s" fill="%s"/>' % (" ".join(f"{x},{y}" for x, y in fold_polygon()), hexc(FOLD)),
    ]
    for (x0, y0, x1, y1) in text_lines(ed["lines"]):
        parts.append(f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{y1 - y0}" rx="9" fill="{hexc(ed["field"])}"/>')
    if ed["formula"]:
        for (x0, y0, x1, y1) in formula(ed["field"]):
            parts.append(f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{y1 - y0}" fill="{hexc(GOLD)}"/>')
    ax0, ay0, ax1, ay1 = arc_box()
    cx, cy, r = (ax0 + ax1) / 2, (ay0 + ay1) / 2, (ax1 - ax0) / 2
    parts.append(f'<path d="M{cx - r:.1f},{cy:.1f} A{r:.1f},{r:.1f} 0 0 1 {cx + r:.1f},{cy:.1f} Z" fill="{hexc(GOLD)}"/>')
    hx0, hy0, hx1, hy1 = horizon()
    parts.append(f'<rect x="{hx0}" y="{hy0}" width="{hx1 - hx0}" height="{hy1 - hy0}" rx="6" fill="{hexc(GOLD)}"/>')
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def main():
    for name, ed in EDITIONS.items():
        out = os.path.join(ROOT, ed["dir"])
        os.makedirs(out, exist_ok=True)
        with open(os.path.join(out, f"{name}.svg"), "w") as f:
            f.write(svg(ed))
        draw(1024, ed).save(os.path.join(out, f"{name}-1024.png"), optimize=True)
        draw(256, ed).save(os.path.join(out, f"{name}-256.png"), optimize=True)
        draw(256, ed).save(os.path.join(out, f"{name}.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
        if name == "psg-pdf":
            draw(1024, ed).save(os.path.join(out, f"{name}.icns"))
        for px in ed["hicolor"]:
            d = os.path.join(out, "hicolor", f"{px}x{px}", "apps")
            os.makedirs(d, exist_ok=True)
            draw(px, ed).save(os.path.join(d, f"{ed['id']}.png"), optimize=True)
        d = os.path.join(out, "hicolor", "scalable", "apps")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, f"{ed['id']}.svg"), "w") as f:
            f.write(svg(ed))


if __name__ == "__main__":
    main()
