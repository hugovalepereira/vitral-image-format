//! W3C Compositing & Blending modes, used to author Vitral images from layers.
//!
//! Formulas follow <https://www.w3.org/TR/compositing-1/> (blending of a source
//! with straight alpha over an opaque backdrop), plus the `plus-lighter` and
//! `plus-darker` operators from Compositing Level 2.

use std::str::FromStr;

use crate::math::{to_u8, to_unit};
use crate::RgbaImage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    PlusLighter,
    PlusDarker,
}

impl BlendMode {
    pub const ALL: [BlendMode; 18] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::PlusLighter,
        BlendMode::PlusDarker,
    ];

    /// CSS keyword for this mode.
    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Multiply => "multiply",
            BlendMode::Screen => "screen",
            BlendMode::Overlay => "overlay",
            BlendMode::Darken => "darken",
            BlendMode::Lighten => "lighten",
            BlendMode::ColorDodge => "color-dodge",
            BlendMode::ColorBurn => "color-burn",
            BlendMode::HardLight => "hard-light",
            BlendMode::SoftLight => "soft-light",
            BlendMode::Difference => "difference",
            BlendMode::Exclusion => "exclusion",
            BlendMode::Hue => "hue",
            BlendMode::Saturation => "saturation",
            BlendMode::Color => "color",
            BlendMode::Luminosity => "luminosity",
            BlendMode::PlusLighter => "plus-lighter",
            BlendMode::PlusDarker => "plus-darker",
        }
    }

    /// How faithfully a v0.1 Vitral image (one affine function per channel)
    /// captures this mode. Every mode is captured exactly over black and white;
    /// what differs is the behaviour in between.
    pub fn fidelity(self) -> Fidelity {
        match self {
            BlendMode::Normal
            | BlendMode::Multiply
            | BlendMode::Screen
            | BlendMode::HardLight
            | BlendMode::Exclusion => Fidelity::Exact,
            BlendMode::PlusLighter
            | BlendMode::PlusDarker
            | BlendMode::Darken
            | BlendMode::Lighten
            | BlendMode::Difference => Fidelity::Approximate,
            BlendMode::Overlay
            | BlendMode::SoftLight
            | BlendMode::ColorDodge
            | BlendMode::ColorBurn
            | BlendMode::Hue
            | BlendMode::Saturation
            | BlendMode::Color
            | BlendMode::Luminosity => Fidelity::Lost,
        }
    }

    pub fn is_exact(self) -> bool {
        self.fidelity() == Fidelity::Exact
    }

    fn separable(self, cb: f32, cs: f32) -> f32 {
        match self {
            BlendMode::Normal => cs,
            BlendMode::Multiply => cb * cs,
            BlendMode::Screen => cb + cs - cb * cs,
            BlendMode::Overlay => BlendMode::HardLight.separable(cs, cb),
            BlendMode::Darken => cb.min(cs),
            BlendMode::Lighten => cb.max(cs),
            BlendMode::ColorDodge => {
                if cb == 0.0 {
                    0.0
                } else if cs >= 1.0 {
                    1.0
                } else {
                    (cb / (1.0 - cs)).min(1.0)
                }
            }
            BlendMode::ColorBurn => {
                if cb >= 1.0 {
                    1.0
                } else if cs <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - cb) / cs).min(1.0)
                }
            }
            BlendMode::HardLight => {
                if cs <= 0.5 {
                    cb * 2.0 * cs
                } else {
                    let s = 2.0 * cs - 1.0;
                    cb + s - cb * s
                }
            }
            BlendMode::SoftLight => {
                if cs <= 0.5 {
                    cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
                } else {
                    let d = if cb <= 0.25 {
                        ((16.0 * cb - 12.0) * cb + 4.0) * cb
                    } else {
                        cb.sqrt()
                    };
                    cb + (2.0 * cs - 1.0) * (d - cb)
                }
            }
            BlendMode::Difference => (cb - cs).abs(),
            BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
            _ => unreachable!("not a separable blend mode"),
        }
    }

    /// Places a source pixel (straight colour `cs`, alpha `a`) over a backdrop
    /// pixel (straight colour `cb`, alpha `ab`): blend, then source-over, as in
    /// W3C Compositing. Returns the straight colour and alpha of the result.
    ///
    /// Where the backdrop is transparent the source shows its own colour, so a
    /// layer over nothing looks the same in every mode.
    pub fn apply(self, cb: [f32; 3], ab: f32, cs: [f32; 3], a: f32) -> ([f32; 3], f32) {
        let ao_add = (a + ab).min(1.0);
        match self {
            BlendMode::PlusLighter => {
                let co = cb.map_with(cs, |b, s| (ab * b + a * s).min(1.0));
                return (unpremultiply(co, ao_add), ao_add);
            }
            BlendMode::PlusDarker => {
                // Darkness (alpha minus premultiplied colour) adds up.
                let co = cb.map_with(cs, |b, s| {
                    (ao_add - a * (1.0 - s) - ab * (1.0 - b)).max(0.0)
                });
                return (unpremultiply(co, ao_add), ao_add);
            }
            _ => {}
        }
        let blended = match self {
            BlendMode::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
            BlendMode::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
            BlendMode::Color => set_lum(cs, lum(cb)),
            BlendMode::Luminosity => set_lum(cb, lum(cs)),
            mode => cb.map_with(cs, |b, s| mode.separable(b, s)),
        };
        let mixed = cs.map_with(blended, |s, m| (1.0 - ab) * s + ab * m);
        let ao = a + ab * (1.0 - a);
        let co = cb.map_with(mixed, |b, m| a * m + (1.0 - a) * ab * b);
        (unpremultiply(co, ao), ao)
    }
}

