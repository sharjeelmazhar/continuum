#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Renders the device pictures (desktop, laptop, phone, tablet) in the logo's style:
glassy screens with a cyan-to-violet glow on deep navy, like AirDrop's device bubbles.

    python3 scripts/render-device-icons.py      # needs Pillow; writes into android/ and desktop/

Outputs are committed, so this only needs re-running when the design changes.
"""
import os

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
N = 1728  # 4x supersampling of 432 px (108 dp at xxxhdpi)
NAVY = (3, 8, 24)
CYAN = (79, 195, 255)
BLUE = (64, 110, 255)
VIOLET = (170, 92, 255)


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def gradient(size, stops):
    """Linear gradient over `stops`, from the top-left corner to the bottom-right one."""
    w, h = size
    strip = Image.new("RGB", (256, 1))
    for x in range(256):
        t = x / 255
        for k in range(len(stops) - 1):
            lo, hi = k / (len(stops) - 1), (k + 1) / (len(stops) - 1)
            if t <= hi:
                strip.putpixel((x, 0), lerp(stops[k], stops[k + 1], (t - lo) / (hi - lo)))
                break
    side = int((w + h) * 0.75)
    big = strip.resize((side, side)).rotate(-45, resample=Image.BILINEAR)
    left, top = (side - w) // 2, (side - h) // 2
    return big.crop((left, top, left + w, top + h))


def background():
    bg = Image.new("RGB", (N, N), NAVY)
    glow = Image.new("L", (N, N), 0)
    ImageDraw.Draw(glow).ellipse((N * 0.18, N * 0.18, N * 0.82, N * 0.82), fill=90)
    glow = glow.filter(ImageFilter.GaussianBlur(N * 0.12))
    bg.paste(Image.new("RGB", (N, N), (20, 40, 110)), (0, 0), glow)
    return bg.convert("RGBA")


def shape(box, radius):
    m = Image.new("L", (N, N), 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius=radius, fill=255)
    return m


def paint(canvas, mask, fill):
    layer = fill if isinstance(fill, Image.Image) else Image.new("RGB", (N, N), fill)
    canvas.paste(layer.convert("RGBA"), (0, 0), mask)


def neon(canvas, mask, width):
    """A glowing gradient rim along the edge of `mask`."""
    inner = mask.filter(ImageFilter.MinFilter(width * 2 + 1))
    rim = ImageChops.subtract(mask, inner)
    glow = rim.filter(ImageFilter.GaussianBlur(width * 3)).point(lambda v: min(255, v * 2))
    grad = gradient((N, N), [CYAN, BLUE, VIOLET])
    paint(canvas, glow.point(lambda v: v * 6 // 10), grad)
    paint(canvas, rim, grad)


def screen(canvas, box, radius, bezel):
    outer = shape(box, radius)
    paint(canvas, outer, (10, 16, 40))
    x0, y0, x1, y1 = box
    inner_box = (x0 + bezel, y0 + bezel, x1 - bezel, y1 - bezel)
    inner = shape(inner_box, max(4, radius - bezel))
    paint(canvas, inner, gradient((N, N), [(40, 90, 230), (34, 30, 120), (90, 40, 170)]))
    # Glass reflection: a soft diagonal band.
    shine = Image.new("L", (N, N), 0)
    w = x1 - x0
    ImageDraw.Draw(shine).polygon(
        [(x0 + w * 0.35, y0), (x0 + w * 0.62, y0), (x0 + w * 0.25, y1), (x0 - w * 0.02, y1)], fill=38)
    paint(canvas, ImageChops.multiply(shine, inner), (255, 255, 255))
    neon(canvas, outer, 9)


def desktop(c):
    screen(c, (N * .24, N * .27, N * .76, N * .62), 46, 30)
    stand = shape((N * .455, N * .62, N * .545, N * .70), 6)
    paint(c, stand, gradient((N, N), [BLUE, VIOLET]))
    base = shape((N * .38, N * .695, N * .62, N * .725), 20)
    paint(c, base, gradient((N, N), [CYAN, VIOLET]))
    neon(c, base, 5)


def laptop(c):
    screen(c, (N * .28, N * .29, N * .72, N * .60), 40, 26)
    deck = Image.new("L", (N, N), 0)
    ImageDraw.Draw(deck).rounded_rectangle((N * .22, N * .60, N * .78, N * .655), radius=28, fill=255)
    paint(c, deck, gradient((N, N), [CYAN, BLUE, VIOLET]))
    notch = shape((N * .46, N * .60, N * .54, N * .618), 10)
    paint(c, notch, (20, 26, 70))
    neon(c, deck, 6)


def phone(c):
    screen(c, (N * .37, N * .22, N * .63, N * .78), 70, 22)
    island = shape((N * .46, N * .255, N * .54, N * .285), 30)
    paint(c, island, (5, 8, 22))


def tablet(c):
    screen(c, (N * .27, N * .24, N * .73, N * .76), 56, 26)


def render(draw, size):
    c = background()
    draw(c)
    return c.resize((size, size), Image.LANCZOS)


def main():
    res = os.path.join(ROOT, "android/app/src/main/res")
    dens = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
    for name, fn in (("desktop", desktop), ("laptop", laptop)):
        big = render(fn, 432)
        for d, f in dens.items():
            out = os.path.join(res, f"mipmap-{d}", f"ic_device_{name}.png")
            os.makedirs(os.path.dirname(out), exist_ok=True)
            big.resize((int(108 * f),) * 2, Image.LANCZOS).convert("RGB").save(out, optimize=True)
    ui = os.path.join(ROOT, "desktop/continuum-ui/data")
    os.makedirs(ui, exist_ok=True)
    for name, fn in (("phone", phone), ("tablet", tablet), ("desktop", desktop), ("laptop", laptop)):
        # The desktop app shows these in a circle, AirDrop style.
        full = background()
        fn(full)
        z = int(N * 0.14)  # zoom in: the circle needs no launcher safe zone
        img = full.crop((z, z, N - z, N - z)).resize((192, 192), Image.LANCZOS)
        m = Image.new("L", (192, 192), 0)
        ImageDraw.Draw(m).ellipse((0, 0, 191, 191), fill=255)
        img.putalpha(m)
        img.save(os.path.join(ui, f"device-{name}.png"), optimize=True)


if __name__ == "__main__":
    main()
