#!/usr/bin/env python3
"""Draws the images of the web demo and writes them to web/samples/.

    cargo build --release -p vitral-cli
    python3 tools/demo_samples.py

Each image is authored as a stack of blend-mode layers, as in a design tool,
flattened into the three exports a designer would make (over black, over
white and over transparency) and encoded with `vitral encode`. Only modes
Vitral keeps exactly are used: normal, multiply, screen and difference with
white. Images are drawn at twice the size the page shows them.

The capsule at the top of the page is also written as its three exports,
which the encoder section of the page starts from, and as capsule.png. Its
plain PNG shows only the intention: the blend layers as flat, half-transparent
colour. The sunglasses do the same, so their lenses are see-through in both.

Needs numpy and Pillow, and the Inter typeface for the film's edge printing.
The output is deterministic. The backdrops come from tools/demo_backdrops.py.
"""

import os
import subprocess
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "web" / "samples"
VITRAL = os.environ.get("VITRAL", str(ROOT / "target" / "release" / "vitral"))
INTER = Path("/usr/share/fonts/opentype/inter")

WHITE = (255, 255, 255)
INK = (20, 20, 22)
MAGENTA = (236, 40, 132)
GREEN = (24, 150, 92)

# ------------------------------------------------------------------ shapes ---
# Signed distances are positive inside; coverage antialiases them over a pixel.


def cov(d):
    return np.clip(d + 0.5, 0, 1)


def smooth(d, soft):
    t = np.clip(d / soft + 0.5, 0, 1)
    return t * t * (3 - 2 * t)


def circle(x, y, cx, cy, r):
    return r - np.hypot(x - cx, y - cy)


def rect(x, y, x0, y0, x1, y1):
    return np.minimum(np.minimum(x - x0, x1 - x), np.minimum(y - y0, y1 - y))


def local(x, y, cx, cy, deg):
    """Coordinates of (x, y) in a frame centred on (cx, cy), turned by deg."""
    s, c = np.sin(np.radians(deg)), np.cos(np.radians(deg))
    dx, dy = x - cx, y - cy
    return dx * c + dy * s, -dx * s + dy * c


def box(x, y, cx, cy, hw, hh, deg=0.0, r=0.0):
    """A rounded box of half size (hw, hh), turned by deg."""
    u, v = local(x, y, cx, cy, deg)
    qx, qy = np.abs(u) - hw + r, np.abs(v) - hh + r
    return -(np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r)


def seg(x, y, ax, ay, bx, by):
    """Distance to the segment from a to b."""
    px, py, dx, dy = x - ax, y - ay, bx - ax, by - ay
    t = np.clip((px * dx + py * dy) / (dx * dx + dy * dy), 0, 1)
    return np.hypot(px - dx * t, py - dy * t)


def stripes(y, period=56, thick=28, start=0.0):
    return cov(thick / 2 - np.abs(np.mod(y - start, period) - period / 2))


def _hash(x, y):
    """A repeatable pseudo-random value in [0, 1) for integer lattice points."""
    m = np.uint64(0xFFFFFFFF)
    x = np.asarray(x, np.int64).astype(np.uint64) & m
    y = np.asarray(y, np.int64).astype(np.uint64) & m
    h = ((x * np.uint64(374_761_393)) & m) ^ ((y * np.uint64(668_265_263)) & m)
    h = ((h ^ (h >> np.uint64(13))) * np.uint64(1_274_126_177)) & m
    return (h ^ (h >> np.uint64(16))).astype(np.float64) / 4_294_967_296.0


def noise(x, y):
    """Smooth value noise in [0, 1], varying over one unit."""
    ix, iy = np.floor(x), np.floor(y)
    fx, fy = x - ix, y - iy
    sx, sy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    ix, iy = ix.astype(np.int64), iy.astype(np.int64)
    top = _hash(ix, iy) + (_hash(ix + 1, iy) - _hash(ix, iy)) * sx
    bottom = _hash(ix, iy + 1) + (_hash(ix + 1, iy + 1) - _hash(ix, iy + 1)) * sx
    return (top + (bottom - top) * sy).astype(np.float32)


def blotches(w, h, scale, seed):
    """Smooth random variation in [0, 1] over about `scale` pixels."""
    gw, gh = int(w / scale) + 3, int(h / scale) + 3
    g = Image.fromarray((np.random.default_rng(seed).random((gh, gw)) * 255).astype(np.uint8))
    big = g.resize((int(gw * scale), int(gh * scale)), Image.BICUBIC)
    return np.asarray(big, np.float32)[:h, :w] / 255