fn unpremultiply(co: [f32; 3], ao: f32) -> [f32; 3] {
    if ao <= 0.0 {
        [0.0; 3]
    } else {
        co.map(|v| (v / ao).clamp(0.0, 1.0))
    }
}

/// See [`BlendMode::fidelity`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fidelity {
    /// Reproduced exactly over any backdrop.
    Exact,
    /// Exact over black and white, linear in between (e.g. `plus-lighter`
    /// behaves like `screen`, `difference` like `exclusion`).
    Approximate,
    /// The mode leaves black and white backdrops (nearly) untouched, so the
    /// affine model cannot see it. Needs the layered model planned for v2.
    Lost,
}

trait MapWith {
    fn map_with(self, other: Self, f: impl Fn(f32, f32) -> f32) -> Self;
}

impl MapWith for [f32; 3] {
    fn map_with(self, other: Self, f: impl Fn(f32, f32) -> f32) -> Self {
        [
            f(self[0], other[0]),
            f(self[1], other[1]),
            f(self[2], other[2]),
        ]
    }
}

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 {
        out = out.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        out = out.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    out
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    if max > min {
        c.map(|v| (v - min) * s / (max - min))
    } else {
        [0.0; 3]
    }
}

impl FromStr for BlendMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = s.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        let key = match key.as_str() {
            "add" | "plus" | "linear-dodge" => "plus-lighter",
            "linear-burn" => "plus-darker",
            other => other,
        };
        BlendMode::ALL
            .into_iter()
            .find(|m| m.name() == key)
            .ok_or_else(|| format!("unknown blend mode '{s}'"))
    }
}

/// A full-canvas RGBA layer with a blend mode and an opacity.
#[derive(Clone, Debug)]
pub struct Layer {
    pub image: RgbaImage,
    pub mode: BlendMode,
    pub opacity: f32,
}

