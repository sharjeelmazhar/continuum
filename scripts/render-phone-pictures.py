#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Renders the phone pictures shown in the desktop app's sidebar: original back-view
illustrations of popular phone families (body shape, camera layout and a signature colour),
drawn from scratch. No photos, no logos, no brand marks.

    python3 scripts/render-phone-pictures.py         # needs Pillow; writes desktop/continuum-ui/data/phones/
    python3 scripts/render-phone-pictures.py pixel-9 # only the named pictures

Outputs are committed, so this only needs re-running when a design changes. The mapping
from phone models to these pictures lives in desktop/continuum-ui/src/models.rs.
"""
import math
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "desktop", "continuum-ui", "data", "phones")
SIZE = 512
S = SIZE * 3  # supersampled canvas


def mix(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def lighter(c, t):
    return mix(c, (255, 255, 255), t)


def darker(c, t):
    return mix(c, (0, 0, 0), t)


def grad_mask(w, h, angle):
    """0..255 ramp over a w x h box; angle 0 runs top to bottom, 90 left to right."""
    n = 32
    dx, dy = math.sin(math.radians(angle)), math.cos(math.radians(angle))
    corners = [x * w * dx + y * h * dy for x in (0, 1) for y in (0, 1)]
    lo, hi = min(corners), max(corners)
    small = Image.new("L", (n, n))
    small.putdata([
        int(255 * ((x / (n - 1)) * w * dx + (y / (n - 1)) * h * dy - lo) / (hi - lo or 1))
        for y in range(n) for x in range(n)
    ])
    return small.resize((max(1, int(w)), max(1, int(h))), Image.BILINEAR)


def gradient(box, c0, c1, angle=45):
    """Full-canvas RGB image with a c0 -> c1 gradient laid over `box`."""
    x0, y0, x1, y1 = [int(v) for v in box]
    img = Image.new("RGB", (S, S), c1)
    w, h = max(1, x1 - x0), max(1, y1 - y0)
    m = grad_mask(w, h, angle)
    part = Image.composite(Image.new("RGB", (w, h), c1), Image.new("RGB", (w, h), c0), m)
    img.paste(part, (x0, y0))
    return img


def radial(cx, cy, r, c_in, c_out):
    img = Image.new("RGB", (S, S), c_out)
    d = max(2, int(2 * r))
    m = Image.radial_gradient("L").resize((d, d), Image.BILINEAR)
    part = Image.composite(Image.new("RGB", (d, d), c_out), Image.new("RGB", (d, d), c_in), m)
    img.paste(part, (int(cx - r), int(cy - r)))
    return img


def blank():
    return Image.new("L", (S, S), 0)


def rrect(box, r):
    m = blank()
    ImageDraw.Draw(m).rounded_rectangle([int(v) for v in box], radius=int(r), fill=255)
    return m


def ellipse(box):
    m = blank()
    ImageDraw.Draw(m).ellipse([int(v) for v in box], fill=255)
    return m


def circle(cx, cy, r):
    return ellipse((cx - r, cy - r, cx + r, cy + r))


def scale(mask, a):
    return mask.point(lambda v: v * a // 255)


def shift(mask, dx, dy):
    out = blank()
    out.paste(mask, (int(dx), int(dy)))
    return out


def quantize(img):
    """256-colour PNG: 224 dithered colours for the opaque phone, 32 alpha steps of black for
    the shadow and the anti-aliased outline. About a fifth of the size of full RGBA."""
    alpha = img.getchannel("A")
    rgb = Image.new("RGB", img.size, (0, 0, 0))
    rgb.paste(img.convert("RGB"), (0, 0), alpha.point(lambda a: 255 if a == 255 else 0))
    body = rgb.quantize(colors=224, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
    palette = body.getpalette()[:224 * 3]
    palette += [0, 0, 0] * 32
    idx = [b if a == 255 else 224 + a // 8 for b, a in zip(body.tobytes(), alpha.tobytes())]
    out = Image.new("P", img.size)
    out.putdata(idx)
    out.putpalette(palette)
    out.info["transparency"] = bytes([255] * 224 + [min(255, k * 8 + 4) if k else 0 for k in range(32)])
    return out


class Phone:
    """One back-view drawing. Positions are in units of the body width, from its top-left."""

    def __init__(self, aspect, radius, color, color2=None, frame=None, finish="glass",
                 height=0.80, rim=0.016):
        H = S * height
        W = H / aspect
        if W > S * 0.62:
            W = S * 0.62
            H = W * aspect
        self.W, self.H = W, H
        self.x0, self.y0 = (S - W) / 2, (S - H) / 2 - S * 0.025
        self.finish = finish
        self.color = color
        self.img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        r = radius * W
        box = (self.x0, self.y0, self.x0 + W, self.y0 + H)
        self.body = rrect(box, r)
        # Soft drop shadow and a tighter contact shadow under the bottom edge.
        inset = W * 0.06
        drop = rrect((box[0] + inset, box[1] + inset, box[2] - inset, box[3]), r)
        self.over((0, 0, 0), scale(shift(drop, 0, S * 0.03).filter(
            ImageFilter.GaussianBlur(S * 0.028)), 130))
        self.over((0, 0, 0), scale(shift(self.body, 0, S * 0.008).filter(
            ImageFilter.GaussianBlur(S * 0.008)), 90))
        # Metal frame, then the back panel inset by the rim.
        frame = frame or darker(color, 0.15)
        self.paint(self.body, gradient(box, lighter(frame, 0.45), darker(frame, 0.25), 30))
        t = rim * W
        self.back = rrect((box[0] + t, box[1] + t, box[2] - t, box[3] - t), max(2, r - t))
        c2 = color2 or darker(color, 0.16)
        c1 = lighter(color, 0.14) if color2 is None else color
        self.paint(self.back, gradient(box, c1, c2, 40))

    # -- primitives ---------------------------------------------------------------------
    def P(self, u, v):
        return self.x0 + u * self.W, self.y0 + v * self.W

    def box(self, u0, v0, u1, v1):
        return (*self.P(u0, v0), *self.P(u1, v1))

    def clip(self, mask):
        return ImageChops.multiply(mask, self.back)

    def paint(self, mask, fill):
        src = fill.convert("RGBA") if isinstance(fill, Image.Image) else Image.new("RGBA", (S, S), fill)
        src.putalpha(mask)
        self.img.alpha_composite(src)

    def over(self, color, mask):
        self.paint(mask, color)

    def raise_shadow(self, mask, depth, strength=120):
        sh = shift(mask, depth * 0.3, depth).filter(ImageFilter.GaussianBlur(depth * 1.2))
        self.over((0, 0, 0), self.clip(scale(sh, strength)))

    def rim_light(self, mask, d, strength=110):
        """Bright top-left edge and dark bottom-right edge on a raised part."""
        self.over((255, 255, 255), scale(ImageChops.subtract(mask, shift(mask, d, d)), strength))
        self.over((0, 0, 0), scale(ImageChops.subtract(mask, shift(mask, -d, -d)), strength // 2))

    # -- parts ----------------------------------------------------------------------------
    def module(self, u0, v0, u1, v1, r, color, color2=None, raised=True, clip=True, angle=40):
        b = self.box(u0, v0, u1, v1)
        m = rrect(b, r * self.W)
        if clip:
            m = self.clip(m)
        if raised:
            self.raise_shadow(m, self.W * 0.018)
        self.paint(m, gradient(b, lighter(color, 0.08), color2 or darker(color, 0.12), angle))
        if raised:
            self.rim_light(m, max(2, self.W * 0.006))
        return m

    def disc(self, u, v, R, color, color2=None, ring=None, ring_w=0.03):
        cx, cy = self.P(u, v)
        Rp = R * self.W
        outer = self.clip(circle(cx, cy, Rp))
        self.raise_shadow(outer, self.W * 0.02, 140)
        if ring:
            self.paint(outer, gradient((cx - Rp, cy - Rp, cx + Rp, cy + Rp),
                                       lighter(ring, 0.5), darker(ring, 0.3), 35))
            Rp -= ring_w * self.W
        inner = self.clip(circle(cx, cy, Rp))
        self.paint(inner, gradient((cx - Rp, cy - Rp, cx + Rp, cy + Rp),
                                   lighter(color, 0.1), color2 or darker(color, 0.25), 35))
        self.rim_light(outer, max(2, self.W * 0.005))

    def lens(self, u, v, r, ring=(200, 202, 208), bezel=0.84, glass=0.68):
        cx, cy = self.P(u, v)
        R = r * self.W
        self.raise_shadow(circle(cx, cy, R), R * 0.16, 150)
        if ring:
            self.paint(circle(cx, cy, R), gradient((cx - R, cy - R, cx + R, cy + R),
                                                   lighter(ring, 0.55), darker(ring, 0.35), 35))
        self.over((14, 14, 18), circle(cx, cy, R * bezel))
        g = R * glass
        self.paint(circle(cx, cy, g), radial(cx + g * 0.12, cy + g * 0.12, g * 1.05,
                                             (46, 54, 96), (4, 4, 10)))
        crescent = ImageChops.subtract(circle(cx, cy, g), circle(cx - g * 0.18, cy - g * 0.2, g))
        self.over((120, 70, 210), scale(crescent.filter(ImageFilter.GaussianBlur(g * 0.06)), 120))
        ring2 = ImageChops.subtract(circle(cx, cy, g * 0.52), circle(cx, cy, g * 0.44))
        self.over((80, 100, 160), scale(ring2, 110))
        self.over((0, 0, 0), circle(cx, cy, g * 0.28))
        hl = circle(cx - g * 0.38, cy - g * 0.4, g * 0.2).filter(ImageFilter.GaussianBlur(g * 0.05))
        self.over((255, 255, 255), scale(hl, 230))
        self.over((255, 255, 255), scale(circle(cx + g * 0.3, cy + g * 0.32, g * 0.08), 120))
        if ring:
            self.rim_light(circle(cx, cy, R), max(2, R * 0.05), 120)

    def flash(self, u, v, r):
        cx, cy = self.P(u, v)
        R = r * self.W
        self.over((30, 30, 32), circle(cx, cy, R))
        self.paint(circle(cx, cy, R * 0.78), radial(cx, cy, R * 0.78, (255, 250, 228), (214, 196, 150)))
        self.over((255, 255, 255), scale(circle(cx - R * 0.25, cy - R * 0.25, R * 0.25), 200))

    def dot(self, u, v, r, color=(12, 12, 16)):
        cx, cy = self.P(u, v)
        self.over(color, circle(cx, cy, r * self.W))

    def square_lens(self, u, v, half, ring=(200, 202, 208)):
        """Periscope window."""
        b = self.box(u - half, v - half, u + half, v + half)
        m = rrect(b, half * 0.25 * self.W)
        self.raise_shadow(m, half * self.W * 0.15)
        self.paint(m, gradient(b, lighter(ring, 0.5), darker(ring, 0.3), 35))
        h2 = half * 0.78
        inner = self.box(u - h2, v - h2, u + h2, v + h2)
        cx, cy = self.P(u, v)
        self.paint(rrect(inner, h2 * 0.2 * self.W), radial(cx, cy, h2 * 1.3 * self.W, (40, 46, 84), (4, 4, 10)))
        self.over((255, 255, 255), scale(circle(cx - h2 * 0.45 * self.W, cy - h2 * 0.45 * self.W,
                                               h2 * 0.18 * self.W), 200))

    def band(self, v0, v1, color, color2=None, raised=False):
        b = (self.x0, self.y0 + v0 * self.W, self.x0 + self.W, self.y0 + v1 * self.W)
        m = self.clip(rrect(b, 0))
        if raised:
            self.raise_shadow(m, self.W * 0.02)
        self.paint(m, gradient(b, lighter(color, 0.12), color2 or darker(color, 0.12), 20))
        if raised:
            self.rim_light(m, max(2, self.W * 0.007))
        return m

    def screen(self, u0, v0, u1, v1, r, glow=(60, 80, 200), glow2=(150, 60, 200)):
        b = self.box(u0, v0, u1, v1)
        m = self.clip(rrect(b, r * self.W))
        self.over((6, 6, 10), m)
        # A dim wallpaper glow and a glass reflection.
        cx, cy = (b[0] + b[2]) / 2, (b[1] + b[3]) / 2
        w = (b[2] - b[0]) * 0.5
        self.over(glow, ImageChops.multiply(m, scale(circle(cx - w * 0.3, cy - w * 0.1, w * 0.7)
                                                     .filter(ImageFilter.GaussianBlur(w * 0.35)), 140)))
        self.over(glow2, ImageChops.multiply(m, scale(circle(cx + w * 0.4, cy + w * 0.3, w * 0.5)
                                                      .filter(ImageFilter.GaussianBlur(w * 0.35)), 120)))
        return m

    def line(self, pts, w, color=(255, 255, 255), glow=True, casing=None):
        if casing:
            self.line(pts, w * 1.7, casing, glow=False)
        m = blank()
        d = ImageDraw.Draw(m)
        pp = [self.P(u, v) for u, v in pts]
        d.line(pp, fill=255, width=int(w * self.W), joint="curve")
        for p in (pp[0], pp[-1]):
            rr = w * self.W / 2
            d.ellipse((p[0] - rr, p[1] - rr, p[0] + rr, p[1] + rr), fill=255)
        if glow:
            self.over(color, scale(m.filter(ImageFilter.GaussianBlur(w * self.W * 1.5)), 200))
        self.over(color, m)

    def arc(self, u, v, R, start, end, w, color=(255, 255, 255), casing=None):
        cx, cy = self.P(u, v)
        Rp = R * self.W
        if casing:
            c = blank()
            ImageDraw.Draw(c).arc((cx - Rp, cy - Rp, cx + Rp, cy + Rp), start, end, fill=255,
                                  width=int(w * 1.7 * self.W) + int(w * 0.35 * self.W))
            self.over(casing, c)
        m = blank()
        ImageDraw.Draw(m).arc((cx - Rp, cy - Rp, cx + Rp, cy + Rp), start, end, fill=255,
                              width=int(w * self.W))
        self.over(color, scale(m.filter(ImageFilter.GaussianBlur(w * self.W * 1.5)), 200))
        self.over(color, m)

    def finish_up(self):
        """Curved-edge shading, a top-left specular rim and a soft diagonal sheen."""
        W = self.W
        inner = self.body.filter(ImageFilter.GaussianBlur(W * 0.04))
        edge = ImageChops.multiply(ImageChops.invert(inner), self.body)
        self.over((0, 0, 0), scale(edge, 90))
        spec = ImageChops.subtract(self.back, shift(self.back, W * 0.012, W * 0.012))
        self.over((255, 255, 255), scale(spec.filter(ImageFilter.GaussianBlur(W * 0.004)), 120))
        band = blank()
        x0, y0 = self.x0, self.y0
        ImageDraw.Draw(band).polygon([
            (x0 + W * 0.55, y0 - W * 0.1), (x0 + W * 1.2, y0 - W * 0.1),
            (x0 - W * 0.1, y0 + self.H * 0.75), (x0 - W * 0.1, y0 + self.H * 0.42),
        ], fill=255)
        band = band.filter(ImageFilter.GaussianBlur(W * 0.18))
        strength = {"glass": 60, "matte": 26, "leather": 14}[self.finish]
        self.over((255, 255, 255), ImageChops.multiply(scale(band, strength), self.body))

    def save(self, name):
        self.finish_up()
        img = self.img.resize((SIZE, SIZE), Image.LANCZOS)
        q = quantize(img)
        os.makedirs(OUT, exist_ok=True)
        path = os.path.join(OUT, name + ".png")
        tmp = path + ".tmp"
        q.save(tmp, format="PNG", optimize=True, transparency=q.info["transparency"])
        os.replace(tmp, path)
        return path


# -- shared layouts -----------------------------------------------------------------------
SILVER = (200, 202, 208)
GRAPHITE = (70, 72, 78)
GLASS = (26, 27, 32)


def column(p, u, vs, r, ring=SILVER):
    for v in vs:
        p.lens(u, v, r, ring)


def samsung_s(color, ring, flash_u=0.37, r=0.083):
    p = Phone(2.06, 0.13, color)
    column(p, 0.2, (0.2, 0.41, 0.62), r, ring)
    p.flash(flash_u, 0.2, 0.032)
    return p


def samsung_ultra(color, frame, radius, ring):
    p = Phone(2.1, radius, color, frame=frame)
    column(p, 0.18, (0.19, 0.41, 0.63), 0.088, ring)
    p.lens(0.38, 0.30, 0.07, ring)
    p.lens(0.38, 0.52, 0.07, ring)
    p.flash(0.54, 0.19, 0.03)
    p.dot(0.54, 0.30, 0.022)
    return p


def contour(p, color, right=0.36, bottom=0.78):
    """Samsung's camera housing that flows into the frame (S21, S22)."""
    m = p.module(-0.2, -0.2, right, bottom, 0.12, color, clip=True)
    return m


