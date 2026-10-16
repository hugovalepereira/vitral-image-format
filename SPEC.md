# Vitral Image Format — Specification

**Version:** 0.1 (draft) · **Date:** October 2026 · **Editor:** Hugo Vale Pereira
**Status:** Working draft. Anything may change before 1.0. Feedback is welcome as
GitHub issues.

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be interpreted as
described in RFC 2119.

---

## 1. Introduction

PNG lets an image say *how much* of the backdrop shows through each pixel
(alpha). Vitral lets an image say *how each pixel transforms the backdrop*: tint
it, lighten it, darken it or invert it, per colour channel. This is what design
tools do with blend modes, carried inside the file so that it survives export.

A Vitral file is a valid PNG datastream:

* the main image is the image's **native appearance**: each pixel's own colour,
  shown where nothing is behind the image. Any PNG decoder displays it, so it is
  also the **fallback**;
* an ancillary chunk, `viTr`, carries a **residual** that lets a Vitral decoder
  recover the exact transfer function of every pixel.

### 1.1 Goals

* Carry blend behaviour inside the image, per pixel.
* Remain a valid PNG that degrades gracefully in software without support.
* Be renderable with blend modes that hosts already support (no new compositor
  primitives required).
* Be authorable with any tool, without plugins.

### 1.2 Non-goals for 0.1

* Non-affine blend modes (overlay, soft light, HSL modes), neighbourhood effects
  (backdrop blur, refraction), animation, more than 8 bits per sample, linear-light
  blending. See §12.

---

## 2. Pixel model

### 2.1 Transfer function

For every pixel and every colour channel *c* ∈ {R, G, B}, a Vitral image defines a
function from the backdrop value *b* to the result value *r*, all normalised to
[0, 1]:

```
r = K + (W − K) · b
```

* **K** is the result over a black backdrop (*b* = 0).
* **W** is the result over a white backdrop (*b* = 1).

K and W are each in [0, 1], so *r* is always in [0, 1] and no clamping is needed.
Every affine function from [0, 1] to [0, 1] has exactly one (K, W) pair, including
decreasing ones (W < K), which invert the backdrop.

In this version, K and W are stored as 8-bit integers (0–255).

### 2.2 Relation to PNG transparency

A straight-alpha RGBA pixel with colour *C* and alpha *α* composited normally is
the special case `K = α·C`, `W = α·C + (1 − α)`, with the same W − K on all three
channels. Every PNG image is therefore a Vitral image; Vitral adds per-channel
transmission (coloured filters), emission without occlusion (light), and
inversion.

### 2.3 Native appearance and transparent backdrops

Every pixel also has a **native appearance** *N*: a straight-alpha RGBA colour,
which is how the pixel looks with nothing behind it. It plays the role of a
layer's own colour in a design tool, where a layer in any blend mode shows its
own colour over a transparent canvas.

The native appearance cannot be derived from K and W (for example, multiply with
a colour *s* and hard light with *s*/2 have the same K and W but different own
colours), so it is stored: it is the main PNG image (§4.1).

Over a backdrop pixel with straight colour *b* and alpha *a*b, the result is, per
channel, with *f*(*b*) the transfer function of §2.1 and premultiplied colours:

```
colour = a_b · f(b) + (1 − a_b) · N_a · N_c
alpha  = a_b + (1 − a_b) · N_a
```

This reduces to §2.1 over an opaque backdrop (*a*b = 1) and to the native
appearance over nothing (*a*b = 0), and it has the same structure as the W3C
compositing formula for a blend mode over a partly transparent backdrop.

In practice, anything visible behind the image (a page, a photo, a colour, a
checkerboard drawn by a viewer) is an opaque backdrop, and the image blends with
it. A transparent backdrop only occurs when the image is flattened before its
final backdrop is known: a layer group composited on its own (in CSS, an
isolated group), an artboard without a fill exported as PNG, or software
without Vitral support. The result must then be a plain RGBA pixel, which cannot
carry a blend mode, so it shows the native appearance, as design tools do.

### 2.4 Colour space

K, W and the native appearance are encoded values in the **image's colour space**,
which the main PNG image declares in the usual way. Version 0.1 defines two:

| Colour space | Declared by |
|---|---|
| **sRGB** | an `sRGB` chunk, an ICC profile with sRGB primaries, `cICP` 1/13, or no colour chunk at all |
| **Display P3** | an ICC profile with Display P3 primaries, or `cICP` 12/13 (P3 D65 primaries, sRGB transfer function) |

Blending operates on the encoded values of the image's colour space (sRGB
transfer function, not linear light). That is what design tools do in a document
of that colour space, and what CSS Compositing and Blending Level 1 and the Canvas
2D API do in sRGB, so an image keeps the look it had where it was made.

