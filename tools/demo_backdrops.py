#!/usr/bin/env python3
"""Renders the photographic backdrops of the web demo into web/samples/.

    python3 tools/demo_backdrops.py

Needs numpy and Pillow, and the Inter typeface for the backdrops with type
(it falls back to Pillow's default font without it). The output is
deterministic: noise comes from a fixed seed.

  studio.jpg    a small 3D scene: a sphere, a cube and a ball on a cove
  gradient.jpg  a grainy gradient of saturated colour
  poster.jpg    a photocopied poster of the studio scene, with fluorescent ink
  dark.jpg      a website in dark mode, with an image, type and buttons
"""

from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

OUT = Path(__file__).resolve().parent.parent / "web" / "samples"
W, H = 1600, 1000
RNG = np.random.default_rng(7)
INTER = Path("/usr/share/fonts/opentype/inter")


def font(name, size):
    try:
        return ImageFont.truetype(str(INTER / f"{name}.otf"), size)
    except OSError:
        return ImageFont.load_default(size)


def grain(img, amount):
    noise = RNG.normal(0.0, amount, img.shape[:2])[..., None]
    return np.clip(img + noise, 0.0, 1.0)


def to_image(linear):
    """Linear RGB in [0, 1] to an 8-bit sRGB image."""
    c = np.clip(linear, 0.0, 1.0)
    srgb = np.where(c <= 0.0031308, 12.92 * c, 1.055 * c ** (1 / 2.4) - 0.055)
    return Image.fromarray((srgb * 255 + 0.5).astype(np.uint8))


def save(img, name):
    img.save(OUT / name, quality=86, optimize=True, progressive=True)
    print(f"{name:14} {(OUT / name).stat().st_size:>7} bytes")


# ---------------------------------------------------------------- studio ---

SPHERES = [  # centre, radius, linear colour, shininess
    (np.array([-1.15, 0.78, -0.55]), 0.78, np.array([0.62, 0.12, 0.05]), 0.5),
    (np.array([1.55, 0.3, 0.55]), 0.3, np.array([0.02, 0.06, 0.62]), 0.9),
]
BOX = (np.array([0.62, 0.55, -0.95]), np.array([0.55, 0.55, 0.55]), np.radians(32))
BOX_COLOUR = np.array([0.86, 0.80, 0.70])
COVE = np.array([0.62, 0.58, 0.53])
LIGHT = np.array([-0.55, 0.8, 0.45]) / np.linalg.norm([-0.55, 0.8, 0.45])
FAR = 1e9


def hit_sphere(o, d, c, r):
    oc = o - c
    b = np.einsum("ij,ij->i", oc, d)
    q = np.einsum("ij,ij->i", oc, oc) - r * r
    disc = b * b - q
    t = -b - np.sqrt(np.maximum(disc, 0))
    return np.where((disc > 0) & (t > 1e-4), t, FAR)


def box_frame(p, d):
    centre, _, angle = BOX
    ca, sa = np.cos(angle), np.sin(angle)
    rot = np.array([[ca, 0, -sa], [0, 1, 0], [sa, 0, ca]])
    return (p - centre) @ rot.T, d @ rot.T, rot


def hit_box(o, d):
    lo, ld, _ = box_frame(o, d)
    half = BOX[1]
    with np.errstate(divide="ignore", invalid="ignore"):
        t1 = (-half - lo) / ld
        t2 = (half - lo) / ld
    tn = np.nanmax(np.minimum(t1, t2), axis=1)
    tf = np.nanmin(np.maximum(t1, t2), axis=1)
    return np.where((tn < tf) & (tn > 1e-4), tn, FAR)


def box_normal(p):
    lp, _, rot = box_frame(p, p)
    k = np.argmax(np.abs(lp) / BOX[1], axis=1)
    n = np.zeros_like(lp)
    n[np.arange(len(k)), k] = np.sign(lp[np.arange(len(k)), k])
    return n @ rot