# -- the drawings -------------------------------------------------------------------------
def galaxy_s20():
    p = Phone(2.08, 0.13, (176, 200, 226))
    p.module(0.07, 0.07, 0.34, 0.72, 0.08, GLASS)
    column(p, 0.205, (0.2, 0.395, 0.59), 0.075, (90, 92, 100))
    p.flash(0.44, 0.16, 0.03)
    return p


def galaxy_s20_ultra():
    p = Phone(2.1, 0.12, (96, 98, 104))
    p.module(0.06, 0.06, 0.55, 0.98, 0.08, GLASS)
    p.lens(0.19, 0.21, 0.095, (110, 112, 120))
    p.lens(0.19, 0.48, 0.08, (110, 112, 120))
    p.lens(0.42, 0.21, 0.07, (110, 112, 120))
    p.square_lens(0.19, 0.80, 0.09, (110, 112, 120))
    p.flash(0.42, 0.42, 0.03)
    p.dot(0.42, 0.55, 0.025, (50, 50, 56))
    return p


def galaxy_s21():
    p = Phone(2.07, 0.13, (172, 146, 200), frame=(214, 168, 128))
    contour(p, (214, 168, 128))
    column(p, 0.17, (0.17, 0.38, 0.59), 0.078, (40, 40, 44))
    p.flash(0.46, 0.14, 0.03)
    return p