def text_mask(w, h, xy, text, face, size):
    img = Image.new("L", (w, h), 0)
    try:
        font = ImageFont.truetype(str(INTER / f"{face}.otf"), size)
    except OSError:
        font = ImageFont.load_default(size)
    ImageDraw.Draw(img).text(xy, text, fill=255, font=font)
    return np.asarray(img, np.float32) / 255


def lookup(mask, x, y):
    """Bilinear lookup of a mask at fractional pixel coordinates."""
    h, w = mask.shape
    x = np.clip(x - 0.5, 0, w - 1.001)
    y = np.clip(y - 0.5, 0, h - 1.001)
    x0, y0 = x.astype(int), y.astype(int)
    fx, fy = x - x0, y - y0
    top = mask[y0, x0] * (1 - fx) + mask[y0, x0 + 1] * fx
    bottom = mask[y0 + 1, x0] * (1 - fx) + mask[y0 + 1, x0 + 1] * fx
    return top * (1 - fy) + bottom * fy


# ------------------------------------------------------------------ layers ---


def blend(mode, b, c):
    if mode == "normal":
        return np.broadcast_to(c, b.shape)
    if mode == "multiply":
        return b * c
    if mode == "screen":
        return b + c - b * c
    if mode == "difference":
        return np.abs(b - c)
    raise ValueError(mode)


class Art:
    """A stack of layers, bottom first, each a colour, an alpha and a mode."""

    def __init__(self, w, h):
        self.w, self.h = w, h
        ys, xs = np.mgrid[0:h, 0:w].astype(np.float32) + 0.5
        self.x, self.y = xs, ys
        self.layers = []

    def add(self, colour, alpha, mode="normal", intent=None):
        """intent: this layer's opacity in the 'intention' PNG; by default
        blend layers fade to 55% and normal ones stay as they are."""
        c = np.asarray(colour, np.float32) / 255
        c = np.broadcast_to(c, (self.h, self.w, 3)) if c.ndim == 1 else c
        a = np.clip(np.broadcast_to(np.asarray(alpha, np.float32), (self.h, self.w)), 0, 1)
        if intent is None:
            intent = 1.0 if mode == "normal" else 0.55
        self.layers.append((c, a, mode, intent))

    def over(self, backdrop):
        b = np.full((self.h, self.w, 3), backdrop, np.float32)
        for c, a, mode, _ in self.layers:
            b = b * (1 - a[..., None]) + a[..., None] * blend(mode, b, c)
        return b

    def native(self, intention):
        """Over transparency, as a design tool exports it (blend modes act on
        what earlier layers left), or as the intention: plain, faded layers."""
        co = np.zeros((self.h, self.w, 3), np.float32)
        ao = np.zeros((self.h, self.w), np.float32)
        for c, a, mode, intent in self.layers:
            if intention:
                a, mode = a * intent, "normal"
            cb = co / np.maximum(ao, 1e-6)[..., None]
            a3, ab = a[..., None], ao[..., None]
            co = c * a3 * (1 - ab) + a3 * ab * blend(mode, cb, c) + co * (1 - a3)
            ao = a + ao * (1 - a)
        return np.dstack([co / np.maximum(ao, 1e-6)[..., None], ao])

    def exports(self, intention=False):
        to8 = lambda a: Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8))
        return to8(self.over(0.0)), to8(self.over(1.0)), to8(self.native(intention))


def write(name, art, intention=False, sources=False):
    black, white, transparent = art.exports(intention)
    out = OUT / f"{name}.vitral.png"
    with tempfile.TemporaryDirectory() as tmp:
        paths = [Path(tmp) / f"{k}.png" for k in ("black", "white", "transparent")]
        for img, p in zip((black, white, transparent), paths):
            img.save(p)
        subprocess.run([VITRAL, "encode", "--black", paths[0], "--white", paths[1],
                        "--transparent", paths[2], "-o", out], check=True, capture_output=True)
    if sources:
        transparent.save(OUT / f"{name}.png", optimize=True)
        for img, k in zip((black, white, transparent), ("black", "white", "transparent")):
            img.save(OUT / "sources" / f"{name}-over-{k}.png", optimize=True)
    print(f"{name:14} {art.w:>4}x{art.h:<4} {out.stat().st_size:>7} bytes")