def hit_cove(o, d):
    """Floor (y = 0), back wall (z = -3) and the quarter cylinder joining them."""
    with np.errstate(divide="ignore", invalid="ignore"):
        tf = -o[:, 1] / d[:, 1]
        tw = (-3 - o[:, 2]) / d[:, 2]
    pf = o + tf[:, None] * d
    pw = o + tw[:, None] * d
    tf = np.where((tf > 1e-4) & (pf[:, 2] >= -2), tf, FAR)
    tw = np.where((tw > 1e-4) & (pw[:, 1] >= 1), tw, FAR)
    # Cylinder along x through (y, z) = (1, -2), radius 1; the far root is
    # the inside surface the camera sees.
    oy, oz = o[:, 1] - 1, o[:, 2] + 2
    a = d[:, 1] ** 2 + d[:, 2] ** 2
    b = oy * d[:, 1] + oz * d[:, 2]
    c = oy**2 + oz**2 - 1
    disc = b * b - a * c
    tc = (-b + np.sqrt(np.maximum(disc, 0))) / np.maximum(a, 1e-9)
    pc = o + tc[:, None] * d
    tc = np.where((disc > 0) & (tc > 1e-4) & (pc[:, 2] < -2) & (pc[:, 1] < 1), tc, FAR)
    t = np.minimum(np.minimum(tf, tw), tc)
    p = o + t[:, None] * d
    n = np.zeros_like(p)
    n[:, 1] = 1
    wall = t == tw
    n[wall] = [0, 0, 1]
    cyl = t == tc
    n[cyl, 1] = -(p[cyl, 1] - 1)
    n[cyl, 2] = -(p[cyl, 2] + 2)
    return t, n


def occluded(p, d):
    t = hit_box(p, d)
    for c, r, _, _ in SPHERES:
        t = np.minimum(t, hit_sphere(p, d, c, r))
    return t < FAR


def studio():
    scale = 2
    w, h = W * scale, H * scale
    eye = np.array([0.0, 1.55, 6.2])
    target = np.array([0.15, 0.75, -0.6])
    fwd = (target - eye) / np.linalg.norm(target - eye)
    right = np.cross(fwd, [0, 1, 0])
    right /= np.linalg.norm(right)
    up = np.cross(right, fwd)
    focal = 1 / np.tan(np.radians(34) / 2)
    out = np.zeros((h, w, 3), np.float32)
    jitter = RNG.normal(0, 0.09, (10, 3))
    for y0 in range(0, h, 100):
        ys, xs = np.mgrid[y0 : y0 + 100, 0:w]
        u = (xs + 0.5 - w / 2) / (h / 2)
        v = -(ys + 0.5 - h / 2) / (h / 2)
        d = u.reshape(-1, 1) * right + v.reshape(-1, 1) * up + focal * fwd
        d /= np.linalg.norm(d, axis=1, keepdims=True)
        o = np.broadcast_to(eye, d.shape)

        t, n = hit_cove(o, d)
        albedo = np.broadcast_to(COVE, d.shape).copy()
        shine = np.zeros(len(d))
        tb = hit_box(o, d)
        m = tb < t
        t = np.where(m, tb, t)
        albedo[m] = BOX_COLOUR
        n[m] = box_normal(o[m] + tb[m, None] * d[m])
        for c, r, col, s in SPHERES:
            ts = hit_sphere(o, d, c, r)
            m = ts < t
            t = np.where(m, ts, t)
            albedo[m] = col
            shine[m] = s
            n[m] = (o[m] + ts[m, None] * d[m] - c) / r

        p = o + t[:, None] * d + n * 1e-3
        lit = np.zeros(len(d))
        for j in jitter:
            ld = LIGHT + j
            ld = np.broadcast_to(ld / np.linalg.norm(ld), d.shape)
            lit += ~occluded(p, ld)
        lit /= len(jitter)
        diffuse = np.maximum(np.einsum("ij,j->i", n, LIGHT), 0) * lit
        # Ambient from the sky, darkened near the objects (cheap occlusion).
        ao = np.ones(len(d))
        for c, r, _, _ in SPHERES + [(BOX[0], 0.72, None, None)]:
            dc = c - p
            dist = np.linalg.norm(dc, axis=1)
            cos = np.maximum(np.einsum("ij,ij->i", n, dc) / dist, 0)
            ao -= 0.9 * cos * (r / np.maximum(dist, r)) ** 2
        ao = np.clip(ao, 0.15, 1)
        sky = (0.55 + 0.45 * n[:, 1]) * ao
        half = LIGHT - d
        half /= np.linalg.norm(half, axis=1, keepdims=True)
        spec = shine * np.maximum(np.einsum("ij,ij->i", n, half), 0) ** 60 * lit
        rgb = albedo * (1.25 * diffuse[:, None] + 0.42 * sky[:, None]) + 0.6 * spec[:, None]
        out[y0 : y0 + 100] = rgb.reshape(100, w, 3)
    img = out.reshape(H, scale, W, scale, 3).mean(axis=(1, 3))
    ys, xs = np.mgrid[0:H, 0:W]
    vignette = 1 - 0.22 * (((xs - W / 2) / W) ** 2 + ((ys - H / 2) / H) ** 2) * 2.4
    return grain(img * vignette[..., None], 0.008)


# -------------------------------------------------------------- gradient ---


