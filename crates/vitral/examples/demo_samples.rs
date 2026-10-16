//! Generates the images used by the web demo in `web/samples/`.
//!
//!     cargo run --release -p vitral --example demo_samples
//!
//! Each image is authored as a stack of blend-mode layers, like in a design
//! tool, and flattened into a `.vitral.png`. Only modes Vitral 0.1 keeps
//! exactly are used: multiply, screen and inversion (difference with white).
//! Images are drawn at twice the size the page shows them. The poster, which
//! opens the page, is also written as the three exports a designer would make
//! (over black, over white and over transparency): the demo's encoder section
//! starts from them.

use std::fs;
use std::path::Path;

use vitral::{BlendMode, Layer, RgbaImage, VitralImage};

const WHITE: [u8; 3] = [255, 255, 255];
const ULTRAMARINE: [u8; 3] = [44, 72, 214];
const ORANGE: [u8; 3] = [233, 84, 32];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/samples");
    fs::create_dir_all(root.join("sources"))?;

    let images: [(&str, VitralImage); 4] = [
        ("poster", poster()),
        ("mark", mark()),
        ("overprint", overprint()),
        ("bars", bars()),
    ];
    for (name, img) in images {
        if name == "poster" {
            let (w, h) = (img.width, img.height);
            fs::write(
                root.join("sources/poster-over-black.png"),
                vitral::write_rgb8(w, h, &img.black)?,
            )?;
            fs::write(
                root.join("sources/poster-over-white.png"),
                vitral::write_rgb8(w, h, &img.white)?,
            )?;
            fs::write(
                root.join("sources/poster-over-transparent.png"),
                vitral::write_rgba8(&img.native)?,
            )?;
        }
        let bytes = vitral::encode(&img)?;
        fs::write(root.join(format!("{name}.vitral.png")), &bytes)?;
        println!(
            "{name:10} {:>4}x{:<4} {:>7} bytes",
            img.width,
            img.height,
            bytes.len()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- samples ---

/// One shape per square of a 4 x 2 grid of 200 px squares, the grid the page
/// behind it uses: a disc that inverts, a bar in multiply and a half disc in
/// multiply.
fn poster() -> VitralImage {
    let (w, h) = (800, 400);
    let disc = paint(w, h, |x, y| {
        rgba(WHITE, coverage(circle(x, y, 200.0, 200.0, 200.0)))
    });
    let inks = paint(w, h, |x, y| {
        let bar = coverage(rect(x, y, 400.0, 0.0, 800.0, 200.0));
        let half = coverage(circle(x, y, 600.0, 200.0, 200.0)) * coverage(y - 200.0);
        if bar > 0.0 {
            rgba(ULTRAMARINE, bar)
        } else {
            rgba(ORANGE, half)
        }
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(inks, BlendMode::Multiply),
            Layer::new(disc, BlendMode::Difference),
        ],
    )
    .unwrap()
}

/// A logo: a square of ink in multiply and a disc that inverts, overlapping
/// so that the overlap inverts the inked backdrop.
fn mark() -> VitralImage {
    let (w, h) = (600, 600);
    let square = paint(w, h, |x, y| {
        rgba(ULTRAMARINE, coverage(rect(x, y, 60.0, 60.0, 380.0, 380.0)))
    });
    let disc = paint(w, h, |x, y| {
        rgba(WHITE, coverage(circle(x, y, 380.0, 380.0, 180.0)))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(square, BlendMode::Multiply),
            Layer::new(disc, BlendMode::Difference),
        ],
    )
    .unwrap()
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

/// Bars on a steady rhythm that invert what they cross, over a band of light
/// in screen.
fn bars() -> VitralImage {
    let (w, h) = (720, 480);
    // Bar widths in units of 20 px, separated by gaps of one unit.
    let widths = [2.0, 1.0, 4.0, 1.0, 2.0, 6.0, 1.0, 3.0, 1.0, 2.0];
    let mut edges = Vec::new();
    let mut at = 1.0;
    for wd in widths {
        edges.push((at * 20.0, (at + wd) * 20.0));
        at += wd + 1.0;
    }
    let invert = paint(w, h, move |x, y| {
        let a = edges
            .iter()
            .map(|&(x0, x1)| coverage(rect(x, y, x0, 0.0, x1, 480.0)))
            .fold(0.0, f32::max);
        rgba(WHITE, a)
    });
    let band = paint(w, h, |x, y| {
        rgba(ORANGE, coverage(rect(x, y, 0.0, 300.0, 720.0, 380.0)))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(band, BlendMode::Screen),
            Layer::new(invert, BlendMode::Difference),
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
