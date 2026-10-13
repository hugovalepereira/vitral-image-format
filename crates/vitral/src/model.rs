//! The Vitral pixel model: per channel, `result = K + (W - K) * backdrop`.

use crate::blend::{flatten, Layer};
use crate::math::{composite, mul255};
use crate::{Error, RgbaImage};

/// A Vitral image in canonical form: the image as seen over black (`K`) and
/// over white (`W`), both RGB8, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VitralImage {
    pub width: u32,
    pub height: u32,
    /// RGB8 values over a black backdrop (`K`).
    pub black: Vec<u8>,
    /// RGB8 values over a white backdrop (`W`).
    pub white: Vec<u8>,
}

/// Three RGBA8 layers that reproduce a Vitral image with blend modes that every
/// compositor supports. Stack them over the backdrop, bottom to top:
/// `difference`, then `screen`, then `multiply`.
///
/// Pixels where a layer has no effect have alpha 0; every other pixel is
/// opaque, so the stack is exact up to 8-bit rounding.
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
    pub fn new(width: u32, height: u32, black: Vec<u8>, white: Vec<u8>) -> Result<Self, Error> {
        let expected = width as usize * height as usize * 3;
        if black.len() != expected || white.len() != expected {
            return Err(Error::InvalidInput(format!(
                "K/W buffers must have {expected} bytes for {width}x{height}"
            )));
        }
        Ok(VitralImage {
            width,
            height,
            black,
            white,
        })
    }

    /// An image with no effect on its backdrop.
    pub fn identity(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize * 3;
        VitralImage {
            width,
            height,
            black: vec![0; n],
            white: vec![255; n],
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
        Ok(VitralImage {
            width: over_black.width,
            height: over_black.height,
            black: flatten_over(over_black, 0),
            white: flatten_over(over_white, 255),
        })
    }

    /// Builds a Vitral image from a stack of layers (bottom first), as a design
    /// tool would composite them over a black and over a white backdrop.
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
            black: flatten(width, height, layers, 0.0),
            white: flatten(width, height, layers, 1.0),
        })
    }

    /// Reference compositing (normative): places the image with its top-left
    /// corner at `(x, y)` over an opaque backdrop. The backdrop's alpha is
    /// ignored and set to 255 where the image covers it.
    pub fn composite_onto(&self, backdrop: &mut RgbaImage, x: i64, y: i64) {
        for_each_overlap(self.width, self.height, backdrop, x, y, |src3, _, px| {
            for (c, value) in px[..3].iter_mut().enumerate() {
                *value = composite(self.black[src3 + c], self.white[src3 + c], *value);
            }
            px[3] = 255;
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
