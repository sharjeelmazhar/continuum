#!/usr/bin/env python3
"""Renders the Phone Mirroring app icon (same look as the Continuum icon: dark rounded square,
blue-to-purple glow): a phone showing its home screen at every hicolor size into desktop/data/icons.

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


def blob(size, cx, cy, r, color):
    """A soft coloured light, for the wallpaper."""
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    ImageDraw.Draw(layer).ellipse((cx - r, cy - r, cx + r, cy + r), fill=color)
    return layer.filter(ImageFilter.GaussianBlur(r * 0.55))


def main():
    icon = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # Background squircle, like the Continuum icon, with a faint top light.
    pad = 64
    square = rounded((S, S), (pad, pad, S - pad, S - pad), 200)
    bg = gradient((S, S), (24, 28, 58, 255), (5, 7, 18, 255))
    icon.paste(bg, (0, 0), square)
    top = blob((S, S), S // 2, 120, 380, (70, 90, 200, 70))
    icon.alpha_composite(Image.composite(top, Image.new("RGBA", (S, S)), square))

    # The phone.
    pw, ph = 404, 760
    x0, y0 = (S - pw) // 2, (S - ph) // 2 + 4
    body = rounded((S, S), (x0, y0, x0 + pw, y0 + ph), 92)
    inset = 20
    sx0, sy0, sx1, sy1 = x0 + inset, y0 + inset, x0 + pw - inset, y0 + ph - inset
    screen = rounded((S, S), (sx0, sy0, sx1, sy1), 74)

    # Glow behind it, in the Continuum blue-to-purple.
    halo = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    halo.paste(gradient((S, S), (70, 170, 255, 255), (175, 80, 255, 255), diagonal=True), (0, 0), body)
    icon.alpha_composite(Image.composite(halo.filter(ImageFilter.GaussianBlur(46)), Image.new("RGBA", (S, S)), square))

    # Titanium rim: light at the top-left edge, darker below.
    rim = gradient((S, S), (200, 210, 235, 255), (70, 76, 100, 255), diagonal=True)
    icon.paste(rim, (0, 0), body)
    bezel = rounded((S, S), (x0 + 7, y0 + 7, x0 + pw - 7, y0 + ph - 7), 86)
    icon.paste(Image.new("RGBA", (S, S), (8, 9, 14, 255)), (0, 0), bezel)

    # Wallpaper: deep blue with soft coloured lights.
    wall = gradient((S, S), (28, 40, 120, 255), (60, 20, 110, 255))
    for cx, cy, r, c in (
        (sx0 + 60, sy0 + 120, 230, (60, 170, 255, 255)),
        (sx1 - 40, sy0 + 330, 210, (190, 80, 255, 255)),
        (sx0 + 90, sy1 - 170, 220, (255, 90, 170, 235)),
        (sx1 - 70, sy1 - 40, 170, (255, 170, 80, 200)),
    ):
        wall.alpha_composite(blob((S, S), cx, cy, r, c))
    icon.paste(wall, (0, 0), screen)

    # Home screen: a 4x4 app grid and a dock, in Apple's system colours.
    d = ImageDraw.Draw(icon)
    colours = [
        (255, 59, 48), (255, 149, 0), (255, 204, 0), (52, 199, 89),
        (0, 199, 190), (48, 176, 199), (0, 122, 255), (88, 86, 214),
        (175, 82, 222), (255, 45, 85), (162, 132, 94), (90, 200, 250),
        (52, 199, 89), (0, 122, 255), (255, 149, 0), (255, 59, 48),
    ]
    cols, tile, gap = 4, 62, 25
    gx = sx0 + ((sx1 - sx0) - (cols * tile + (cols - 1) * gap)) // 2
    gy = sy0 + 110
    for i, c in enumerate(colours):
        r, k = divmod(i, cols)
        tx, ty = gx + k * (tile + gap), gy + r * (tile + gap + 14)
        tl = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        tl.paste(gradient((S, S), tuple(min(255, v + 45) for v in c) + (255,), c + (255,)), (0, 0),
                 rounded((S, S), (tx, ty, tx + tile, ty + tile), 16))
        icon.alpha_composite(tl)
    # Dock: a frosted shelf with four apps.
    dy0 = sy1 - 128
    dock = rounded((S, S), (sx0 + 22, dy0, sx1 - 22, sy1 - 22), 52)
    icon.paste(Image.new("RGBA", (S, S), (255, 255, 255, 255)), (0, 0), ImageChops.multiply(dock, Image.new("L", (S, S), 60)))
    for k, c in enumerate(((52, 199, 89), (0, 122, 255), (255, 255, 255), (255, 59, 48))):
        tx, ty = gx + k * (tile + gap), dy0 + 22
        tl = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        tl.paste(gradient((S, S), tuple(min(255, v + 45) for v in c) + (255,), c + (255,)), (0, 0),
                 rounded((S, S), (tx, ty, tx + tile, ty + tile), 16))
        icon.alpha_composite(tl)

    # A soft reflection across the glass.
    shine = Image.new("L", (S, S), 0)
    ImageDraw.Draw(shine).polygon([(sx0, sy0 + 40), (sx1, sy0 - 120), (sx1, sy0 + 170), (sx0, sy0 + 360)], fill=34)
    icon.paste(Image.new("RGBA", (S, S), (255, 255, 255, 255)), (0, 0), ImageChops.multiply(shine, screen))
    # Camera pill at the top.
    d.rounded_rectangle((S // 2 - 58, sy0 + 26, S // 2 + 58, sy0 + 62), 18, fill=(0, 0, 0, 255))

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