def galaxy_s21_ultra():
    p = Phone(2.1, 0.12, (196, 198, 206), frame=(170, 172, 180))
    contour(p, (150, 152, 162), right=0.5, bottom=0.95)
    column(p, 0.16, (0.18, 0.43, 0.68), 0.088, (24, 24, 28))
    p.square_lens(0.37, 0.44, 0.075, (24, 24, 28))
    p.lens(0.37, 0.70, 0.06, (24, 24, 28))
    p.flash(0.37, 0.16, 0.03)
    p.dot(0.37, 0.27, 0.022)
    return p


def galaxy_s22():
    p = Phone(2.06, 0.13, (66, 98, 84))
    contour(p, (58, 88, 76))
    column(p, 0.17, (0.17, 0.38, 0.59), 0.078, (130, 150, 140))
    p.flash(0.46, 0.14, 0.03)
    return p


def galaxy_s22_ultra():
    return samsung_ultra((122, 34, 50), (150, 90, 96), 0.045, (150, 90, 96))


def galaxy_s23():
    return samsung_s((236, 226, 204), (220, 206, 180))


def galaxy_s23_ultra():
    return samsung_ultra((118, 134, 112), (150, 160, 140), 0.05, (150, 160, 140))


def galaxy_s24():
    return samsung_s((156, 146, 212), (140, 130, 196))