def soft_shadow(a, d, dx=10, dy=14, soft=20, strength=0.3, intent=1.0):
    """A soft multiply shadow of the shape d, offset on the page."""
    cast = smooth(np.roll(np.roll(d, dy, axis=0), dx, axis=1), soft)
    a.add((70, 72, 86), cast * strength * (1 - cov(d)), "multiply", intent)


# ----------------------------------------------------------------- capsule ---


def capsule():
    """The object dropped on the page at the top: a capsule turned by 45
    degrees, bands that invert across its top half and a disc of magenta ink
    in multiply filling its lower end, with a green square printed inside."""
    a = Art(640, 640)
    shape = lambda x, y: box(*[c + 320 for c in local(x, y, 320, 320, -45)], 320, 320, 150, 250, 0, 150)
    x, y = [c + 320 for c in local(a.x, a.y, 320, 320, -45)]
    d = shape(a.x, a.y)
    cast = smooth(shape(a.x - 14, a.y - 20), 28)
    a.add((70, 72, 86), cast * 0.34 * (1 - cov(d)), "multiply", intent=1.0)
    inside = cov(d)
    a.add(WHITE, stripes(y, start=70) * inside * cov(305 - y), "difference")
    a.add(MAGENTA, cov(circle(x, y, 320, 420, 150)) * inside, "multiply")
    a.add(GREEN, cov(rect(x, y, 260, 360, 380, 480)), "multiply")
    a.add(INK, cov(d + 1.5) * cov(1.5 - d), "normal")
    return a


# ----------------------------------------------------------------- samples ---


def overprint():
    """Three discs of process ink in multiply, overprinted like a risograph."""
    a = Art(680, 640)
    for cx, cy, ink in ((230, 220, (0, 164, 222)), (450, 220, MAGENTA), (340, 420, (255, 214, 0))):
        a.add(ink, cov(circle(a.x, a.y, cx, cy, 190)), "multiply")
    return a


def highlighter():
    """Three strokes of highlighter in multiply."""
    a = Art(720, 440)
    alpha = np.zeros((a.h, a.w), np.float32)
    colour = np.zeros((a.h, a.w, 3), np.float32)
    for cx, cy, half, deg, ink in ((330, 110, 280, -2, (255, 226, 0)), (300, 220, 230, 1.5, (255, 226, 0)),
                                   (390, 330, 300, -1, (255, 128, 176))):
        s = cov(box(a.x, a.y, cx, cy, half, 30, deg, 10)) * 0.92
        colour = np.where((s > alpha)[..., None], np.asarray(ink, np.float32), colour)
        alpha = np.maximum(alpha, s)
    a.add(colour, alpha, "multiply")
    return a


def lens_tint(a, inside, top, bottom, y0, y1):
    t = np.clip((a.y - y0) / (y1 - y0), 0, 1)[..., None]
    a.add(np.asarray(top, np.float32) * (1 - t) + np.asarray(bottom, np.float32) * t, inside, "multiply")


def sunglasses():
    """Sunglasses: a heavy black frame, bottle-green lenses in multiply and a
    glint of light in screen."""
    a = Art(720, 300)
    d = np.full((a.h, a.w), -1e9, np.float32)
    for cx in (180, 540):
        u, v = a.x - cx, a.y - 150
        hw = 130 - 24 * np.clip((v + 86) / 172, 0, 1)
        r = np.where(v < 0, 18, 70)
        qx, qy = np.abs(u) - hw + r, np.abs(v) - 86 + r
        d = np.maximum(d, -(np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r))
    inside = cov(d)
    lens_tint(a, inside, (40, 66, 56), (110, 136, 126), 64, 236)
    u = a.x + 0.6 * a.y
    glint = sum(cov(16 - np.abs(u - o)) + cov(5 - np.abs(u - o - 36)) for o in (320, 680))
    a.add(WHITE, np.clip(glint, 0, 1) * cov(d - 10) * 0.7, "screen")
    frame = np.maximum.reduce([cov(d + 18), cov(box(a.x, a.y, 360, 74, 336, 18, 0, 8)),
                               cov(box(a.x, a.y, 22, 84, 22, 12)), cov(box(a.x, a.y, 698, 84, 22, 12))])
    a.add(INK, frame * (1 - inside), "normal")
    return a