* Encoders MUST tag the main image with its colour space. For Display P3 they
  MUST write an ICC profile (`iCCP`) and SHOULD also write `cICP` 12, 13, 0, 1.
* Encoders MUST NOT take samples in one colour space as values in another. Inputs
  in different supported colour spaces are converted to one of them (the
  reference encoder uses Display P3, whose gamut contains sRGB); inputs in other
  colour spaces, or holding linear-light values (`cICP` transfer 8, an ICC tone
  curve or `gAMA` of 1.0), are converted by the encoder or refused.
* Decoders MUST ignore the `viTr` chunk of a file that declares another colour
  space (§6).
* Hosts blend in their own working space (§7.2), which may differ from the
  image's. Linear-light blending is future work (§12).

---

## 3. Arithmetic

All normative computations use integers. Define, for integers *a*, *b* in 0–255:

```
mul255(a, b) = round(a · b / 255)
```

The quotient *a·b/255* is never exactly halfway between two integers, so the
rounding direction is unambiguous. An exact integer implementation is
`t = a·b + 128; (t + (t >> 8)) >> 8`.

---

## 4. File structure

A Vitral file MUST be a valid PNG datastream (PNG Specification, Third Edition)
that contains exactly one `viTr` chunk.

### 4.1 Main image (native appearance and fallback)

The main image (`IHDR`, `PLTE`, `tRNS`, `IDAT`) is the native appearance *N*
(§2.3), which software without Vitral support displays as a fallback. It MAY use any
colour type, with a bit depth of 8 or less. Before use, decoders normalise it to
**RGBA8**:

* palette indices are expanded and `tRNS` is applied as alpha;
* bit depths below 8 are scaled to 8 bits as the PNG specification describes;
* greyscale is replicated to R, G and B;
* a missing alpha channel is filled with 255;
* no gamma correction or colour management is applied to the samples.

Call the normalised main image *F* (the native appearance), with channels *F*c
and *F*a. Define, per channel:

```
Fk = mul255(Fc, Fa)          (the fallback over black)
Fw = Fk + 255 − Fa           (the fallback over white)
```

In 0.1, the main image MUST NOT have a bit depth of 16, and the file MUST NOT be an
animated PNG (an `acTL` chunk is present). Decoders that find either MUST ignore
the `viTr` chunk.

### 4.2 The `viTr` chunk

| Property | Value |
|---|---|
| Chunk type | `viTr` (0x76 0x69 0x54 0x72) |
| Ancillary | yes (lowercase first letter): decoders without support skip it |
| Private | yes (lowercase second letter): not yet registered |
| Reserved bit | clear (uppercase third letter), as required |
| Safe to copy | **yes** (lowercase fourth letter): the chunk depends on the pixel values, but carries its own check of them (§4.4), so tools that recompress or re-encode the image losslessly may keep it, and decoders detect any visible change to the pixels |
| Position | after `IHDR`, before `IEND`; encoders SHOULD place it after the last `IDAT` |
| Multiplicity | exactly one |

If the chunk type is registered as a public chunk in the future, the second
letter becomes uppercase (`vITr`).

### 4.3 Payload

All multi-byte integers are big-endian, as in PNG.

| Offset | Size | Field | Value in 0.1 |
|---|---|---|---|
| 0 | 1 | major version | 0 |
| 1 | 1 | minor version | 1 |
| 2 | 1 | model | 1 = affine, black/white (this document) |
| 3 | 1 | flags | bit 0: residual storage (§4.5); other bits 0 |
| 4 | 4 | fallback CRC | CRC-32 of the normalised fallback (§4.4) |
| 8 | *n* | residual | a complete PNG datastream (§4.5) |

* A decoder MUST ignore the chunk if the major version is not one it implements.
  Minor versions within a major version are backward compatible.
* A decoder MUST ignore the chunk if the model is unknown. Model values 2–255 are
  reserved.
* Flag bit 0 (value 1) selects how the residual stores K and W (§4.5).
  Encoders MUST write the other bits as 0. Decoders MUST ignore flag bits they
  do not understand.

### 4.4 Fallback CRC

The fallback CRC is the CRC-32 used by PNG (ISO 3309 / ITU-T V.42), computed over
the bytes of the normalised RGBA8 fallback: rows top to bottom, pixels left to
right, bytes in R, G, B, A order, with no filter bytes. Every pixel whose alpha
is 0 is counted as (0, 0, 0, 0), whatever colour the file stores for it.

It detects files whose main image was re-encoded with changes (lossy
optimisation, editing) by software that kept the `viTr` chunk. When it does not
match, the decoder MUST ignore the chunk.

The colour of a fully transparent pixel is invisible and does not affect *F*k or
*F*w, so it is left out: lossless optimisers that clear it (for example
`oxipng -a` or `pngcrush -blacken`) keep the file valid.

### 4.5 Residual

