//! Generates the images used by the web demo in `web/samples/`.
//!
//!     cargo run --release -p vitral --example demo_samples
//!
//! Each image is authored as a stack of blend-mode layers, like in a design
//! tool, and flattened into a `.vitral.png`. Only modes Vitral 0.1 keeps
//! exactly are used: normal, multiply, screen and inversion (difference with
//! white). Images are drawn at twice the size the page shows them. The lens,
//! which opens the page, is also written as the three exports a designer
//! would make (over black, over white and over transparency): the demo's
//! encoder section starts from them.

use std::f32::consts::PI;
use std::fs;
use std::path::Path;

use vitral::{BlendMode, Layer, RgbaImage, VitralImage};

const WHITE: [u8; 3] = [255, 255, 255];
const INK: [u8; 3] = [20, 20, 22];
const ULTRAMARINE: [u8; 3] = [44, 72, 214];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/samples");
    fs::create_dir_all(root.join("sources"))?;

    let images: [(&str, VitralImage); 8] = [
        ("lens", lens()),
        ("highlighter", highlighter()),
        ("sunglasses", sunglasses()),
        ("film", film()),
        ("blinds", blinds()),
        ("tape", tape()),
        ("cup", cup()),
        ("overprint", overprint()),
    ];
    for (name, img) in images {
        if name == "lens" {
            let (w, h) = (img.width, img.height);
            fs::write(
                root.join("sources/lens-over-black.png"),
                vitral::write_rgb8(w, h, &img.black)?,
            )?;
            fs::write(
                root.join("sources/lens-over-white.png"),
                vitral::write_rgb8(w, h, &img.white)?,
            )?;
            fs::write(
                root.join("sources/lens-over-transparent.png"),
                vitral::write_rgba8(&img.native)?,
            )?;
        }
        let bytes = vitral::encode(&img)?;
        fs::write(root.join(format!("{name}.vitral.png")), &bytes)?;
        println!(
            "{name:12} {:>4}x{:<4} {:>7} bytes",
            img.width,
            img.height,
            bytes.len()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- samples ---

/// The object dropped on the page at the top: a disc of fine bands that
/// invert, a smaller disc of ink in multiply, and the soft shadow both cast,
/// also in multiply, so it reads as lying on top of the page.
fn lens() -> VitralImage {
    let (w, h) = (640, 640);
    let (bx, by, br) = (290.0, 280.0, 230.0);
    let (dx, dy, dr) = (470.0, 455.0, 120.0);
    let shadow = paint(w, h, |x, y| {
        let soft = |cx: f32, cy: f32, r: f32| smooth(circle(x, y, cx + 18.0, cy + 26.0, r), 34.0);
        // The shadow falls only around the discs, not through them.
        let under = coverage(circle(x, y, bx, by, br)).max(coverage(circle(x, y, dx, dy, dr)));
        rgba(
            [70, 72, 86],
            soft(bx, by, br).max(soft(dx, dy, dr)) * 0.45 * (1.0 - under),
        )
    });
    let ink = paint(w, h, |x, y| {
        rgba(ULTRAMARINE, coverage(circle(x, y, dx, dy, dr)))
    });
    let bands = paint(w, h, |x, y| {
        // Bands 56 px apart, 28 px thick, inside the disc.
        let band = coverage(14.0 - ((y - by + 14.0).rem_euclid(56.0) - 28.0).abs());
        rgba(WHITE, band * coverage(circle(x, y, bx, by, br)))
    });
    let rim = paint(w, h, |x, y| {
        let d = circle(x, y, bx, by, br);
        rgba(INK, coverage(d + 1.5) * coverage(1.5 - d))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(shadow, BlendMode::Multiply),
            Layer::new(ink, BlendMode::Multiply),
            Layer::new(bands, BlendMode::Difference),
            Layer::new(rim, BlendMode::Normal),
        ],
    )
    .unwrap()
}

/// Three strokes of highlighter: the ink is multiply, so the text under it
/// stays readable.
fn highlighter() -> VitralImage {
    let (w, h) = (720, 440);
    let strokes = [
        (330.0, 110.0, 280.0, -2.0, [255, 226, 0]),
        (300.0, 220.0, 230.0, 1.5, [255, 226, 0]),
        (390.0, 330.0, 300.0, -1.0, [255, 128, 176]),
    ];
    let layer = paint(w, h, move |x, y| {
        let mut out = [0.0; 4];
        for (cx, cy, half, deg, ink) in strokes {
            let a = coverage(-boxd(x, y, cx, cy, half, 30.0, deg, 10.0)) * 0.92;
            if a > out[3] {
                out = rgba(ink, a);
            }
        }
        out
    });
    VitralImage::from_layers(w, h, &[Layer::new(layer, BlendMode::Multiply)]).unwrap()
}

/// Sunglasses: tinted lenses in multiply, darker at the top, in a black
/// frame.
fn sunglasses() -> VitralImage {
    let (w, h) = (720, 300);
    let lenses = [(200.0, 150.0), (520.0, 150.0)];
    let lens_d = move |x: f32, y: f32| {
        lenses
            .iter()
            .map(|&(cx, cy)| -boxd(x, y, cx, cy, 130.0, 95.0, 0.0, 70.0))
            .fold(f32::MIN, f32::max)
    };
    let tint = paint(w, h, move |x, y| {
        let t = (y - 55.0) / 190.0;
        let shade = 0.25 + 0.35 * t.clamp(0.0, 1.0);
        let c = [70.0, 54.0, 40.0].map(|v: f32| v / 255.0 + shade * 0.6);
        [c[0], c[1], c[2], coverage(lens_d(x, y) - 2.0)]
    });
    let frame = paint(w, h, move |x, y| {
        let d = lens_d(x, y);
        let rim = coverage(d + 14.0) * coverage(2.0 - d);
        let bridge = coverage(-boxd(x, y, 360.0, 112.0, 40.0, 9.0, 0.0, 9.0));
        rgba(INK, rim.max(bridge))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(tint, BlendMode::Multiply),
            Layer::new(frame, BlendMode::Normal),
        ],
    )
    .unwrap()
}

/// A strip of colour negative: an orange base in multiply, frames that invert
/// what is behind them, and the sprocket holes cut out.
fn film() -> VitralImage {
    let (w, h) = (760, 360);
    let hole = |x: f32, y: f32| {
        let row = if y < 180.0 { 34.0 } else { 326.0 };
        let cx = (x / 48.0).floor() * 48.0 + 24.0;
        coverage(-boxd(x, y, cx, row, 11.0, 14.0, 0.0, 4.0))
    };
    let frame = |x: f32, y: f32| {
        let cx = ((x - 20.0) / 240.0).floor() * 240.0 + 140.0;
        coverage(-boxd(x, y, cx, 180.0, 108.0, 112.0, 0.0, 6.0))
    };
    let base = paint(w, h, move |x, y| {
        let strip = coverage(rect(x, y, 0.0, 0.0, 760.0, 360.0));
        rgba(
            [232, 142, 78],
            strip * (1.0 - hole(x, y)) * (1.0 - frame(x, y)),
        )
    });
    let invert = paint(w, h, move |x, y| rgba(WHITE, frame(x, y)));
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(base, BlendMode::Multiply),
            Layer::new(invert, BlendMode::Difference),
        ],
    )
    .unwrap()
}

