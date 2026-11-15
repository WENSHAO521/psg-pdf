#!/usr/bin/env python3
"""Generate every PSG PDF and PSG Academic PDF icon from the brand master.

The one editable source is assets/brand/psg-logo-master.svg (see assets/brand/README.md): both
app icons side by side, one group per product. This script cuts each product's group out of it
and writes, per product:

    <dir>/<name>.svg                         the 1024 x 1024 vector icon
    <dir>/<name>-1024.png, <name>-256.png    rasters (the window icons)
    <dir>/<name>.ico                         Windows, 16-256 px
    <dir>/<name>.icns                        macOS, 16-1024 px
    <dir>/hicolor/<n>x<n>/apps/<app id>.png  Linux icon theme, and hicolor/scalable/apps/<app id>.svg

Sizes up to SMALL px leave out the master's `class="detail"` elements (keylines, ornaments,
hairlines), which turn into noise at that size, and use its `class="compact"` elements instead:
the same line art with heavier strokes, so a 16 px icon is still the approved design.

Contributor-original artwork, MIT OR Apache-2.0, like the repository.

Usage: python3 packaging/psg-icons.py   (needs resvg on PATH: `cargo install resvg --locked`,
and Pillow). Then `cargo xtask assets --write` to record the new hashes, and
`python3 packaging/macos/dmg/generate.py` for the DMG backgrounds, which show the icons.
"""

import copy
import io
import os
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

from PIL import Image

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
MASTER = os.path.join(ROOT, "assets", "brand", "psg-logo-master.svg")
SVG_NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", SVG_NS)

# Keep in sync with packaging/products.toml (icon_dir, app_id).
PRODUCTS = {
    "psg-pdf": {
        "dir": "assets/app-icon",
        "id": "com.panoramascholarly.psgpdf",
        "hicolor": [16, 24, 32, 48, 64, 128, 256, 512],
    },
    "psg-academic-pdf": {
        "dir": "assets/app-icon-academic",
        "id": "com.panoramascholarly.psgacademicpdf",
        "hicolor": [16, 24, 32, 48, 64, 128, 256, 512],
    },
}
SMALL = 48
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
ICNS_SIZES = [16, 32, 64, 128, 256, 512, 1024]


def product_svg(name, detail=True):
    """The master's group `name` as a standalone 1024 x 1024 SVG."""
    tree = ET.parse(MASTER)
    root = tree.getroot()
    group = root.find(f".//{{{SVG_NS}}}g[@id='{name}']")
    if group is None:
        sys.exit(f"error: {MASTER} has no <g id=\"{name}\">")
    defs = root.find(f"{{{SVG_NS}}}defs")
    out = ET.Element(f"{{{SVG_NS}}}svg", {"width": "1024", "height": "1024", "viewBox": "0 0 1024 1024"})
    out.append(ET.Comment(f" Generated from assets/brand/psg-logo-master.svg (#{name}) by packaging/psg-icons.py; edit the master. "))
    if defs is not None:
        out.append(copy.deepcopy(defs))
    g = copy.deepcopy(group)
    g.attrib.pop("transform", None)
    # `detail` is for 64 px and up, `compact` for 16-48 px; the master's <style> only hides
    # `compact` in its own preview, so it isn't copied.
    strip(g, "detail" if not detail else "compact")
    out.append(g)
    ET.indent(out, "  ")
    return ET.tostring(out, encoding="unicode") + "\n"


def strip(el, cls):
    for child in list(el):
        if cls in child.attrib.get("class", "").split():
            el.remove(child)
        else:
            strip(child, cls)


def render(svg_text, size):
    with tempfile.TemporaryDirectory() as tmp:
        src, out = os.path.join(tmp, "i.svg"), os.path.join(tmp, "i.png")
        with open(src, "w") as f:
            f.write(svg_text)
        subprocess.run(["resvg", "--skip-system-fonts", "-w", str(size), "-h", str(size), src, out], check=True)
        with Image.open(out) as im:
            return im.convert("RGBA")


def macos(icon, n):
    """Apple's icon grid: the tile (944 of 1024 units) drawn as an 824/1024 body, centred on a
    transparent canvas, so the Dock shows it at the size of other Mac apps."""
    m = round(n * 824 / 944)
    canvas = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    off = (n - m) // 2
    canvas.alpha_composite(icon(m), (off, off))
    return canvas


def main():
    for name, p in PRODUCTS.items():
        out = os.path.join(ROOT, p["dir"])
        os.makedirs(out, exist_ok=True)
        full, small = product_svg(name), product_svg(name, detail=False)
        cache = {}

        def icon(n):
            if n not in cache:
                cache[n] = render(full if n > SMALL else small, n)
            return cache[n]

        with open(os.path.join(out, f"{name}.svg"), "w") as fh:
            fh.write(full)
        icon(1024).save(os.path.join(out, f"{name}-1024.png"), optimize=True)
        icon(256).save(os.path.join(out, f"{name}-256.png"), optimize=True)
        ico = [icon(n) for n in ICO_SIZES]
        ico[-1].save(os.path.join(out, f"{name}.ico"), sizes=[(n, n) for n in ICO_SIZES], append_images=ico[:-1])
        icns = [macos(icon, n) for n in ICNS_SIZES]
        icns[-1].save(os.path.join(out, f"{name}.icns"), append_images=icns[:-1])
        for n in p["hicolor"]:
            d = os.path.join(out, "hicolor", f"{n}x{n}", "apps")
            os.makedirs(d, exist_ok=True)
            icon(n).save(os.path.join(d, f"{p['id']}.png"), optimize=True)
        d = os.path.join(out, "hicolor", "scalable", "apps")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, f"{p['id']}.svg"), "w") as fh:
            fh.write(full)
        print(f"wrote {name}: {p['dir']}")


if __name__ == "__main__":
    main()