def galaxy_s24_ultra():
    return samsung_ultra((124, 124, 122), (150, 150, 148), 0.035, (110, 110, 110))


def galaxy_s25():
    return samsung_s((196, 220, 236), (40, 44, 52), r=0.088)


def galaxy_s25_ultra():
    return samsung_ultra((150, 166, 188), (176, 186, 200), 0.10, (40, 44, 52))


def galaxy_note20():
    p = Phone(2.13, 0.04, (196, 156, 130), finish="matte")
    p.module(0.06, 0.06, 0.52, 0.82, 0.06, (52, 40, 36))
    column(p, 0.2, (0.2, 0.45, 0.70), 0.1, (190, 150, 120))
    p.flash(0.42, 0.18, 0.03)
    p.dot(0.42, 0.32, 0.025, (60, 50, 46))
    p.dot(0.42, 0.44, 0.025, (60, 50, 46))
    return p


def galaxy_z_fold():
    p = Phone(2.34, 0.10, (178, 200, 222))
    # Hinge along the left edge.
    p.module(-0.1, -0.1, 0.06, 3, 0.0, (118, 124, 136), raised=False)
    column(p, 0.24, (0.2, 0.42, 0.64), 0.085, (60, 66, 76))
    p.flash(0.44, 0.2, 0.03)
    return p