/// Sunlight through blinds: slanted bands of warm light in screen.
fn blinds() -> VitralImage {
    let (w, h) = (760, 520);
    let light = paint(w, h, |x, y| {
        // Bands across a skewed axis, fading out towards the edges.
        let u = y - 0.35 * x;
        let band = smooth(18.0 - ((u).rem_euclid(64.0) - 32.0).abs(), 6.0);
        let fade = smooth(rect(x, y, 0.0, 0.0, 760.0, 520.0), 120.0);
        rgba([255, 196, 132], band * fade * 0.9)
    });
    VitralImage::from_layers(w, h, &[Layer::new(light, BlendMode::Screen)]).unwrap()
}

/// Two strips of striped paper tape crossing, in multiply.
fn tape() -> VitralImage {
    let (w, h) = (680, 480);
    let strip = |cx: f32, cy: f32, deg: f32, ink: [u8; 3]| {
        paint(w, h, move |x, y| {
            let (u, v) = local(x, y, cx, cy, deg);
            // Notched ends, like tape torn by hand.
            let notch = 6.0 * ((v / 9.0).rem_euclid(2.0) - 1.0).abs();
            let d = (290.0 - notch - u.abs()).min(46.0 - v.abs());
            let stripe = if (u / 18.0).rem_euclid(2.0) < 1.0 {
                1.0
            } else {
                0.55
            };
            rgba(ink, coverage(d) * stripe * 0.9)
        })
    };
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(
                strip(340.0, 200.0, -12.0, [96, 196, 214]),
                BlendMode::Multiply,
            ),
            Layer::new(
                strip(330.0, 280.0, 24.0, [246, 130, 150]),
                BlendMode::Multiply,
            ),
        ],
    )
    .unwrap()
}

