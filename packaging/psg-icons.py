#!/usr/bin/env python3
"""Generate the PSG PDF and PSG Academic PDF app icons.

Original artwork made for this repository (no fonts, no third-party graphics). The brand star is
Wenchang (文昌), the star of letters and scholarship. The mark follows the public star pattern the
Records of the Grand Historian describes as "six stars crowning the Dipper's bowl" (斗魁戴匡六星曰文昌宮):
six stars on an arc over a gold four-pointed star, inside a star-chart tick ring.

- PSG PDF: the six Wenchang stars over the gold star, on deep indigo.
- PSG Academic PDF: the six stars over the four stars of the Dipper's bowl (斗魁), whose first star
  is the gold one, on purple.

Licensed MIT OR Apache-2.0, like the repository.

Usage: python3 packaging/psg-icons.py   (needs Pillow and NumPy)
Writes assets/app-icon/ and assets/app-icon-academic/.
"""

import math
import os

import numpy as np
from PIL import Image, ImageDraw

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

LIGHT = (243, 236, 220)  # #f3ecdc starlight
GOLD = (230, 190, 106)   # #e6be6a

EDITIONS = {
    "psg-pdf": {
        "dir": "assets/app-icon",
        "id": "com.panoramascholarly.psgpdf",
        "top": (44, 40, 120),     # #2c2878
        "bottom": (17, 15, 46),   # #110f2e
        "mark": "wenchang",
        "hicolor": [16, 24, 32, 48, 64, 128, 256, 512],
    },
    "psg-academic-pdf": {
        "dir": "assets/app-icon-academic",
        "id": "com.panoramascholarly.psgacademicpdf",
        "top": (106, 44, 122),    # #6a2c7a
        "bottom": (42, 16, 54),   # #2a1036
        "mark": "wenchang-kui",
        "hicolor": [16, 32, 48, 64, 128, 256],
    },
}

# The drawing lives on a 120-unit grid; the tile covers 4..116 and the mark is drawn at 80 % around (60, 60).
TILE = (4.0, 112.0)
TILE_R = 24.5
MARK_SCALE = 0.8

ARC_ANGLES = [200, 228, 256, 284, 312, 340]
ARC_JITTER = [0, 2, -1, 1, -2, 0]
ARC_SIZES = [3.4, 2.2, 2.8, 2.0, 2.6, 2.3]
BOWL = [(42, 64), (80, 61), (84, 88), (46, 92)]
BOWL_SIZES = [2.8, 2.4, 2.6]


def hexc(c):
    return "#%02x%02x%02x" % c


def arc(cx, cy, r):
    out = []
    for a, j in zip(ARC_ANGLES, ARC_JITTER):
        t = math.radians(a)
        out.append((cx + (r + j) * math.cos(t), cy + (r + j) * math.sin(t)))
    return out


def sparkle_paths(cx, cy, r):
    """Two four-pointed stars (the main one and a smaller diagonal one) as quadratic Bézier loops."""
    k = r * 0.12
    main = [((cx, cy - r), (cx + k, cy - k), (cx + r, cy)), ((cx + r, cy), (cx + k, cy + k), (cx, cy + r)),
            ((cx, cy + r), (cx - k, cy + k), (cx - r, cy)), ((cx - r, cy), (cx - k, cy - k), (cx, cy - r))]
    r2 = r * 0.42
    k2 = r2 * 0.18
    c = r2 * math.sqrt(0.5)
    diag = [((cx - c, cy - c), (cx, cy - k2), (cx + c, cy - c)), ((cx + c, cy - c), (cx + k2, cy), (cx + c, cy + c)),
            ((cx + c, cy + c), (cx, cy + k2), (cx - c, cy + c)), ((cx - c, cy + c), (cx - k2, cy), (cx - c, cy - c))]
    return main, diag


