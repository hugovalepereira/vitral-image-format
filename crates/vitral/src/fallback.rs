//! Default fallback image: the plain RGBA image that best approximates a Vitral
//! image for software that ignores the `vITR` chunk.

use crate::{RgbaImage, VitralImage};

/// Computes, per pixel, the straight-alpha RGBA colour whose normal
/// compositing is closest (least squares over uniformly distributed backdrop
/// values) to the Vitral transfer function.
///
/// Pixels that already behave like plain PNG pixels are reproduced exactly, so
/// their residual is zero and costs almost nothing to store.
pub fn least_squares_fallback(img: &VitralImage) -> RgbaImage {
    let pixels = img.width as usize * img.height as usize;
    let mut data = Vec::with_capacity(pixels * 4);
    for (k, w) in img.black.chunks_exact(3).zip(img.white.chunks_exact(3)) {
        let t: [i32; 3] = [0, 1, 2].map(|c| w[c] as i32 - k[c] as i32);
        // Shared transmission: the mean of the per-channel transmissions.
        let sum = t[0] + t[1] + t[2];
        let tau = ((sum + if sum >= 0 { 1 } else { -1 }) / 3).clamp(0, 255);
        let alpha = 255 - tau;
        if alpha == 0 {
            data.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for c in 0..3 {
            // Twice the premultiplied colour: 2K + (t - tau), within [0, 2 alpha].
            let p2 = (2 * k[c] as i32 + t[c] - tau).clamp(0, 2 * alpha);
            let colour = ((p2 * 255 + alpha) / (2 * alpha)).min(255);
            data.push(colour as u8);
        }
        data.push(alpha as u8);
    }
    RgbaImage {
        width: img.width,
        height: img.height,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::mul255;

    #[test]
    fn plain_rgba_pixels_round_trip_exactly() {
        for a in [0u8, 1, 2, 17, 128, 254, 255] {
            for c in [0u8, 1, 77, 128, 200, 255] {
                let k = mul255(c, a);
                let w = k + (255 - a);
                let img = VitralImage::new(1, 1, vec![k, k, k], vec![w, w, w]).unwrap();
                let f = least_squares_fallback(&img);
                let fa = f.data[3];
                let fk = mul255(f.data[0], fa);
                assert_eq!((fk, fa), (k, a), "c={c} a={a}");
            }
        }
    }
}
