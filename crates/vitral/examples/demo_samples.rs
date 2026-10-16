//! Generates the images used by the web demo in `web/samples/`.
//!
//!     cargo run --release -p vitral --example demo_samples
//!
//! Each image is authored as a stack of blend-mode layers, like in a design
//! tool, and flattened into a `.vitral.png`. Only modes Vitral 0.1 keeps
//! exactly are used: normal, multiply, screen and inversion (difference with
//! white). Images are drawn at twice the size the page shows them. The ring,
//! which opens the page, is also written as the three exports a designer
//! would make (over black, over white and over transparency): the demo's
//! encoder section starts from them, and the page shows the transparent one
//! as the plain PNG next to the Vitral.

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

    let images: [(&str, VitralImage); 12] = [
        ("ring", ring()),
        ("overprint", overprint()),
        ("highlighter", highlighter()),
        ("sunglasses", sunglasses()),
        ("film", film()),
        ("blinds", blinds()),
        ("tape", tape()),
        ("cup", cup()),
        ("set-square", set_square()),
        ("halftone", halftone()),
        ("neon", neon()),
        ("postmark", postmark()),
    ];
    for (name, img) in images {
        if name == "ring" {
            let (w, h) = (img.width, img.height);
            fs::write(root.join("ring.png"), vitral::write_rgba8(&img.native)?)?;
            fs::write(
                root.join("sources/ring-over-black.png"),
                vitral::write_rgb8(w, h, &img.black)?,
            )?;
            fs::write(
                root.join("sources/ring-over-white.png"),
                vitral::write_rgb8(w, h, &img.white)?,
            )?;
            fs::write(
                root.join("sources/ring-over-transparent.png"),
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

/// The object dropped on the page at the top: a letter O of horizontal bands
/// that invert, thin at the top and thick at the bottom, a disc of ink in
/// multiply caught in its counter, and the soft shadow the two cast, also in
/// multiply, so it reads as lying on top of the page.
fn ring() -> VitralImage {
    let (w, h) = (640, 640);
    let o = |x: f32, y: f32| {
        let outer = circle(x, y, 300.0, 300.0, 250.0);
        let counter = circle(x, y, 336.0, 304.0, 118.0);
        outer.min(-counter)
    };
    let (dx, dy, dr) = (410.0, 392.0, 86.0);
    let shadow = paint(w, h, |x, y| {
        let cast = smooth(o(x - 18.0, y - 26.0), 34.0)
            .max(smooth(circle(x, y, dx + 18.0, dy + 26.0, dr), 34.0));
        let under = coverage(o(x, y)).max(coverage(circle(x, y, dx, dy, dr)));
        rgba([70, 72, 86], cast * 0.45 * (1.0 - under))
    });
    let ink = paint(w, h, |x, y| {
        rgba(ULTRAMARINE, coverage(circle(x, y, dx, dy, dr)))
    });
    let bands = paint(w, h, |x, y| {
        // Bands every 32 px, from 6 px thick at the top to 26 px at the bottom.
        let t = 6.0 + 20.0 * ((y - 50.0) / 500.0).clamp(0.0, 1.0);
        let band = coverage(t / 2.0 - ((y - 50.0).rem_euclid(32.0) - 16.0).abs());
        rgba(WHITE, band * coverage(o(x, y)))
    });
    let rim = paint(w, h, |x, y| {
        let d = o(x, y);
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

/// Round sunglasses: lenses tinted in multiply, two glints of light in
/// screen across each lens, and a thin black frame.
fn sunglasses() -> VitralImage {
    let (w, h) = (720, 300);
    let lenses = [(210.0, 160.0), (510.0, 160.0)];
    let r = 112.0;
    let lens_d = move |x: f32, y: f32| {
        lenses
            .iter()
            .map(|&(cx, cy)| circle(x, y, cx, cy, r))
            .fold(f32::MIN, f32::max)
    };
    let tint = paint(w, h, move |x, y| {
        let t = ((y - 48.0) / 224.0).clamp(0.0, 1.0);
        let c = [44.0, 70.0, 120.0].map(|v: f32| (v + 120.0 * t) / 255.0);
        [c[0], c[1], c[2], coverage(lens_d(x, y))]
    });
    let glint = paint(w, h, move |x, y| {
        // Two diagonal bands per lens, 18 and 8 px wide.
        let u = x + y;
        let band = |c: f32, half: f32| coverage(half - (u - c).abs());
        let a = lenses
            .iter()
            .map(|&(cx, cy)| {
                let o = cx + cy;
                band(o - 70.0, 9.0).max(band(o - 36.0, 4.0))
                    * coverage(circle(x, y, cx, cy, r - 14.0))
            })
            .fold(0.0, f32::max);
        rgba(WHITE, a * 0.75)
    });
    let frame = paint(w, h, move |x, y| {
        let d = lens_d(x, y);
        let rim = coverage(d + 8.0) * coverage(1.0 - d);
        let br = ((x - 360.0).powi(2) + (y - 168.0).powi(2)).sqrt();
        let bridge = coverage(4.0 - (br - 52.0).abs()) * coverage(150.0 - y);
        let temples = coverage(-boxd(x, y, 52.0, 118.0, 52.0, 4.0, 0.0, 4.0))
            .max(coverage(-boxd(x, y, 668.0, 118.0, 52.0, 4.0, 0.0, 4.0)));
        rgba(INK, rim.max(bridge).max(temples))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(tint, BlendMode::Multiply),
            Layer::new(glint, BlendMode::Screen),
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

/// A transparent set square: a tinted triangle in multiply with a triangle
/// cut out of it and a scale printed along one edge.
fn set_square() -> VitralImage {
    let (w, h) = (600, 440);
    let outer = [(40.0, 400.0), (560.0, 400.0), (40.0, 60.0)];
    let inner = [(120.0, 350.0), (380.0, 350.0), (120.0, 180.0)];
    let body = paint(w, h, move |x, y| {
        let a = coverage(triangle(x, y, outer)) * (1.0 - coverage(triangle(x, y, inner)));
        rgba([150, 214, 200], a * 0.9)
    });
    let scale = paint(w, h, |x, y| {
        // Ticks every 10 px, longer every 50 px, along the bottom edge.
        let i = ((x - 50.0) / 10.0).round();
        let len = if i as i32 % 5 == 0 { 26.0 } else { 12.0 };
        let tick = coverage(1.5 - (x - 50.0 - i * 10.0).abs())
            * coverage(y - (396.0 - len))
            * coverage(396.0 - y);
        let inside = if (0.0..=48.0).contains(&i) { 1.0 } else { 0.0 };
        rgba([30, 40, 60], tick * inside)
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(body, BlendMode::Multiply),
            Layer::new(scale, BlendMode::Multiply),
        ],
    )
    .unwrap()
}

/// A disc of halftone dots in multiply, large in the middle and fading to
/// nothing at the edge, on a screen turned by 15 degrees.
fn halftone() -> VitralImage {
    let (w, h) = (600, 600);
    let dots = paint(w, h, |x, y| {
        let (u, v) = local(x, y, 300.0, 300.0, 15.0);
        let (cu, cv) = ((u / 24.0).round() * 24.0, (v / 24.0).round() * 24.0);
        let tone = (1.0 - (cu * cu + cv * cv).sqrt() / 280.0).clamp(0.0, 1.0);
        let d = 13.0 * tone.sqrt() - ((u - cu).powi(2) + (v - cv).powi(2)).sqrt();
        rgba([236, 40, 132], coverage(d))
    });
    VitralImage::from_layers(w, h, &[Layer::new(dots, BlendMode::Multiply)]).unwrap()
}

/// A neon sign of two tubes, a ring and a bar, in screen: each tube has a
/// bright core and a soft glow.
fn neon() -> VitralImage {
    let (w, h) = (720, 400);
    let tube = |d: f32, colour: [u8; 3]| {
        let core = coverage(5.0 - d.abs());
        let glow = (-(d / 26.0).powi(2)).exp() * 0.75;
        let hot = core * 0.6;
        let c = colour.map(|v| v as f32 / 255.0 * (1.0 - hot) + hot);
        [c[0], c[1], c[2], core.max(glow)]
    };
    let ring = paint(w, h, move |x, y| {
        let r = ((x - 210.0).powi(2) + (y - 200.0).powi(2)).sqrt();
        tube(r - 120.0, [255, 70, 150])
    });
    let bar = paint(w, h, move |x, y| {
        tube(
            boxd(x, y, 520.0, 200.0, 130.0, 0.0, 0.0, 0.0),
            [60, 210, 255],
        )
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(ring, BlendMode::Screen),
            Layer::new(bar, BlendMode::Screen),
        ],
    )
    .unwrap()
}

/// A postmark in one ink, in multiply: a double ring and wavy cancellation
/// lines.
fn postmark() -> VitralImage {
    let (w, h) = (600, 400);
    let ink = paint(w, h, |x, y| {
        let r = ((x - 170.0).powi(2) + (y - 200.0).powi(2)).sqrt();
        let rings = coverage(4.0 - (r - 128.0).abs()).max(coverage(2.0 - (r - 112.0).abs()));
        let waves = (0..5)
            .map(|i| {
                let base = 128.0 + 36.0 * i as f32;
                let wave = base + 9.0 * ((x - 300.0) / 46.0 * 2.0 * PI).sin();
                coverage(3.0 - (y - wave).abs()) * coverage(x - 300.0) * coverage(580.0 - x)
            })
            .fold(0.0, f32::max);
        rgba([36, 40, 52], rings.max(waves) * 0.88)
    });
    VitralImage::from_layers(w, h, &[Layer::new(ink, BlendMode::Multiply)]).unwrap()
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

/// Signed distance to a triangle (positive inside), from its three corners.
fn triangle(x: f32, y: f32, p: [(f32, f32); 3]) -> f32 {
    let (cx, cy) = (
        (p[0].0 + p[1].0 + p[2].0) / 3.0,
        (p[0].1 + p[1].1 + p[2].1) / 3.0,
    );
    (0..3)
        .map(|i| {
            let ((ax, ay), (bx, by)) = (p[i], p[(i + 1) % 3]);
            let len = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
            let side = |px: f32, py: f32| ((bx - ax) * (py - ay) - (by - ay) * (px - ax)) / len;
            side(x, y) * side(cx, cy).signum()
        })
        .fold(f32::MAX, f32::min)
}