def galaxy_z_flip():
    p = Phone(1.16, 0.15, (180, 222, 200), height=0.56)
    p.screen(0.05, 0.05, 0.95, 1.11, 0.12)
    p.lens(0.2, 0.86, 0.085, (150, 196, 176))
    p.lens(0.42, 0.86, 0.085, (150, 196, 176))
    return p


def galaxy_z_flip_classic():
    p = Phone(1.16, 0.15, (238, 226, 196), height=0.56)
    p.band(-0.1, 0.42, (24, 24, 28))
    p.screen(0.48, 0.1, 0.9, 0.32, 0.06, (50, 60, 120), (120, 60, 160))
    p.lens(0.14, 0.21, 0.075, (60, 60, 66))
    p.lens(0.33, 0.21, 0.075, (60, 60, 66))
    p.band(1.1, 1.2, (170, 168, 160))
    return p


def galaxy_a():
    p = Phone(2.1, 0.12, (212, 232, 150))
    column(p, 0.18, (0.18, 0.37, 0.56), 0.075, (196, 216, 136))
    p.flash(0.35, 0.18, 0.028)
    return p


def galaxy_a_classic():
    p = Phone(2.12, 0.12, (90, 140, 212))
    p.module(0.07, 0.07, 0.45, 0.62, 0.09, GLASS)
    p.lens(0.19, 0.2, 0.082, (80, 84, 92))
    p.lens(0.35, 0.2, 0.06, (80, 84, 92))
    p.lens(0.19, 0.44, 0.06, (80, 84, 92))
    p.lens(0.35, 0.40, 0.045, (80, 84, 92))
    p.flash(0.35, 0.53, 0.028)
    return p


def xiaomi_ultra():
    p = Phone(2.1, 0.14, (34, 34, 36), finish="leather")
    p.band(-0.1, 1.06, (70, 72, 76))
    p.disc(0.5, 0.52, 0.38, (20, 20, 24), ring=(196, 198, 204), ring_w=0.04)
    for du, dv in ((-0.14, -0.14), (0.14, -0.14), (-0.14, 0.14), (0.14, 0.14)):
        p.lens(0.5 + du, 0.52 + dv, 0.105, (60, 62, 68))
    p.flash(0.82, 0.96, 0.028)
    return p


def xiaomi_square():
    p = Phone(2.1, 0.14, (168, 200, 180))
    p.module(0.07, 0.07, 0.52, 0.52, 0.12, (150, 182, 162))
    p.lens(0.2, 0.2, 0.08, (210, 214, 210))
    p.lens(0.39, 0.2, 0.08, (210, 214, 210))
    p.lens(0.2, 0.39, 0.08, (210, 214, 210))
    p.flash(0.39, 0.39, 0.035)
    return p


def xiaomi_classic():
    p = Phone(2.12, 0.13, (110, 142, 204))
    p.module(0.07, 0.07, 0.46, 0.64, 0.1, GLASS)
    p.lens(0.21, 0.22, 0.105, (80, 84, 92))
    p.lens(0.38, 0.17, 0.05, (80, 84, 92))
    p.lens(0.38, 0.32, 0.05, (80, 84, 92))
    p.lens(0.21, 0.49, 0.06, (80, 84, 92))
    p.flash(0.38, 0.47, 0.028)
    return p


def redmi_note_11():
    p = Phone(2.14, 0.12, (90, 104, 150), color2=(126, 104, 170))
    p.module(0.07, 0.07, 0.47, 0.58, 0.08, (34, 36, 46))
    p.lens(0.2, 0.21, 0.1, (90, 94, 110))
    p.lens(0.2, 0.45, 0.07, (90, 94, 110))
    p.lens(0.38, 0.33, 0.04, (90, 94, 110))
    p.lens(0.38, 0.46, 0.04, (90, 94, 110))
    p.flash(0.38, 0.17, 0.03)
    return p


def redmi_note_13():
    p = Phone(2.15, 0.12, (70, 142, 152))
    p.module(0.07, 0.07, 0.33, 0.78, 0.13, (62, 128, 138))
    column(p, 0.2, (0.22, 0.47), 0.095, (40, 44, 50))
    p.lens(0.2, 0.66, 0.06, (40, 44, 50))
    p.flash(0.42, 0.16, 0.03)
    return p


def redmi_note_14():
    p = Phone(2.15, 0.13, (170, 140, 214), color2=(226, 160, 196))
    p.module(0.07, 0.07, 0.55, 0.55, 0.16, (40, 36, 52))
    p.lens(0.22, 0.22, 0.1, (180, 170, 200))
    p.lens(0.42, 0.21, 0.06, (180, 170, 200))
    p.lens(0.22, 0.42, 0.06, (180, 170, 200))
    p.flash(0.41, 0.41, 0.033)
    return p


def redmi():
    p = Phone(2.17, 0.12, (232, 200, 206))
    column(p, 0.18, (0.19, 0.42), 0.09, (210, 176, 182))
    p.flash(0.36, 0.17, 0.028)
    return p


def poco_x():
    p = Phone(2.15, 0.12, (250, 206, 36))
    p.module(0.06, 0.06, 0.94, 0.4, 0.1, (24, 24, 26))
    for u in (0.2, 0.42):
        p.lens(u, 0.23, 0.085, (70, 72, 76))
    p.lens(0.62, 0.23, 0.055, (70, 72, 76))
    p.flash(0.8, 0.23, 0.032)
    return p


