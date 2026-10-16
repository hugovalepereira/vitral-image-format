//! The Vitral pixel model: per channel, `result = K + (W - K) * backdrop`,
//! plus the image's native appearance where there is no backdrop.

use crate::blend::{flatten, rgb_of, Layer};
use crate::colour::{convert_pixels, convert_rgba, RgbSpace};
use crate::fallback::least_squares_native;
use crate::math::{composite, mul255, to_u8};
use crate::pngio::read_rgba8_in_space;
use crate::{Error, RgbaImage};

/// A Vitral image in canonical form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VitralImage {
    pub width: u32,
    pub height: u32,
    /// RGB8 values over a black backdrop (`K`).
    pub black: Vec<u8>,
    /// RGB8 values over a white backdrop (`W`).
    pub white: Vec<u8>,
    /// How the image looks with nothing behind it (straight RGBA8): each
    /// pixel's own colour. It is also the PNG image that software without
    /// Vitral support displays.
    pub native: RgbaImage,
    /// The colour space of K, W and the native appearance. Blending happens
    /// on their encoded values in this space (SPEC §4.1).
    pub colour_space: RgbSpace,
}

/// Three RGBA8 layers that reproduce a Vitral image with blend modes that every
/// compositor supports. Stack them over the backdrop, bottom to top:
/// `difference`, then `screen`, then `multiply`.
///
/// Pixels where a layer has no effect have alpha 0; every other pixel is
/// opaque, so the stack is exact up to 8-bit rounding over opaque backdrops.
#[derive(Clone, Debug)]
pub struct RenderPlanes {
    pub width: u32,
    pub height: u32,
    pub difference: Vec<u8>,
    pub screen: Vec<u8>,
    pub multiply: Vec<u8>,
    /// `false` when no pixel needs inverting, so the difference layer can be
    /// skipped entirely.
    pub uses_difference: bool,
}

/// How each pixel of a Vitral image behaves, for diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelStats {
    /// No effect at all (`K = 0`, `W = 255`): fully transparent.
    pub identity: u64,
    /// Behaves like a plain PNG pixel (normal blending with opacity).
    pub normal: u64,
    /// Needs Vitral (coloured transmission, light, inversion...).
    pub vitral: u64,
}

impl VitralImage {
    /// Builds an image from K and W. The native appearance is not known from
    /// K and W alone, so it is approximated (see [`least_squares_native`]);
    /// set the real one with [`VitralImage::with_native`] when available.
    pub fn new(width: u32, height: u32, black: Vec<u8>, white: Vec<u8>) -> Result<Self, Error> {
        let expected = width as usize * height as usize * 3;
        if black.len() != expected || white.len() != expected {
            return Err(Error::InvalidInput(format!(
                "K/W buffers must have {expected} bytes for {width}x{height}"
            )));
        }
        let native = least_squares_native(width, height, &black, &white);
        Ok(VitralImage {
            width,
            height,
            black,
            white,
            native,
            colour_space: RgbSpace::Srgb,
        })
    }

    /// Replaces the native appearance, e.g. with an export of the artwork
    /// over a transparent background.
    pub fn with_native(mut self, native: RgbaImage) -> Result<Self, Error> {
        if native.width != self.width || native.height != self.height {
            return Err(Error::InvalidInput(format!(
                "native image is {}x{}, expected {}x{}",
                native.width, native.height, self.width, self.height
            )));
        }
        self.native = native;
        Ok(self)
    }

    /// An image with no effect on its backdrop.
    pub fn identity(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize * 3;
        VitralImage {
            width,
            height,
            black: vec![0; n],
            white: vec![255; n],
            native: RgbaImage::filled(width, height, [0, 0, 0, 0]),
            colour_space: RgbSpace::Srgb,
        }
    }

    /// Builds a Vitral image from two renders of the same artwork: one over a
    /// black background and one over a white background. Transparent pixels in
    /// the renders are composited over their respective background.
    pub fn from_renders(over_black: &RgbaImage, over_white: &RgbaImage) -> Result<Self, Error> {
        if over_black.width != over_white.width || over_black.height != over_white.height {
            return Err(Error::InvalidInput(format!(
                "renders differ in size: {}x{} vs {}x{}",
                over_black.width, over_black.height, over_white.width, over_white.height
            )));
        }
        let flatten_over = |img: &RgbaImage, bg: u8| -> Vec<u8> {
            let mut out = Vec::with_capacity(img.data.len() / 4 * 3);
            for px in img.data.as_chunks::<4>().0 {
                let a = px[3];
                for &c in &px[..3] {
                    out.push(mul255(c, a) + mul255(bg, 255 - a));
                }
            }
            out
        };
        VitralImage::new(
            over_black.width,
            over_black.height,
            flatten_over(over_black, 0),
            flatten_over(over_white, 255),
        )
    }