def mark(name):
    """The mark as primitives on the 120 grid: (kind, data, colour, alpha, width)."""
    prims = [("circle", (60, 60, 55), LIGHT, 0.4, 0.9)]
    for t in range(36):
        a = math.radians(t * 10)
        r1 = 51 if t % 3 == 0 else 53
        prims.append(("line", [(60 + r1 * math.cos(a), 60 + r1 * math.sin(a)), (60 + 55 * math.cos(a), 60 + 55 * math.sin(a))], LIGHT, 0.4, 0.8))
    if name == "wenchang":
        stars = arc(60, 76, 44)
        prims.append(("line", stars, LIGHT, 0.6, 1.4))
        prims += [("dot", (x, y, s), LIGHT, 1.0, 0) for (x, y), s in zip(stars, ARC_SIZES)]
        prims.append(("sparkle", (60, 72, 20), GOLD, 1.0, 0))
    else:
        stars = arc(60, 70, 42)
        prims.append(("line", stars, LIGHT, 0.6, 1.4))
        prims.append(("line", BOWL + [BOWL[0]], LIGHT, 0.55, 1.3))
        prims += [("dot", (x, y, s), LIGHT, 1.0, 0) for (x, y), s in zip(stars, ARC_SIZES)]
        prims += [("dot", (x, y, s), LIGHT, 1.0, 0) for (x, y), s in zip(BOWL[1:], BOWL_SIZES)]
        prims.append(("sparkle", (BOWL[0][0], BOWL[0][1], 13), GOLD, 1.0, 0))
    return prims


def to_tile(p):
    return (60 + (p[0] - 60) * MARK_SCALE, 60 + (p[1] - 60) * MARK_SCALE)


# ---------- SVG ----------

def f(n):
    return ("%.2f" % n).rstrip("0").rstrip(".")


def quad_d(loop):
    d = "M%s,%s" % (f(loop[0][0][0]), f(loop[0][0][1]))
    for _, ctrl, end in loop:
        d += " Q%s,%s %s,%s" % (f(ctrl[0]), f(ctrl[1]), f(end[0]), f(end[1]))
    return d + "Z"