def poco_f():
    p = Phone(2.15, 0.12, (32, 34, 38), frame=(120, 122, 128))
    p.module(0.07, 0.07, 0.36, 0.72, 0.145, (22, 22, 26))
    column(p, 0.215, (0.23, 0.53), 0.11, (190, 192, 198))
    p.flash(0.48, 0.16, 0.03)
    return p


def oppo_find_x():
    p = Phone(2.12, 0.14, (236, 232, 224))
    p.disc(0.5, 0.42, 0.32, (24, 24, 28), ring=(200, 198, 192), ring_w=0.03)
    p.lens(0.38, 0.32, 0.09, (70, 72, 78))
    p.lens(0.62, 0.32, 0.09, (70, 72, 78))
    p.lens(0.38, 0.54, 0.09, (70, 72, 78))
    p.lens(0.62, 0.54, 0.09, (70, 72, 78))
    p.flash(0.86, 0.42, 0.028)
    return p


def oppo_find_x_classic():
    p = Phone(2.15, 0.12, (60, 92, 156), color2=(40, 60, 110))
    p.module(0.06, 0.06, 0.48, 0.66, 0.16, (70, 104, 170), color2=(50, 74, 130))
    p.lens(0.19, 0.2, 0.085, (190, 196, 210))
    p.lens(0.36, 0.2, 0.085, (190, 196, 210))
    p.lens(0.19, 0.43, 0.065, (190, 196, 210))
    p.lens(0.36, 0.43, 0.05, (190, 196, 210))
    p.flash(0.36, 0.56, 0.028)
    return p


def oppo_reno():
    p = Phone(2.15, 0.13, (150, 204, 196), color2=(196, 176, 228))
    p.module(0.07, 0.07, 0.31, 0.78, 0.12, (34, 38, 44))
    column(p, 0.19, (0.21, 0.46), 0.09, (200, 205, 214))
    p.lens(0.19, 0.66, 0.055, (200, 205, 214))
    p.flash(0.41, 0.16, 0.03)
    return p


def vivo_x():
    p = Phone(2.12, 0.14, (44, 84, 164), color2=(24, 40, 96))
    p.disc(0.5, 0.48, 0.4, (16, 16, 20), ring=(200, 202, 210), ring_w=0.025)
    p.lens(0.5, 0.32, 0.1, (54, 56, 62))
    p.lens(0.34, 0.56, 0.1, (54, 56, 62))
    p.lens(0.66, 0.56, 0.1, (54, 56, 62))
    p.flash(0.5, 0.72, 0.03)
    return p


def vivo_x_classic():
    p = Phone(2.18, 0.12, (116, 156, 206), color2=(160, 140, 210))
    p.module(0.06, 0.06, 0.46, 0.84, 0.08, (180, 196, 222), color2=(120, 140, 190))
    p.module(0.1, 0.1, 0.42, 0.42, 0.16, GLASS)
    p.lens(0.26, 0.26, 0.12, (90, 94, 104))
    p.lens(0.17, 0.55, 0.06, (90, 94, 104))
    p.lens(0.35, 0.55, 0.06, (90, 94, 104))
    p.lens(0.17, 0.72, 0.05, (90, 94, 104))
    p.flash(0.35, 0.72, 0.028)
    return p


def vivo_v():
    p = Phone(2.2, 0.13, (60, 130, 120), color2=(36, 92, 96))
    p.module(0.07, 0.07, 0.33, 0.6, 0.13, (40, 44, 48))
    column(p, 0.2, (0.2, 0.46), 0.095, (180, 186, 190))
    m = rrect(p.box(0.07, 0.66, 0.33, 0.86), 0.06 * p.W)
    p.over((255, 236, 200), scale(m.filter(ImageFilter.GaussianBlur(p.W * 0.01)), 230))
    p.rim_light(m, 2)
    return p


def oneplus():
    p = Phone(2.18, 0.13, (40, 104, 84), color2=(22, 66, 56))
    # Round housing joined to the left edge of the frame.
    p.disc(0.3, 0.35, 0.27, (24, 26, 28), ring=(170, 174, 170), ring_w=0.03)
    p.lens(0.22, 0.27, 0.075, (60, 62, 66))
    p.lens(0.38, 0.27, 0.075, (60, 62, 66))
    p.lens(0.22, 0.43, 0.075, (60, 62, 66))
    p.flash(0.38, 0.43, 0.032)
    return p


def oneplus_classic():
    p = Phone(2.2, 0.12, (200, 206, 214))
    p.module(0.07, 0.07, 0.42, 0.62, 0.1, (34, 36, 40))
    p.lens(0.2, 0.2, 0.08, (180, 184, 190))
    p.lens(0.2, 0.42, 0.08, (180, 184, 190))
    p.lens(0.34, 0.31, 0.045, (180, 184, 190))
    p.dot(0.34, 0.48, 0.02, (60, 60, 64))
    p.flash(0.34, 0.16, 0.028)
    return p