    /// Builds a Vitral image from PNG files exported by a design tool: the
    /// artwork over black, over white and, optionally, over nothing (its
    /// native look). Each export may be sRGB or Display P3; if they differ,
    /// all are converted to Display P3, whose gamut holds both.
    pub fn from_exports(
        black: &[u8],
        white: &[u8],
        transparent: Option<&[u8]>,
    ) -> Result<Self, Error> {
        let read = |bytes: &[u8], what: &str| {
            read_rgba8_in_space(bytes).map_err(|e| {
                let reason = match e {
                    Error::InvalidInput(reason) => reason,
                    other => other.to_string(),
                };
                Error::InvalidInput(format!("{what}: {reason}"))
            })
        };
        let mut exports = vec![
            read(black, "export over black")?,
            read(white, "export over white")?,
        ];
        if let Some(bytes) = transparent {
            exports.push(read(bytes, "transparent export")?);
        }
        let space = RgbSpace::widest(exports.iter().map(|(_, s)| *s));
        let mut exports = exports
            .into_iter()
            .map(|(img, from)| convert_rgba(&img, from, space));
        let (over_black, over_white) = (exports.next().unwrap(), exports.next().unwrap());
        let mut img = VitralImage::from_renders(&over_black, &over_white)?.in_colour_space(space);
        if let Some(native) = exports.next() {
            img = img.with_native(native)?;
        }
        Ok(img)
    }

    /// Builds a Vitral image from a stack of layers (bottom first), as a design
    /// tool would composite them over black, over white and over nothing.
    ///
    /// Exact for layers whose modes are affine in the backdrop (normal,
    /// multiply, screen, hard-light, exclusion); other modes are captured
    /// exactly over black and white and linearly in between.
    pub fn from_layers(width: u32, height: u32, layers: &[Layer]) -> Result<Self, Error> {
        for (i, layer) in layers.iter().enumerate() {
            if layer.image.width != width || layer.image.height != height {
                return Err(Error::InvalidInput(format!(
                    "layer {i} is not {width}x{height}"
                )));
            }
        }
        Ok(VitralImage {
            width,
            height,
            black: rgb_of(&flatten(width, height, layers, [0.0, 0.0, 0.0, 1.0])),
            white: rgb_of(&flatten(width, height, layers, [1.0, 1.0, 1.0, 1.0])),
            native: RgbaImage::new(width, height, flatten(width, height, layers, [0.0; 4]))?,
            colour_space: RgbSpace::Srgb,
        })
    }

    /// Declares the colour space the image's values are in, without changing
    /// them. Use it when the inputs (exports or layers) were in that space.
    pub fn in_colour_space(mut self, space: RgbSpace) -> Self {
        self.colour_space = space;
        self
    }

    /// The same image converted to another colour space: K, W and the native
    /// colours are converted, clipping what falls outside the target gamut.
    ///
    /// Over black and over white the result is the same colour; over other
    /// backdrops it is an approximation, because blending the encoded values
    /// of two spaces gives different results. Renderers that blend in a
    /// different space than the image's (CSS usually blends in sRGB) use this
    /// before building the render planes (SPEC §7.2).
    pub fn to_colour_space(&self, space: RgbSpace) -> VitralImage {
        let mut out = self.clone();
        convert_pixels(&mut out.black, 3, self.colour_space, space);
        convert_pixels(&mut out.white, 3, self.colour_space, space);
        out.native = convert_rgba(&self.native, self.colour_space, space);
        out.colour_space = space;
        out
    }

    /// Reference compositing (normative): places the image with its top-left
    /// corner at `(x, y)` over a backdrop.
    ///
    /// Where the backdrop is opaque, each channel becomes `K + (W - K) * b`.
    /// Where it is partly transparent (alpha `ab`), the result mixes that with
    /// the native appearance: colour `ab * f(b) + (1 - ab) * native` (both
    /// premultiplied) and alpha `ab + (1 - ab) * native_alpha`.
    pub fn composite_onto(&self, backdrop: &mut RgbaImage, x: i64, y: i64) {
        for_each_overlap(self.width, self.height, backdrop, x, y, |src3, src4, px| {
            let (k, w) = (&self.black[src3..src3 + 3], &self.white[src3..src3 + 3]);
            if px[3] == 255 {
                for c in 0..3 {
                    px[c] = composite(k[c], w[c], px[c]);
                }
                return;
            }
            let ab = px[3] as f64 / 255.0;
            let native = &self.native.data[src4..src4 + 4];
            let na = native[3] as f64 / 255.0;
            let ao = ab + (1.0 - ab) * na;
            for c in 0..3 {
                let over =
                    (255.0 * k[c] as f64 + (w[c] as f64 - k[c] as f64) * px[c] as f64) / 65025.0;
                let own = native[c] as f64 / 255.0 * na;
                let premultiplied = ab * over + (1.0 - ab) * own;
                px[c] = if ao > 0.0 {
                    to_u8((premultiplied / ao) as f32)
                } else {
                    0
                };
            }
            px[3] = to_u8(ao as f32);
        });
    }