impl Layer {
    pub fn new(image: RgbaImage, mode: BlendMode) -> Self {
        Layer {
            image,
            mode,
            opacity: 1.0,
        }
    }

    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

/// Composites `layers` (bottom first) over a uniform backdrop, given as a
/// straight RGBA colour, and returns RGBA8. Intermediate results are kept in
/// floating point and clamped to `[0, 1]` after each layer, like a design tool.
pub(crate) fn flatten(width: u32, height: u32, layers: &[Layer], backdrop: [f32; 4]) -> Vec<u8> {
    let pixels = width as usize * height as usize;
    let mut out = Vec::with_capacity(pixels * 4);
    for i in 0..pixels {
        let mut colour = [backdrop[0], backdrop[1], backdrop[2]];
        let mut alpha = backdrop[3];
        for layer in layers {
            let px = &layer.image.data[i * 4..i * 4 + 4];
            let a = to_unit(px[3]) * layer.opacity;
            if a <= 0.0 {
                continue;
            }
            let cs = [to_unit(px[0]), to_unit(px[1]), to_unit(px[2])];
            (colour, alpha) = layer.mode.apply(colour, alpha, cs, a);
        }
        out.extend(colour.map(to_u8));
        out.push(to_u8(alpha));
    }
    out
}

/// The RGB channels of an RGBA8 buffer.
pub(crate) fn rgb_of(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, _]| [r, g, b])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VitralImage;

    fn solid(rgba: [u8; 4]) -> RgbaImage {
        RgbaImage::filled(1, 1, rgba)
    }

    /// For the affine modes, a Vitral image built from black/white captures must
    /// match direct blending over every grey backdrop (within rounding).
    #[test]
    fn affine_modes_are_exact_over_any_backdrop() {
        let sources = [
            [255, 0, 0, 255],
            [30, 140, 220, 200],
            [250, 240, 10, 90],
            [128, 128, 128, 255],
        ];
        for mode in BlendMode::ALL.into_iter().filter(|m| m.is_exact()) {
            for src in sources {
                let layer = Layer::new(solid(src), mode);
                let img = VitralImage::from_layers(1, 1, std::slice::from_ref(&layer)).unwrap();
                for b in (0..=255u16).step_by(15) {
                    let b = b as u8;
                    let mut bd = solid([b, b, b, 255]);
                    img.composite_onto(&mut bd, 0, 0);
                    let v = to_unit(b);
                    let direct =
                        rgb_of(&flatten(1, 1, std::slice::from_ref(&layer), [v, v, v, 1.0]));
                    for (got, want) in bd.data[..3].iter().zip(&direct) {
                        let err = got.abs_diff(*want);
                        assert!(err <= 1, "{} src={src:?} b={b} err={err}", mode.name());
                    }
                }
            }
        }
    }

    /// "Lost" modes must really be invisible to black/white capture, and the
    /// others must not be: this keeps `fidelity()` honest.
    #[test]
    fn fidelity_matches_black_and_white_capture() {
        let sources = [
            [255, 0, 0, 255],
            [30, 140, 220, 255],
            [250, 240, 10, 255],
            [90, 90, 90, 255],
        ];
        for mode in BlendMode::ALL {
            let changes = sources.iter().any(|&src| {
                let layer = Layer::new(solid(src), mode);
                let img = VitralImage::from_layers(1, 1, &[layer]).unwrap();
                img.black != [0, 0, 0] || img.white != [255, 255, 255]
            });
            let lost = mode.fidelity() == Fidelity::Lost;
            if mode == BlendMode::Luminosity {
                // Captured, but as a flat grey: wrong rather than invisible.
                assert!(lost && changes);
            } else {
                assert_eq!(lost, !changes, "{}", mode.name());
            }
        }
    }

    /// Over nothing, a layer shows its own colour whatever its blend mode.
    #[test]
    fn layers_over_transparency_show_their_own_colour() {
        let src = [200, 40, 90, 170];
        for mode in BlendMode::ALL {
            let native = flatten(1, 1, &[Layer::new(solid(src), mode)], [0.0; 4]);
            assert_eq!(native, src, "{}", mode.name());
        }
    }

    /// Over an opaque backdrop the alpha-aware formula reduces to the usual
    /// `(1 - a) * backdrop + a * blend`.
    #[test]
    fn opaque_backdrop_matches_the_simple_formula() {
        let (cb, cs, a) = ([0.2, 0.5, 0.9], [0.7, 0.1, 0.4], 0.6);
        let (out, alpha) = BlendMode::Multiply.apply(cb, 1.0, cs, a);
        assert_eq!(alpha, 1.0);
        for c in 0..3 {
            let want = (1.0 - a) * cb[c] + a * cb[c] * cs[c];
            assert!((out[c] - want).abs() < 1e-6);
        }
    }

    #[test]
    fn parses_css_names_and_aliases() {
        assert_eq!(
            "multiply".parse::<BlendMode>().unwrap(),
            BlendMode::Multiply
        );
        assert_eq!(
            "Hard Light".parse::<BlendMode>().unwrap(),
            BlendMode::HardLight
        );
        assert_eq!("add".parse::<BlendMode>().unwrap(), BlendMode::PlusLighter);
        assert!("glow".parse::<BlendMode>().is_err());
    }

    #[test]
    fn difference_with_white_inverts() {
        let layer = Layer::new(solid([255, 255, 255, 255]), BlendMode::Difference);
        let img = VitralImage::from_layers(1, 1, &[layer]).unwrap();
        let mut bd = solid([10, 100, 250, 255]);
        img.composite_onto(&mut bd, 0, 0);
        assert_eq!(&bd.data[..3], &[245, 155, 5]);
    }
}
