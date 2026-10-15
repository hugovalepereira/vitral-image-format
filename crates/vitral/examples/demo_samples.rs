//! Generates the images used by the web demo in `web/samples/`.
//!
//!     cargo run --release -p vitral --example demo_samples
//!
//! Each image is authored as a stack of blend-mode layers, like in a design
//! tool, and flattened into a `.vitral.png`. The colour gels sample is also
//! written as the three exports a designer would make (over black, over white
//! and over transparency), which the demo's encoder section starts from.

use std::fs;
use std::path::Path;

use vitral::{BlendMode, Layer, RgbaImage, VitralImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/samples");
    fs::create_dir_all(root.join("sources"))?;

    let images: [(&str, VitralImage); 5] = [
        ("gels", gels()),
        ("glass-card", glass_card()),
        ("lumen", lumen()),
        ("inverse", inverse()),
        ("hero", hero()),
    ];
    for (name, img) in images {
        if name == "gels" {
            let (w, h) = (img.width, img.height);
            fs::write(
                root.join("sources/gels-over-black.png"),
                vitral::write_rgb8(w, h, &img.black)?,
            )?;
            fs::write(
                root.join("sources/gels-over-white.png"),
                vitral::write_rgb8(w, h, &img.white)?,
            )?;
            fs::write(
                root.join("sources/gels-over-transparent.png"),
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

/// Three overlapping colour gels in multiply: two panes and a disc.
fn gels() -> VitralImage {
    let (w, h) = (360, 240);
    let pane = |x0: f32, y0: f32, x1: f32, y1: f32, colour: [u8; 3]| {
        paint(w, h, move |x, y| {
            rgba(colour, coverage(rect(x, y, x0, y0, x1, y1)) * 0.88)
        })
    };
    let disc = paint(w, h, |x, y| {
        rgba(
            [244, 184, 48],
            coverage(circle(x, y, 262.0, 138.0, 74.0)) * 0.88,
        )
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(
                pane(24.0, 28.0, 176.0, 208.0, [48, 84, 212]),
                BlendMode::Multiply,
            ),
            Layer::new(
                pane(112.0, 64.0, 292.0, 168.0, [236, 92, 52]),
                BlendMode::Multiply,
            ),
            Layer::new(disc, BlendMode::Multiply),
        ],
    )
    .unwrap()
}

/// A frosted interface card: soft multiply shadow, a faint white body, a
/// screen highlight, a hairline rim and two lines of placeholder text.
fn glass_card() -> VitralImage {
    let (w, h) = (360, 240);
    let card =
        |dy: f32| move |x: f32, y: f32| rounded_rect(x, y - dy, 180.0, 112.0, 140.0, 78.0, 16.0);
    let shadow_alpha = blur(
        &alpha_mask(w, h, |x, y| coverage(card(18.0)(x, y))),
        w,
        h,
        16.0,
    );
    let shadow = paint(w, h, |x, y| {
        rgba([12, 16, 28], shadow_alpha[idx(w, x, y)] * 0.5)
    });
    let body = paint(w, h, |x, y| {
        rgba([255, 255, 255], coverage(card(0.0)(x, y)) * 0.18)
    });
    let highlight = paint(w, h, |x, y| {
        let fade = ((120.0 - y) / 90.0).clamp(0.0, 1.0);
        rgba(
            [255, 255, 255],
            coverage(card(0.0)(x, y) - 1.0) * fade * 0.35,
        )
    });
    let marks = paint(w, h, |x, y| {
        let rim = {
            let d = card(0.0)(x, y);
            coverage(d) * coverage(1.2 - d) * 0.6
        };
        let dot = coverage(circle(x, y, 76.0, 72.0, 12.0));
        let title = coverage(rounded_rect(x, y, 132.0, 120.0, 60.0, 5.0, 5.0)) * 0.9;
        let line = coverage(rounded_rect(x, y, 154.0, 144.0, 82.0, 3.5, 3.5)) * 0.6;
        if dot > 0.0 {
            rgba([240, 96, 48], dot)
        } else {
            rgba([255, 255, 255], rim.max(title).max(line))
        }
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(shadow, BlendMode::Multiply),
            Layer::new(body, BlendMode::Normal),
            Layer::new(highlight, BlendMode::Screen),
            Layer::new(marks, BlendMode::Normal),
        ],
    )
    .unwrap()
}

/// A disc of warm light in screen mode, ringed by a hairline.
fn lumen() -> VitralImage {
    let (w, h) = (300, 300);
    let (cx, cy) = (150.0, 150.0);
    let glow = |r: f32, colour: [u8; 3], strength: f32| {
        paint(w, h, move |x, y| {
            let d2 = ((x - cx).powi(2) + (y - cy).powi(2)) / (r * r);
            rgba(colour, (-d2).exp() * strength)
        })
    };
    let ring = paint(w, h, |x, y| {
        let d = circle(x, y, cx, cy, 132.0);
        rgba(
            [255, 255, 255],
            coverage(d + 0.75) * coverage(0.75 - d) * 0.75,
        )
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(glow(64.0, [255, 176, 104], 0.95), BlendMode::Screen),
            Layer::new(glow(30.0, [255, 244, 226], 0.95), BlendMode::Screen),
            Layer::new(ring, BlendMode::Screen),
        ],
    )
    .unwrap()
}

/// A disc with a square cut out of it, in difference: it inverts whatever is
/// behind it, except inside the square.
fn inverse() -> VitralImage {
    let (w, h) = (240, 240);
    let shape = paint(w, h, |x, y| {
        let disc = coverage(circle(x, y, 120.0, 120.0, 112.0));
        let hole = coverage(rect(x, y, 84.0, 84.0, 156.0, 156.0));
        rgba([255, 255, 255], disc * (1.0 - hole))
    });
    VitralImage::from_layers(w, h, &[Layer::new(shape, BlendMode::Difference)]).unwrap()
}

/// The opening image: one pane in three strips, multiply, screen and
/// difference, inside a thin frame.
fn hero() -> VitralImage {
    let (w, h) = (360, 200);
    let strip = |x0: f32, colour: [u8; 3], alpha: f32| {
        paint(w, h, move |x, y| {
            rgba(
                colour,
                coverage(rect(x, y, x0, 0.0, x0 + 120.0, 200.0)) * alpha,
            )
        })
    };
    let frame = paint(w, h, |x, y| {
        let d = rect(x, y, 0.0, 0.0, 360.0, 200.0);
        rgba([24, 24, 26], coverage(d) * coverage(1.5 - d))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(strip(0.0, [48, 84, 212], 0.95), BlendMode::Multiply),
            Layer::new(strip(120.0, [255, 184, 96], 0.95), BlendMode::Screen),
            Layer::new(strip(240.0, [255, 255, 255], 1.0), BlendMode::Difference),
            Layer::new(frame, BlendMode::Normal),
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

fn rounded_rect(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (x - cx).abs() - (hw - r);
    let qy = (y - cy).abs() - (hh - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    r - outside - qx.max(qy).min(0.0)
}

fn idx(w: u32, x: f32, y: f32) -> usize {
    (y as usize) * w as usize + x as usize
}

fn alpha_mask(w: u32, h: u32, f: impl Fn(f32, f32) -> f32) -> Vec<f32> {
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            out.push(f(x as f32 + 0.5, y as f32 + 0.5));
        }
    }
    out
}

/// Approximate Gaussian blur: three box blurs in each direction.
fn blur(src: &[f32], w: u32, h: u32, sigma: f32) -> Vec<f32> {
    let radius = ((sigma * sigma * 4.0 + 1.0).sqrt() / 2.0).round() as i32;
    let mut a = src.to_vec();
    for _ in 0..3 {
        a = box_blur(&a, w as i32, h as i32, radius, true);
        a = box_blur(&a, w as i32, h as i32, radius, false);
    }
    a
}

fn box_blur(src: &[f32], w: i32, h: i32, r: i32, horizontal: bool) -> Vec<f32> {
    let mut out = vec![0.0; src.len()];
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    for o in 0..outer {
        for i in 0..inner {
            let mut sum = 0.0;
            for k in -r..=r {
                let j = (i + k).clamp(0, inner - 1);
                let (x, y) = if horizontal { (j, o) } else { (o, j) };
                sum += src[(y * w + x) as usize];
            }
            let (x, y) = if horizontal { (i, o) } else { (o, i) };
            out[(y * w + x) as usize] = sum / (2 * r + 1) as f32;
        }
    }
    out
}