def oneplus_nord():
    p = Phone(2.2, 0.12, (58, 108, 206), color2=(40, 70, 160))
    p.module(0.07, 0.07, 0.32, 0.33, 0.13, (34, 36, 44))
    p.module(0.07, 0.38, 0.32, 0.64, 0.13, (34, 36, 44))
    p.lens(0.195, 0.2, 0.095, (170, 176, 190))
    p.lens(0.195, 0.51, 0.095, (170, 176, 190))
    p.flash(0.42, 0.16, 0.03)
    return p


def pixel_5():
    p = Phone(2.05, 0.11, (166, 186, 160), finish="matte")
    p.module(0.07, 0.07, 0.4, 0.4, 0.09, (22, 22, 24))
    p.lens(0.17, 0.17, 0.06, (60, 60, 64))
    p.lens(0.3, 0.3, 0.06, (60, 60, 64))
    p.flash(0.3, 0.17, 0.026)
    p.dot(0.17, 0.3, 0.018, (60, 60, 64))
    return p


def pixel_6():
    p = Phone(2.06, 0.1, (232, 136, 112))
    p.band(-0.1, 0.34, (244, 204, 190))
    p.band(0.34, 0.54, (22, 22, 24), raised=True)
    p.module(0.12, 0.37, 0.48, 0.51, 0.07, (12, 12, 14), raised=False)
    p.lens(0.22, 0.44, 0.058, None)
    p.lens(0.38, 0.44, 0.058, None)
    p.flash(0.62, 0.44, 0.026)
    return p


def pixel_7():
    p = Phone(2.1, 0.11, (218, 216, 152))
    p.band(0.3, 0.52, (214, 212, 196), raised=True)
    p.module(0.1, 0.35, 0.56, 0.47, 0.06, (14, 14, 16), raised=False)
    p.lens(0.2, 0.41, 0.05, None)
    p.lens(0.33, 0.41, 0.05, None)
    p.lens(0.46, 0.41, 0.04, None)
    p.flash(0.72, 0.41, 0.024)
    return p


def pixel_9():
    p = Phone(2.1, 0.13, (238, 172, 202))
    p.module(0.1, 0.18, 0.9, 0.46, 0.14, (226, 222, 222))
    p.module(0.15, 0.23, 0.66, 0.41, 0.09, (14, 14, 16), raised=False)
    p.lens(0.26, 0.32, 0.06, None)
    p.lens(0.43, 0.32, 0.06, None)
    p.flash(0.58, 0.32, 0.024)
    p.dot(0.78, 0.32, 0.014, (120, 116, 118))
    return p


def pixel_9a():
    p = Phone(2.1, 0.13, (156, 150, 214), finish="matte")
    p.module(0.07, 0.09, 0.47, 0.25, 0.08, (24, 24, 30), raised=False)
    p.lens(0.17, 0.17, 0.055, (130, 126, 180))
    p.lens(0.34, 0.17, 0.055, (130, 126, 180))
    p.flash(0.58, 0.17, 0.024)
    return p


def pixel_fold():
    p = Phone(1.76, 0.1, (236, 232, 224), height=0.66)
    p.band(0.16, 0.36, (196, 198, 202), raised=True)
    p.module(0.08, 0.2, 0.5, 0.32, 0.06, (14, 14, 16), raised=False)
    p.lens(0.16, 0.26, 0.045, None)
    p.lens(0.28, 0.26, 0.045, None)
    p.lens(0.40, 0.26, 0.045, None)
    p.flash(0.66, 0.26, 0.022)
    return p


def realme():
    p = Phone(2.2, 0.13, (212, 192, 160), finish="leather")
    p.disc(0.5, 0.42, 0.3, (26, 26, 28), ring=(212, 180, 120), ring_w=0.035)
    p.lens(0.5, 0.29, 0.085, (70, 70, 74))
    p.lens(0.38, 0.5, 0.085, (70, 70, 74))
    p.lens(0.62, 0.5, 0.085, (70, 70, 74))
    p.flash(0.82, 0.18, 0.026)
    return p


def realme_gt():
    p = Phone(2.18, 0.12, (240, 200, 40), finish="leather")
    p.module(0.6, -0.2, 0.72, 3, 0.0, (28, 28, 30), raised=False)
    p.module(0.07, 0.07, 0.38, 0.62, 0.11, (28, 28, 30))
    column(p, 0.225, (0.22, 0.46), 0.1, (90, 92, 96))
    p.flash(0.48, 0.16, 0.028)
    return p


def honor():
    p = Phone(2.15, 0.14, (110, 190, 200), color2=(70, 140, 170))
    p.module(0.2, 0.12, 0.8, 0.72, 0.2, (190, 194, 200))
    p.module(0.24, 0.16, 0.76, 0.68, 0.17, (22, 22, 26), raised=False)
    p.lens(0.38, 0.30, 0.085, (70, 72, 78))
    p.lens(0.62, 0.30, 0.085, (70, 72, 78))
    p.lens(0.38, 0.54, 0.085, (70, 72, 78))
    p.lens(0.62, 0.54, 0.07, (70, 72, 78))
    p.flash(0.5, 0.42, 0.025)
    return p


def moto_edge():
    p = Phone(2.2, 0.14, (240, 168, 140), finish="leather")
    p.module(0.07, 0.07, 0.42, 0.66, 0.1, (246, 186, 162))
    column(p, 0.18, (0.2, 0.42), 0.085, (70, 70, 74))
    p.lens(0.33, 0.31, 0.07, (70, 70, 74))
    p.flash(0.33, 0.52, 0.028)
    return p


