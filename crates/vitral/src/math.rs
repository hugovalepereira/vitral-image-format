//! Exact 8-bit integer arithmetic shared by the encoder, decoder and renderer.
//! These functions are normative: `SPEC.md` defines the format in their terms.

/// `round(a * b / 255)` for `a, b` in `0..=255`.
///
/// `a * b / 255` is never exactly halfway between two integers, so rounding is
/// unambiguous.
#[inline]
pub fn mul255(a: u8, b: u8) -> u8 {
    let t = a as u32 * b as u32 + 128;
    ((t + (t >> 8)) >> 8) as u8
}

/// Reference composite of one channel: `round(K + (W - K) * B / 255)`.
#[inline]
pub fn composite(k: u8, w: u8, b: u8) -> u8 {
    let num = 255 * k as i32 + (w as i32 - k as i32) * b as i32;
    // `num` is in 0..=65025 because the result lies between K and W.
    ((num + 127) / 255) as u8
}

/// Converts a normalised value to 8 bits, rounding half up and clamping.
#[inline]
pub fn to_u8(v: f32) -> u8 {
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

#[inline]
pub fn to_unit(v: u8) -> f32 {
    v as f32 / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul255_is_exact_rounding() {
        for a in 0..=255u32 {
            for b in 0..=255u32 {
                let exact = ((a * b) as f64 / 255.0).round() as u8;
                assert_eq!(mul255(a as u8, b as u8), exact, "a={a} b={b}");
            }
        }
    }

    #[test]
    fn composite_is_exact_rounding() {
        for k in (0..=255u32).step_by(3) {
            for w in (0..=255u32).step_by(5) {
                for b in 0..=255u32 {
                    let exact =
                        (k as f64 + (w as f64 - k as f64) * b as f64 / 255.0).round() as u8;
                    assert_eq!(composite(k as u8, w as u8, b as u8), exact);
                }
            }
        }
    }
}