def glasses_3d():
    """3D glasses: a thick black frame, a red and a cyan lens in multiply."""
    a = Art(720, 300)
    left = box(a.x, a.y, 200, 150, 122, 80, 0, 30)
    right = box(a.x, a.y, 520, 150, 122, 80, 0, 30)
    lens = np.maximum(left, right)
    a.add((230, 34, 46), cov(left), "multiply")
    a.add((0, 184, 232), cov(right), "multiply")
    rim = cov(lens + 16) * (1 - cov(lens))
    bridge = cov(box(a.x, a.y, 360, 104, 50, 12))
    hinges = np.maximum(cov(box(a.x, a.y, 50, 104, 20, 10)), cov(box(a.x, a.y, 670, 104, 20, 10)))
    a.add(INK, np.maximum.reduce([rim, bridge, hinges]), "normal")
    return a


def film():
    """A strip of 35 mm colour negative, turned a little: frames that invert
    what is behind them under the orange base in multiply, sprocket holes cut
    out, and the numbers printed along its edges."""
    a = Art(800, 440)
    u, v = local(a.x, a.y, 400, 220, -4)
    x, y = u + 380, v + 180  # coordinates along the strip
    strip = rect(x, y, 24, 34, 736, 326)
    soft_shadow(a, strip)
    on = cov(strip)
    frames = np.maximum(cov(box(x, y, 220, 180, 152, 102, 0, 6)), cov(box(x, y, 544, 180, 152, 102, 0, 6)))
    pitch = 40.7
    cu = np.floor((x - 30) / pitch) * pitch + 30 + pitch / 2
    holes = np.maximum(cov(box(x, y, cu, 58, 11, 8, 0, 3.5)), cov(box(x, y, cu, 302, 11, 8, 0, 3.5)))
    holes *= cov(rect(x, y, 30, 0, 730, 360))
    a.add(WHITE, frames * on, "difference")
    density = 0.9 + 0.1 * blotches(a.w, a.h, 80, 3)
    a.add((206, 122, 66), on * (1 - holes) * density, "multiply")
    edge = text_mask(760, 360, (60, 40), "14       14A       400-5       15       15A       16", "Inter-SemiBold", 11)
    edge += text_mask(760, 360, (90, 312), "14A              15              15A              16", "Inter-SemiBold", 11)
    a.add((96, 44, 18), np.clip(lookup(edge, x, y), 0, 1) * on * 0.85, "multiply")
    return a


def halftone():
    """A disc of magenta halftone dots in multiply, large in the middle and
    fading out, on a screen turned by 15 degrees."""
    a = Art(600, 600)
    u, v = local(a.x, a.y, 300, 300, 15)
    cu, cv = np.round(u / 24) * 24, np.round(v / 24) * 24
    tone = np.clip(1 - np.hypot(cu, cv) / 280, 0, 1)
    a.add(MAGENTA, cov(24 * 0.56 * np.sqrt(tone) - np.hypot(u - cu, v - cv)), "multiply")
    return a