The residual field is a PNG datastream (signature included) with:

* width equal to the main image width;
* height equal to **twice** the main image height;
* colour type 2 (truecolour), bit depth 8;
* any interlace method.

Rows 0 to H − 1 hold **DK**; rows H to 2H − 1 hold **DW**, where H is the main
image height. With flag bit 0 clear, DK and DW are differences from the main image
(§5); with flag bit 0 set, they are K and W themselves.

The residual MAY contain ancillary chunks, which decoders ignore. It
MUST NOT contain a `viTr` chunk.

Decoders MUST check the residual's `IHDR` (type, depth, dimensions) before
decompressing it, so that a hostile header cannot force a large allocation.

---

## 5. Encoding

Given K and W (8-bit, per pixel and channel) and the native appearance F, the
residual holds either differences (flag bit 0 clear):

```
DK = (K − Fk) mod 256
DW = (W − Fw) mod 256
```

or K and W directly (flag bit 0 set: DK = K, DW = W). Differences are near zero
where the image behaves like a plain PNG; direct storage compresses better where
it blends heavily. The reference encoder writes both and keeps the smaller.

The residual makes the file lossless whatever F is. F decides what is shown over
transparent areas and by software without Vitral support, and affects the file
size: the residual is zero wherever F, composited normally, already matches K
and W.

### 5.1 When the native appearance is unknown (informative)

Encoders SHOULD use the real native appearance: the artwork composited over
nothing (in a design tool, an export with a transparent background). When only K
and W are available, the reference encoder approximates it with the RGBA colour
whose normal compositing best matches the transfer function in the
least-squares sense over a uniformly distributed backdrop value:

```
t_c   = W_c − K_c
τ     = clamp(round((t_R + t_G + t_B) / 3), 0, 255)     shared transmission
α     = 255 − τ
p2_c  = clamp(2·K_c + t_c − τ, 0, 2α)                    twice the premultiplied colour
C_c   = min(255, round(255 · p2_c / (2α)))               (F = 0,0,0,0 when α = 0)
```

For pixels that already behave like plain PNG pixels, this reproduces them
exactly, so their residual is zero.

---

## 6. Decoding

1. Decode the main image and normalise it to RGBA8 (§4.1). If this fails, the file
   is not a usable PNG.
2. Find the `viTr` chunk. If there is none, more than one, or its chunk CRC is
   wrong, treat the file as a plain PNG. Determine the image's colour space
   (§2.4); if it is neither sRGB nor Display P3, treat the file as a plain PNG.
3. Apply the version, model and flag rules (§4.3) and the restrictions of §4.1.
4. Compare the fallback CRC (§4.4). On mismatch, treat the file as a plain PNG.
5. Decode the residual (§4.5). On any error, treat the file as a plain PNG.
6. For every pixel and channel:

```
K = (Fk + DK) mod 256        (flag bit 0 clear)
W = (Fw + DW) mod 256

K = DK,  W = DW              (flag bit 0 set)
```

"Treat the file as a plain PNG" means: use K = Fk and W = Fw, which is exactly
the normal compositing of the fallback. Decoders SHOULD report why the Vitral
data was ignored.

---

## 7. Rendering

### 7.1 Reference compositing (normative)

With the image placed over an opaque backdrop pixel *B* (8-bit):

```
r = round((255·K + (W − K)·B) / 255)
```

The numerator is always in 0–65025 and the quotient is never exactly halfway
between two integers. Integer form: `r = (255·K + (W − K)·B + 127) div 255`.

Over a backdrop pixel that is not opaque, apply §2.3 in real arithmetic and
round the straight colour and the alpha to the nearest 8-bit value. A result
with alpha 0 has colour 0.

*B* is in the image's colour space (§2.4). A backdrop in another colour space is
converted to it first, and the result back.

### 7.2 Blend-mode stack (recommended)

Hosts that offer blend modes but no custom compositing (CSS, Canvas 2D, SVG,
design and video tools) can render a Vitral image as three layers stacked over
the backdrop, bottom to top. Per pixel and channel:

```
lo = min(K, W)      hi = max(K, W)
N  = 255 if W < K, else 0
S  = 0 if hi = 0, else round(255 · lo / hi)
M  = hi
```

| Order | Layer | Blend mode | Effect on backdrop value b (normalised) |
|---|---|---|---|
| 1 | N | difference | b → 1 − b where W < K |
| 2 | S | screen | b → S + (1 − S)·b |
| 3 | M | multiply | b → M·b |

The composition gives `M·S + M·(1 − S)·b' = lo + (hi − lo)·b'`, which is the
transfer function of §2.1.

* A layer pixel whose three channels have no effect (N = 0, S = 0, or M = 255
  respectively) SHOULD have alpha 0; every other layer pixel MUST have alpha 255.
  This keeps the stack exact while minimising artefacts over transparent areas.