def gradient():
    ys, xs = np.mgrid[0:H, 0:W] / H
    blobs = [  # position, size, linear colour
        ((0.15, 0.85), 0.55, (1.0, 0.22, 0.02)),
        ((0.75, 0.3), 0.5, (0.9, 0.05, 0.35)),
        ((1.45, 0.85), 0.4, (0.85, 0.9, 0.05)),
        ((1.5, 0.05), 0.45, (0.05, 0.45, 0.85)),
        ((0.25, 0.05), 0.45, (0.06, 0.02, 0.2)),
    ]
    acc = np.zeros((H, W, 3))
    weight = np.full((H, W), 0.05)
    acc += weight[..., None] * np.array([0.05, 0.02, 0.15])
    for (bx, by), size, colour in blobs:
        k = np.exp(-(((xs - bx) ** 2 + (ys - by) ** 2) / size**2) * 2.2) ** 1.0
        acc += k[..., None] * np.array(colour)
        weight += k
    return grain(acc / weight[..., None], 0.02)


# ---------------------------------------------------------------- poster ---


def poster(studio_linear):
    lum = studio_linear @ np.array([0.2126, 0.7152, 0.0722])
    lum = lum ** (1 / 2.2)
    # A photocopy: hard threshold on a noisy, slightly blurred image.
    noise = np.array(Image.fromarray((RNG.random((H // 3, W // 3)) * 255).astype(np.uint8))
                     .resize((W, H), Image.BILINEAR)) / 255
    toner = (lum + 0.14 * (noise - 0.5) + 0.05 * RNG.normal(0, 1, (H, W))) < 0.45
    paper = np.array([0.93, 0.91, 0.86])
    img = np.where(toner[..., None], np.array([0.05, 0.05, 0.06]), paper)
    out = Image.fromarray((img * 255).astype(np.uint8))
    draw = ImageDraw.Draw(out)
    draw.rectangle((1040, 0, 1600, 1000), fill=(255, 62, 150))
    draw.rectangle((0, 820, 1600, 1000), fill=(18, 18, 20))
    draw.text((60, 838), "NOISE", font=font("InterDisplay-Black", 150), fill=(238, 233, 222))
    draw.text((1080, 60), "N°03\nAUTUMN\nISSUE", font=font("InterDisplay-Bold", 64),
              fill=(18, 18, 20), spacing=6)
    draw.text((1080, 700), "Doors 21:00\nAll ages", font=font("Inter-Medium", 34),
              fill=(18, 18, 20), spacing=10)
    return out


# ------------------------------------------------------------------ dark ---


def dark(studio_linear):
    out = Image.new("RGB", (W, H), (18, 19, 23))
    draw = ImageDraw.Draw(out)
    white, grey, line = (240, 240, 236), (140, 142, 150), (44, 46, 54)
    draw.rectangle((80, 52, 120, 92), fill=(255, 122, 61))
    draw.text((140, 56), "Northlight", font=font("InterDisplay-SemiBold", 30), fill=white)
    for i, item in enumerate(["Work", "Studio", "Journal", "Contact"]):
        draw.text((960 + i * 130, 62), item, font=font("Inter-Medium", 22), fill=grey)
    draw.line((0, 132, W, 132), fill=line, width=2)
    draw.text((80, 210), "Light, after\ndark.", font=font("InterDisplay-SemiBold", 104),
              fill=white, spacing=4)
    body = ("A studio for objects and spaces that\nhold light. We work with glass, metal\n"
            "and the hours after sunset.")
    draw.text((84, 470), body, font=font("Inter-Regular", 26), fill=grey, spacing=12)
    draw.rectangle((84, 620, 300, 684), fill=(255, 122, 61))
    draw.text((112, 636), "See the work", font=font("Inter-SemiBold", 24), fill=(18, 19, 23))
    draw.rectangle((320, 620, 520, 684), outline=(90, 92, 102), width=2)
    draw.text((350, 636), "Get in touch", font=font("Inter-SemiBold", 24), fill=white)
    photo = to_image(studio_linear * 0.75).resize((720, 450), Image.LANCZOS)
    out.paste(photo, (800, 196))
    for i, (title, tone) in enumerate([("Glass", (36, 52, 92)), ("Metal", (70, 40, 62)),
                                       ("Night", (30, 64, 58))]):
        x = 80 + i * 490
        draw.rectangle((x, 760, x + 460, 1000), fill=tone)
        draw.text((x + 28, 788), title, font=font("InterDisplay-SemiBold", 40), fill=white)
        draw.text((x + 28, 846), f"0{i + 1} / 03", font=font("Inter-Medium", 20), fill=(190, 192, 200))
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    scene = studio()
    save(to_image(scene), "studio.jpg")
    save(to_image(gradient()), "gradient.jpg")
    save(poster(scene), "poster.jpg")
    save(dark(scene), "dark.jpg")


if __name__ == "__main__":
    main()