    /// Splits the image into the difference / screen / multiply layers used by
    /// blend-mode compositors.
    pub fn render_planes(&self) -> RenderPlanes {
        let pixels = self.width as usize * self.height as usize;
        let mut difference = vec![0u8; pixels * 4];
        let mut screen = vec![0u8; pixels * 4];
        let mut multiply = vec![0u8; pixels * 4];
        let mut uses_difference = false;
        for i in 0..pixels {
            let (mut n_any, mut s_any, mut m_any) = (false, false, false);
            for c in 0..3 {
                let k = self.black[i * 3 + c];
                let w = self.white[i * 3 + c];
                let (lo, hi) = (k.min(w), k.max(w));
                let n = if w < k { 255 } else { 0 };
                let s = if hi == 0 {
                    0
                } else {
                    ((255 * lo as u32 + hi as u32 / 2) / hi as u32) as u8
                };
                difference[i * 4 + c] = n;
                screen[i * 4 + c] = s;
                multiply[i * 4 + c] = hi;
                n_any |= n != 0;
                s_any |= s != 0;
                m_any |= hi != 255;
            }
            difference[i * 4 + 3] = if n_any { 255 } else { 0 };
            screen[i * 4 + 3] = if s_any { 255 } else { 0 };
            multiply[i * 4 + 3] = if m_any { 255 } else { 0 };
            uses_difference |= n_any;
        }
        RenderPlanes {
            width: self.width,
            height: self.height,
            difference,
            screen,
            multiply,
            uses_difference,
        }
    }

    /// Classifies pixels as identity, plain-PNG-like, or Vitral-only.
    pub fn stats(&self) -> PixelStats {
        let mut stats = PixelStats::default();
        let (black, white) = (self.black.as_chunks::<3>().0, self.white.as_chunks::<3>().0);
        for (k, w) in black.iter().zip(white) {
            if *k == [0, 0, 0] && *w == [255, 255, 255] {
                stats.identity += 1;
                continue;
            }
            let t: Vec<i32> = (0..3).map(|c| w[c] as i32 - k[c] as i32).collect();
            let normal = t[0] == t[1] && t[1] == t[2] && t[0] >= 0 && {
                // Normal blending with opacity a: K = c*a, so K <= a = 255 - t.
                let a = 255 - t[0];
                k.iter().all(|&kc| (kc as i32) <= a)
            };
            if normal {
                stats.normal += 1;
            } else {
                stats.vitral += 1;
            }
        }
        stats
    }
}

impl RenderPlanes {
    /// Simulates a host compositor stacking the three planes over a backdrop
    /// with 8-bit blend modes (difference, screen, multiply), as CSS, Canvas,
    /// Figma or Illustrator would.
    pub fn composite_onto(&self, backdrop: &mut RgbaImage, x: i64, y: i64) {
        for_each_overlap(self.width, self.height, backdrop, x, y, |_, src4, px| {
            for (c, value) in px[..3].iter_mut().enumerate() {
                let mut b = *value;
                if self.difference[src4 + 3] != 0 {
                    b = b.abs_diff(self.difference[src4 + c]);
                }
                if self.screen[src4 + 3] != 0 {
                    let s = self.screen[src4 + c];
                    b = s + b - mul255(s, b);
                }
                if self.multiply[src4 + 3] != 0 {
                    b = mul255(b, self.multiply[src4 + c]);
                }
                *value = b;
            }
            px[3] = 255;
        });
    }
}