def spray(a, d, colour, seed):
    """Paint sprayed through a stencil cut to the shape d, with overspray."""
    body = smooth(d, 3)
    even = 0.78 + 0.22 * blotches(a.w, a.h, 40, seed)
    grain = 0.88 + 0.12 * blotches(a.w, a.h, 2.5, seed + 1)
    blur = Image.fromarray((body * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(16))
    halo = np.clip(np.asarray(blur, np.float32) / 255 * 2.2, 0, 1)
    speck = (np.random.default_rng(seed + 3).random(body.shape) < halo * 0.5) * 0.8
    a.add(colour, np.maximum(body * even * grain, speck * (1 - body)), "multiply")


def stencil():
    """Three bars sprayed in orange through a stencil, a little crooked."""
    a = Art(640, 420)
    d = np.maximum.reduce([box(a.x, a.y, 320, cy, 240, 34, -3) for cy in (110, 210, 310)])
    spray(a, d, (244, 96, 30), 3)
    return a


def neon():
    """A neon circle and triangle in screen: tubes with a white-hot core and
    a glow that fades out before the edge of the image."""
    a = Art(800, 460)
    fade = smooth(rect(a.x, a.y, 0, 0, a.w, a.h), 50)

    def tube(d, colour):
        core = cov(4.5 - d)
        glow = (np.exp(-(d / 22) ** 2) * 0.7 + np.exp(-(d / 56) ** 2) * 0.16) * fade
        hot = (core * 0.7)[..., None]
        a.add(np.asarray(colour, np.float32) * (1 - hot) + 255 * hot, np.maximum(core, glow), "screen")

    tube(np.abs(np.hypot(a.x - 270, a.y - 230) - 118), (255, 64, 150))
    pts = [(440, 344), (640, 344), (540, 118)]
    tube(np.minimum.reduce([seg(a.x, a.y, *pts[i], *pts[(i + 1) % 3]) for i in range(3)]), (70, 200, 255))
    return a


def blinds():
    """Afternoon sun through a window with half-open blinds, cast on the wall
    in screen: sharp near the sill and softer as it goes up."""
    a = Art(760, 520)
    t = 470 - a.y
    s = a.x - 70 - 0.5 * t
    soft = 2 + 0.08 * np.maximum(t, 0)
    pane = np.minimum(np.minimum(s, 370 - s), np.minimum(t, 370 - t))
    bars = np.minimum(np.abs(s - 185), np.abs(t - 185)) - 7
    slat = np.abs(np.mod(t + 14, 28) - 14) - 7.5
    lit = smooth(pane, soft) * smooth(bars, soft) * smooth(-slat, soft)
    inside = smooth(rect(a.x, a.y, 0, 0, a.w, a.h) - 10, 40)
    bloom = np.exp(-(np.minimum(pane, 0) / 60) ** 2) * 0.12 * inside
    a.add((255, 204, 150), np.maximum(lit * 0.85, bloom), "screen")
    return a


def tape():
    """Two strips of tape torn off the roll, a cream masking tape and a blue
    painter's tape, in multiply."""
    a = Art(680, 480)
    for cx, cy, deg, ink, seed in ((330, 210, -14, (232, 214, 170), 0), (350, 290, 19, (128, 176, 222), 10)):
        u, v = local(a.x, a.y, cx, cy, deg)
        side = np.where(u > 0, 0, 5) + seed
        end = 270 - 9 * noise(side, v / 6) - 5 * noise(side + 2, v / 1.8)
        d = np.minimum(end - np.abs(u), 42 - np.abs(v))
        fibre = 0.84 + 0.1 * noise(seed + u / 50, v / 2.5) + 0.06 * noise(u / 4, v / 4)
        edge = (1 - 0.1 * cov(np.abs(v) - 38))[..., None]
        a.add(np.asarray(ink, np.float32) * edge, cov(d) * fibre, "multiply")
    return a


def postmark():
    """A rubber-stamp postmark, inked unevenly, in multiply."""
    a = Art(640, 420)
    x, y = a.x, a.y
    r = np.hypot(x - 190, y - 210)
    ring = cov(9 - np.abs(r - 160))
    band = cov(17 - np.abs(y - 210)) * cov(126 - r)
    date = np.maximum.reduce([cov(box(x, y, 190 + dx, 210, 18, 7, 0, 2)) for dx in (-62, -10, 42)])
    disc = np.clip(cov(122 - r) * (1 - band) + date, 0, 1)
    waves = np.maximum.reduce([
        cov(8 - np.abs(y - (100 + 44 * i + 11 * np.sin((x - 330) / 70 * 2 * np.pi)))) * cov(x - 336) * cov(620 - x)
        for i in range(6)])
    grain = 0.7 + 0.3 * noise(x / 5, y / 5)
    missed = np.where(_hash((x // 4).astype(np.int64), (y // 4).astype(np.int64)) < 0.08, 0.3, 1.0)
    a.add((30, 44, 128), np.maximum.reduce([disc, ring, waves]) * grain * missed, "multiply")
    return a


SAMPLES = {
    "overprint": overprint,
    "highlighter": highlighter,
    "sunglasses": sunglasses,
    "glasses-3d": glasses_3d,
    "film": film,
    "halftone": halftone,
    "stencil": stencil,
    "neon": neon,
    "blinds": blinds,
    "tape": tape,
    "postmark": postmark,
}
# Their plain PNG shows the lenses see-through, as a designer would export them.
INTENTION = {"sunglasses", "glasses-3d"}


def main():
    (OUT / "sources").mkdir(parents=True, exist_ok=True)
    write("capsule", capsule(), intention=True, sources=True)
    for name, draw in SAMPLES.items():
        write(name, draw(), intention=name in INTENTION)


if __name__ == "__main__":
    main()
