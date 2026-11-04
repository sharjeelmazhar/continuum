#!/usr/bin/env python3
"""Renders the Phone Mirroring app icon (same look as the Continuum icon: dark rounded square,
blue-to-purple glow) at every hicolor size into desktop/data/icons.

    python3 scripts/render-mirror-icon.py
"""
import os

from PIL import Image, ImageChops, ImageDraw, ImageFilter

S = 1024
APP_ID = "io.github.sharjeelmazhar.Continuum.Mirror"
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "desktop", "data", "icons", "hicolor")


def gradient(size, top, bottom, diagonal=False):
    w, h = size
    g = Image.new("RGBA", size)
    px = g.load()
    for y in range(h):
        for x in range(w):
            t = ((x + y) / (w + h)) if diagonal else y / max(1, h - 1)
            px[x, y] = tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(4))
    return g


def rounded(size, box, radius):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius, fill=255)
    return m


def main():
    icon = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # Background squircle, like the Continuum icon.
    pad = 64
    bg = gradient((S, S), (22, 26, 52, 255), (6, 8, 20, 255))
    icon.paste(bg, (0, 0), rounded((S, S), (pad, pad, S - pad, S - pad), 200))

    # The phone: a glowing gradient frame with a lit screen.
    pw, ph = 380, 700
    x0, y0 = (S - pw) // 2, (S - ph) // 2 + 6
    frame = (x0, y0, x0 + pw, y0 + ph)
    stroke = gradient((S, S), (90, 200, 255, 255), (170, 90, 255, 255), diagonal=True)
    outer = rounded((S, S), frame, 78)
    inner = rounded((S, S), (x0 + 26, y0 + 26, x0 + pw - 26, y0 + ph - 26), 56)
    ring = ImageChops.subtract(outer, inner)

    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    glow.paste(stroke, (0, 0), ring)
    glow = glow.filter(ImageFilter.GaussianBlur(28))
    icon.alpha_composite(glow)

    screen = gradient((S, S), (40, 80, 190, 255), (80, 30, 150, 255), diagonal=True)
    icon.paste(screen, (0, 0), inner)
    # A soft reflection across the screen.
    shine = Image.new("L", (S, S), 0)
    ImageDraw.Draw(shine).polygon([(x0, y0 + 80), (x0 + pw, y0 - 60), (x0 + pw, y0 + 260), (x0, y0 + 400)], fill=55)
    shine = ImageChops.multiply(shine, inner)
    icon.paste(Image.new("RGBA", (S, S), (255, 255, 255, 255)), (0, 0), shine)
    # Camera notch.
    ImageDraw.Draw(icon).ellipse((S // 2 - 16, y0 + 50, S // 2 + 16, y0 + 82), fill=(10, 12, 30, 255))
    lit = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    lit.paste(stroke, (0, 0), ring)
    icon.alpha_composite(lit)

    for size in (16, 24, 32, 48, 64, 128, 256, 512):
        d = os.path.join(ROOT, f"{size}x{size}", "apps")
        os.makedirs(d, exist_ok=True)
        out = os.path.join(d, f"{APP_ID}.png")
        tmp = out + ".part"
        icon.resize((size, size), Image.LANCZOS).save(tmp, "PNG", optimize=True)
        os.replace(tmp, out)
    print("written", ROOT)


if __name__ == "__main__":
    main()