* When no pixel needs inverting, the difference layer MAY be omitted.
* With 8-bit blending that rounds to nearest, the stack differs from §7.1 by at
  most **2** levels. The reference implementation verifies this for all 2²⁴
  combinations of K, W and B.
* The layers blend with whatever the host considers the backdrop. In CSS, that is
  the content of the same stacking context.
* The stack reproduces §2.1 over **opaque** backdrops only. Over transparent
  areas, blend-mode hosts show the layers' own colours rather than the native
  appearance of §2.3. Web pages always have an opaque backdrop; in design tools,
  place Vitral images over a filled background, or use a renderer that
  implements §7.1.
* The host blends in its own working space: a design tool in its document's
  colour space; CSS in an implementation-defined space, sRGB by default and the
  display's on some systems. When that differs from the image's colour space,
  the renderer converts K and W to the working space (as colours, clipping what
  falls outside its gamut) before computing the layers. The result is then exact
  over black and white backdrops and close elsewhere: blending the encoded values
  of two colour spaces gives different results, as CSS blend modes themselves do
  on different displays. The reference polyfill assumes sRGB.

### 7.3 Alternative stack (informative)

Where `plus-lighter` (additive compositing) is available, a more precise stack
is: difference with N, then multiply with |W − K|, then plus-lighter with
min(K, W).

### 7.4 GPU rendering (informative)

With dual-source blending (`out = src0 + src1 · dst`), draw the inversion pass
with blend factors (ONE_MINUS_DST_COLOR, ONE_MINUS_SRC_COLOR) and N as the source
(only where N ≠ 0), then draw a second pass with factors (ONE, SRC1_COLOR),
src0 = min(K, W) and src1 = |W − K|.

---

## 8. Authoring (informative)

### 8.1 From exports

Render the artwork over a black background and over a white background, with
every layer's blend mode active against that background, and export both as
PNG. The two exports are K and W directly. Any pixel with alpha below 255 in an
export is first composited over that export's background.

Then export the artwork with a transparent background. That export is the native
appearance N. Without it, the encoder approximates N (§5.1).

Export all three in the document's colour space (sRGB or Display P3); the file
keeps it (§2.4).

### 8.2 From layers

A layer stack is flattened three times: over black (K), over white (W) and over
nothing (N), using the W3C formulas for blending over a partly transparent
backdrop. N is always exact; K and W are exact for modes that are affine in the
backdrop and approximate for the others:

| Mode (W3C name) | Captured by 0.1 |
|---|---|
| normal, multiply, screen, hard-light, exclusion | exactly |
| plus-lighter, plus-darker, darken, lighten, difference | exactly over black and white, linearly in between (plus-lighter behaves like screen, difference like exclusion) |
| overlay, soft-light, color-dodge, color-burn, hue, saturation, color | **not at all**: these modes leave black and white backdrops unchanged |
| luminosity | incorrectly (as a flat grey) |

Tools SHOULD warn when a mode is not captured.

---

## 9. Naming and identification

* Recommended file name extension: **`.vitral.png`**. The media type is
  `image/png`.
* Software MUST identify Vitral files by the presence of a valid `viTr` chunk, not
  by the file name.

---

## 10. Security considerations

* The format adds no capability beyond the blend modes hosts already implement.
  Rendering needs no access to backdrop pixels by script, and the arithmetic is
  data-independent (constant time), unlike the timing side channels reported for
  some blend modes in 2018.
* Decoders MUST bound memory by the dimensions in the main `IHDR` and MUST verify
  the residual's dimensions before decompressing it (§4.5).
* Decoders MUST treat a damaged chunk as absent, never as a fatal error, so that
  a hostile `viTr` cannot prevent the fallback from displaying.

---

## 11. Conformance

* A **conforming encoder** writes files that satisfy §4 and decode (§6) to the K
  and W it was given.
* A **conforming decoder** recovers K and W exactly, follows the error handling of
  §6, and falls back to the main image whenever §4 or §6 says to ignore the chunk.
* A **conforming renderer** produces, over an opaque backdrop, results within 2
  levels of §7.1.

The `examples/` directory contains test files. The reference implementation's
command-line tool reproduces them: `vitral decode` writes K, W, the native image and
the three layers; `vitral render` applies §7.1; `vitral diff` compares renders.

---

## 12. Future work

* **Model 2, layered**: layers with named non-affine blend modes (overlay, soft
  light, HSL modes), each composited by the host.
* **Backdrop effects**: per-pixel masks for backdrop blur and refraction (in CSS:
  `backdrop-filter`).
* 16-bit samples, colour spaces wider than Display P3, HDR, and a flag for
  blending in linear light (as some video and 3D tools do; neither CSS nor the
  common design tools blend that way).
* Animation (APNG) and video.
* Public chunk registration.