/// The ring a coffee cup leaves on paper, in multiply.
fn cup() -> VitralImage {
    let (w, h) = (600, 600);
    let ring = paint(w, h, |x, y| {
        let (dx, dy) = (x - 300.0, y - 300.0);
        let a = dy.atan2(dx);
        let r = (dx * dx + dy * dy).sqrt();
        let width = 9.0 + 7.0 * (a * 2.0 + 0.6).sin().max(0.0) + 3.0 * (a * 5.0).sin();
        let edge = coverage(width - (r - 218.0).abs());
        let stain = coverage(212.0 - r) * 0.12;
        let drip = coverage(circle(
            x,
            y,
            300.0 + 236.0 * (0.9 * PI).cos(),
            300.0 + 236.0 * (0.9 * PI).sin(),
            14.0,
        ));
        rgba([150, 92, 48], edge.max(stain).max(drip) * 0.85)
    });
    VitralImage::from_layers(w, h, &[Layer::new(ring, BlendMode::Multiply)]).unwrap()
}

/// Three discs of process ink in multiply, overprinted like a risograph.
fn overprint() -> VitralImage {
    let (w, h) = (680, 640);
    let r = 190.0;
    let disc = |cx: f32, cy: f32, ink: [u8; 3]| {
        paint(w, h, move |x, y| {
            rgba(ink, coverage(circle(x, y, cx, cy, r)))
        })
    };
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(disc(230.0, 220.0, [0, 164, 222]), BlendMode::Multiply),
            Layer::new(disc(450.0, 220.0, [236, 40, 132]), BlendMode::Multiply),
            Layer::new(disc(340.0, 420.0, [255, 214, 0]), BlendMode::Multiply),
        ],
    )
    .unwrap()
}

// ---------------------------------------------------------------- helpers ---

/// Renders a layer by evaluating `f` at each pixel centre; `f` returns straight
/// RGBA in `[0, 1]`.
fn paint(w: u32, h: u32, f: impl Fn(f32, f32) -> [f32; 4]) -> RgbaImage {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let px = f(x as f32 + 0.5, y as f32 + 0.5);
            data.extend(px.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
        }
    }
    RgbaImage::new(w, h, data).unwrap()
}

fn rgba(c: [u8; 3], alpha: f32) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        alpha,
    ]
}

/// Antialiased coverage from a signed distance (positive inside).
fn coverage(d: f32) -> f32 {
    (d + 0.5).clamp(0.0, 1.0)
}

/// Signed distance to an axis-aligned rectangle (positive inside).
fn rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    (x - x0).min(x1 - x).min(y - y0).min(y1 - y)
}

/// Signed distance to a circle (positive inside).
fn circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> f32 {
    r - ((x - cx).powi(2) + (y - cy).powi(2)).sqrt()
}

/// Smooth coverage from a signed distance, over a soft edge `soft` px wide.
fn smooth(d: f32, soft: f32) -> f32 {
    let t = (d / soft + 0.5).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Coordinates of (x, y) in a frame centred on (cx, cy) and turned by `deg`.
fn local(x: f32, y: f32, cx: f32, cy: f32, deg: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().sin_cos();
    let (dx, dy) = (x - cx, y - cy);
    (dx * c + dy * s, -dx * s + dy * c)
}

/// Signed distance to a rounded box (negative inside), turned by `deg`.
#[allow(clippy::too_many_arguments)]
fn boxd(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, deg: f32, radius: f32) -> f32 {
    let (u, v) = local(x, y, cx, cy, deg);
    let qx = u.abs() - hw + radius;
    let qy = v.abs() - hh + radius;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}