def moto_razr():
    p = Phone(1.2, 0.13, (184, 40, 92), finish="leather", height=0.56)
    p.screen(0.03, 0.03, 0.97, 1.0, 0.11, (180, 60, 120), (80, 60, 200))
    p.lens(0.14, 0.14, 0.07, (60, 60, 64))
    p.lens(0.32, 0.14, 0.07, (60, 60, 64))
    p.flash(0.47, 0.14, 0.022)
    p.band(1.08, 1.3, (160, 160, 166))
    return p


def nothing_internals(p, tone, deep):
    """What a see-through back shows: plates, screws and the charging coil."""
    p.module(0.12, 0.6, 0.88, 1.3, 0.05, tone, raised=False)
    p.module(0.08, 1.45, 0.92, 1.95, 0.05, tone, raised=False)
    for u, v in ((0.16, 0.66), (0.84, 0.66), (0.16, 1.24), (0.84, 1.24), (0.14, 1.52), (0.86, 1.88)):
        p.dot(u, v, 0.016, deep)
    cx, cy = p.P(0.5, 0.95)
    R = 0.23 * p.W
    coil = ImageChops.subtract(circle(cx, cy, R), circle(cx, cy, R * 0.55))
    p.over(deep, scale(coil, 70))


def nothing():
    p = Phone(2.16, 0.12, (232, 233, 236))
    nothing_internals(p, (214, 216, 220), (150, 152, 158))
    p.module(0.07, 0.07, 0.45, 0.3, 0.1, (26, 26, 30))
    p.lens(0.18, 0.18, 0.075, (90, 92, 98))
    p.lens(0.34, 0.18, 0.075, (90, 92, 98))
    case = (186, 188, 196)
    p.arc(0.5, 0.95, 0.3, 0, 360, 0.022, casing=case)
    p.line([(0.6, 0.12), (0.8, 0.32)], 0.022, casing=case)
    p.line([(0.5, 1.6), (0.5, 1.82)], 0.022, casing=case)
    p.dot(0.5, 1.9, 0.015, (255, 255, 255))
    return p


def nothing_dark():
    p = Phone(2.16, 0.12, (52, 52, 56))
    nothing_internals(p, (66, 66, 72), (110, 110, 118))
    p.module(0.28, 0.2, 0.72, 0.4, 0.1, (16, 16, 18))
    p.lens(0.4, 0.3, 0.075, (100, 102, 110))
    p.lens(0.6, 0.3, 0.075, (100, 102, 110))
    p.arc(0.5, 0.3, 0.33, 200, 340, 0.022)
    p.arc(0.5, 0.3, 0.33, 20, 160, 0.022)
    p.line([(0.5, 1.7), (0.5, 1.85)], 0.022)
    return p


ART = {
    "galaxy-s20": galaxy_s20,
    "galaxy-s20-ultra": galaxy_s20_ultra,
    "galaxy-s21": galaxy_s21,
    "galaxy-s21-ultra": galaxy_s21_ultra,
    "galaxy-s22": galaxy_s22,
    "galaxy-s22-ultra": galaxy_s22_ultra,
    "galaxy-s23": galaxy_s23,
    "galaxy-s23-ultra": galaxy_s23_ultra,
    "galaxy-s24": galaxy_s24,
    "galaxy-s24-ultra": galaxy_s24_ultra,
    "galaxy-s25": galaxy_s25,
    "galaxy-s25-ultra": galaxy_s25_ultra,
    "galaxy-note20": galaxy_note20,
    "galaxy-z-fold": galaxy_z_fold,
    "galaxy-z-flip": galaxy_z_flip,
    "galaxy-z-flip-classic": galaxy_z_flip_classic,
    "galaxy-a": galaxy_a,
    "galaxy-a-classic": galaxy_a_classic,
    "xiaomi-ultra": xiaomi_ultra,
    "xiaomi-square": xiaomi_square,
    "xiaomi-classic": xiaomi_classic,
    "redmi-note-11": redmi_note_11,
    "redmi-note-13": redmi_note_13,
    "redmi-note-14": redmi_note_14,
    "redmi": redmi,
    "poco-x": poco_x,
    "poco-f": poco_f,
    "oppo-find-x": oppo_find_x,
    "oppo-find-x-classic": oppo_find_x_classic,
    "oppo-reno": oppo_reno,
    "vivo-x": vivo_x,
    "vivo-x-classic": vivo_x_classic,
    "vivo-v": vivo_v,
    "oneplus": oneplus,
    "oneplus-classic": oneplus_classic,
    "oneplus-nord": oneplus_nord,
    "pixel-5": pixel_5,
    "pixel-6": pixel_6,
    "pixel-7": pixel_7,
    "pixel-9": pixel_9,
    "pixel-9a": pixel_9a,
    "pixel-fold": pixel_fold,
    "realme": realme,
    "realme-gt": realme_gt,
    "honor": honor,
    "moto-edge": moto_edge,
    "moto-razr": moto_razr,
    "nothing": nothing,
    "nothing-dark": nothing_dark,
}


def main():
    names = sys.argv[1:] or list(ART)
    for name in names:
        path = ART[name]().save(name)
        print(f"{os.path.relpath(path, ROOT)}  {os.path.getsize(path) // 1024} KB")


if __name__ == "__main__":
    main()