/// Calls `f(src_rgb_index, src_rgba_index, backdrop_pixel)` for every pixel of
/// a `width`x`height` image placed at `(x, y)` that overlaps the backdrop.
fn for_each_overlap(
    width: u32,
    height: u32,
    backdrop: &mut RgbaImage,
    x: i64,
    y: i64,
    mut f: impl FnMut(usize, usize, &mut [u8]),
) {
    let bw = backdrop.width as i64;
    let bh = backdrop.height as i64;
    for sy in 0..height as i64 {
        let dy = y + sy;
        if dy < 0 || dy >= bh {
            continue;
        }
        for sx in 0..width as i64 {
            let dx = x + sx;
            if dx < 0 || dx >= bw {
                continue;
            }
            let src = (sy * width as i64 + sx) as usize;
            let dst = ((dy * bw + dx) * 4) as usize;
            f(src * 3, src * 4, &mut backdrop.data[dst..dst + 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three-layer stack must match the reference composite within 2 levels
    /// for every possible (K, W, backdrop) triple.
    #[test]
    fn render_stack_error_is_at_most_two_levels() {
        let mut worst = 0u8;
        for k in 0..=255u8 {
            for w in 0..=255u8 {
                let img = VitralImage::new(1, 1, vec![k, 0, 0], vec![w, 255, 255]).unwrap();
                let planes = img.render_planes();
                for b in 0..=255u8 {
                    let mut bd = RgbaImage::filled(1, 1, [b, b, b, 255]);
                    planes.composite_onto(&mut bd, 0, 0);
                    let err = bd.data[0].abs_diff(composite(k, w, b));
                    worst = worst.max(err);
                }
            }
        }
        assert!(worst <= 2, "worst error {worst}");
    }

    #[test]
    fn converting_keeps_the_look_over_black_and_white() {
        let img = red_multiply().in_colour_space(RgbSpace::DisplayP3);
        let srgb = img.to_colour_space(RgbSpace::Srgb);
        assert_eq!(srgb.colour_space, RgbSpace::Srgb);
        let mut k = img.black.clone();
        convert_pixels(&mut k, 3, RgbSpace::DisplayP3, RgbSpace::Srgb);
        assert_eq!(srgb.black, k);
        let back = srgb.to_colour_space(RgbSpace::Srgb);
        assert_eq!(back, srgb, "converting to the same space changes nothing");
    }

    #[test]
    fn exports_in_mixed_colour_spaces_become_display_p3() {
        let black = RgbaImage::filled(1, 1, [255, 0, 0, 255]);
        let white = RgbaImage::filled(1, 1, [255, 255, 255, 255]);
        let img = VitralImage::from_exports(
            &crate::write_rgba8(&black).unwrap(),
            &crate::write_rgba8_in(&white, RgbSpace::DisplayP3).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(img.colour_space, RgbSpace::DisplayP3);
        assert_eq!(img.black, [234, 51, 35], "sRGB red, converted to P3");
        assert_eq!(img.white, [255, 255, 255]);
    }

    #[test]
    fn identity_has_no_effect() {
        let img = VitralImage::identity(4, 3);
        let mut bd = RgbaImage::filled(4, 3, [12, 200, 99, 255]);
        let before = bd.clone();
        img.composite_onto(&mut bd, 0, 0);
        assert_eq!(bd, before);
        let planes = img.render_planes();
        assert!(!planes.uses_difference);
        assert!(planes.screen.as_chunks::<4>().0.iter().all(|p| p[3] == 0));
        assert!(planes.multiply.as_chunks::<4>().0.iter().all(|p| p[3] == 0));
    }

    fn red_multiply() -> VitralImage {
        let layer = Layer::new(
            RgbaImage::filled(1, 1, [255, 0, 0, 255]),
            crate::BlendMode::Multiply,
        );
        VitralImage::from_layers(1, 1, &[layer]).unwrap()
    }

    /// With nothing behind it, a pixel shows its own (native) colour.
    #[test]
    fn transparent_backdrop_shows_the_native_colour() {
        let img = red_multiply();
        assert_eq!(img.native.data, [255, 0, 0, 255]);
        let mut bd = RgbaImage::filled(1, 1, [0, 0, 0, 0]);
        img.composite_onto(&mut bd, 0, 0);
        assert_eq!(bd.data, [255, 0, 0, 255]);
    }

    /// A partly transparent backdrop blends smoothly between the native colour
    /// and the opaque result, without a jump next to full opacity.
    #[test]
    fn partly_transparent_backdrop_is_continuous() {
        let img = red_multiply();
        let at = |alpha: u8| {
            let mut bd = RgbaImage::filled(1, 1, [100, 200, 50, alpha]);
            img.composite_onto(&mut bd, 0, 0);
            bd.data
        };
        // Opaque: multiply by red keeps only the red channel of the backdrop.
        assert_eq!(at(255), [100, 0, 0, 255]);
        let near = at(254);
        assert!(near[0].abs_diff(100) <= 2 && near[1] <= 1 && near[3] == 255);
        // Half transparent: halfway between the backdrop result and pure red.
        let half = at(128);
        assert!(half[0].abs_diff(178) <= 1, "{half:?}");
        assert_eq!(&half[1..], &[0, 0, 255]);
    }

    #[test]
    fn composite_respects_offset_and_bounds() {
        // A 2x2 pure black multiply square placed partly outside the backdrop.
        let img = VitralImage::new(2, 2, vec![0; 12], vec![0; 12]).unwrap();
        let mut bd = RgbaImage::filled(3, 3, [200, 200, 200, 255]);
        img.composite_onto(&mut bd, 2, -1);
        let at = |x: usize, y: usize| bd.data[(y * 3 + x) * 4];
        assert_eq!(at(2, 0), 0);
        assert_eq!(at(1, 0), 200);
        assert_eq!(at(2, 1), 200);
    }
}