def svg(ed):
    x0, w = TILE
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{f(x0)} {f(x0)} {f(w)} {f(w)}" width="512" height="512">',
        '<defs><linearGradient id="field" x1="0" y1="0" x2=".4" y2="1">'
        f'<stop offset="0" stop-color="{hexc(ed["top"])}"/><stop offset="1" stop-color="{hexc(ed["bottom"])}"/></linearGradient>'
        f'<radialGradient id="halo"><stop offset="0" stop-color="{hexc(GOLD)}" stop-opacity=".45"/>'
        f'<stop offset="1" stop-color="{hexc(GOLD)}" stop-opacity="0"/></radialGradient></defs>',
        f'<rect x="{f(x0)}" y="{f(x0)}" width="{f(w)}" height="{f(w)}" rx="{f(TILE_R)}" fill="url(#field)"/>',
        f'<g transform="translate(60 60) scale({MARK_SCALE}) translate(-60 -60)">',
    ]
    for kind, data, col, alpha, width in mark(ed["mark"]):
        if kind == "circle":
            cx, cy, r = data
            parts.append(f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r)}" fill="none" stroke="{hexc(col)}" stroke-width="{f(width)}" stroke-opacity="{f(alpha)}"/>')
        elif kind == "line":
            pts = " ".join(f"{f(x)},{f(y)}" for x, y in data)
            parts.append(f'<polyline points="{pts}" fill="none" stroke="{hexc(col)}" stroke-width="{f(width)}" stroke-opacity="{f(alpha)}" stroke-linejoin="round" stroke-linecap="round"/>')
        elif kind == "dot":
            x, y, r = data
            parts.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(r)}" fill="{hexc(col)}"/>')
        elif kind == "sparkle":
            cx, cy, r = data
            main, diag = sparkle_paths(cx, cy, r)
            parts.append(f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r * 1.5)}" fill="url(#halo)"/>')
            parts.append(f'<path d="{quad_d(diag)}" fill="{hexc(col)}" opacity=".85"/>')
            parts.append(f'<path d="{quad_d(main)}" fill="{hexc(col)}"/>')
    parts.append("</g></svg>")
    return "\n".join(parts) + "\n"


# ---------- raster ----------

def flatten(loop, steps=24):
    pts = []
    for p0, c, p1 in loop:
        for i in range(steps):
            t = i / steps
            pts.append(((1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * c[0] + t * t * p1[0],
                        (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * c[1] + t * t * p1[1]))
    return pts


def draw(size, ed):
    ss = max(2, min(8, 4096 // size))
    big = size * ss
    unit = big / TILE[1]  # pixels per grid unit

    def px(p):
        return ((p[0] - TILE[0]) * unit, (p[1] - TILE[0]) * unit)

    # Gradient field along (0,0)->(0.4,1) in the tile's bounding box, clipped to the rounded tile.
    v = np.linspace(0.0, 1.0, big, dtype=np.float32)
    t = np.clip((0.4 * v[None, :] + v[:, None]) / 1.16, 0.0, 1.0)
    top, bot = np.array(ed["top"], np.float32), np.array(ed["bottom"], np.float32)
    rgb = (top[None, None, :] * (1 - t[..., None]) + bot[None, None, :] * t[..., None]).round().astype(np.uint8)
    field = Image.fromarray(rgb, "RGB").convert("RGBA")
    mask = Image.new("L", (big, big), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, big - 1, big - 1), radius=TILE_R * unit, fill=255)
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    img.paste(field, (0, 0), mask)

    sc = MARK_SCALE * unit
    for kind, data, col, alpha, width in mark(ed["mark"]):
        layer = Image.new("RGBA", (big, big), (0, 0, 0, 0))
        d = ImageDraw.Draw(layer)
        fill = col + (round(alpha * 255),)
        if kind == "circle":
            cx, cy, r = data
            x, y = px(to_tile((cx, cy)))
            rr = r * sc
            d.ellipse((x - rr, y - rr, x + rr, y + rr), outline=fill, width=max(1, round(width * sc)))
        elif kind == "line":
            pts = [px(to_tile(p)) for p in data]
            w = max(1, round(width * sc))
            d.line(pts, fill=fill, width=w, joint="curve")
            for x, y in (pts[0], pts[-1]):
                d.ellipse((x - w / 2, y - w / 2, x + w / 2, y + w / 2), fill=fill)
        elif kind == "dot":
            x, y = px(to_tile(data[:2]))
            rr = data[2] * sc
            d.ellipse((x - rr, y - rr, x + rr, y + rr), fill=fill)
        elif kind == "sparkle":
            cx, cy, r = data
            x, y = px(to_tile((cx, cy)))
            hr = max(1, round(r * 1.5 * sc))
            fall = np.clip(1 - np.hypot(*np.meshgrid(np.linspace(-1, 1, 2 * hr), np.linspace(-1, 1, 2 * hr))), 0, 1)
            halo = Image.new("RGBA", (2 * hr, 2 * hr), col + (0,))
            halo.putalpha(Image.fromarray((fall * 0.45 * 255).round().astype(np.uint8), "L"))
            layer.alpha_composite(halo, (round(x - hr), round(y - hr)))
            main, diag = sparkle_paths(cx, cy, r)
            dl = Image.new("RGBA", (big, big), (0, 0, 0, 0))
            ImageDraw.Draw(dl).polygon([px(to_tile(p)) for p in flatten(diag)], fill=col + (217,))
            layer.alpha_composite(dl)
            ImageDraw.Draw(layer).polygon([px(to_tile(p)) for p in flatten(main)], fill=col + (255,))
        img.alpha_composite(layer)
    # Keep everything inside the rounded tile.
    img.putalpha(Image.fromarray(np.minimum(np.asarray(img.getchannel("A")), np.asarray(mask)), "L"))
    return img.resize((size, size), Image.LANCZOS)


def main():
    for name, ed in EDITIONS.items():
        out = os.path.join(ROOT, ed["dir"])
        os.makedirs(out, exist_ok=True)
        with open(os.path.join(out, f"{name}.svg"), "w") as fh:
            fh.write(svg(ed))
        draw(1024, ed).save(os.path.join(out, f"{name}-1024.png"), optimize=True)
        draw(256, ed).save(os.path.join(out, f"{name}-256.png"), optimize=True)
        draw(256, ed).save(os.path.join(out, f"{name}.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
        draw(1024, ed).save(os.path.join(out, f"{name}.icns"))
        for n in ed["hicolor"]:
            d = os.path.join(out, "hicolor", f"{n}x{n}", "apps")
            os.makedirs(d, exist_ok=True)
            draw(n, ed).save(os.path.join(d, f"{ed['id']}.png"), optimize=True)
        d = os.path.join(out, "hicolor", "scalable", "apps")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, f"{ed['id']}.svg"), "w") as fh:
            fh.write(svg(ed))


if __name__ == "__main__":
    main()
