//! Generates the images used by the web demo in `web/samples/`.
//!
//!     cargo run --release -p vitral --example demo_samples
//!
//! Each image is authored as a stack of blend-mode layers, like in a design
//! tool, and flattened into a `.vitral.png`. The window, which opens the page,
//! is also written as the three exports a designer would make (over black,
//! over white and over transparency): the demo's encoder section starts from
//! them.

use std::fs;
use std::path::Path;

use vitral::{BlendMode, Layer, RgbaImage, VitralImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/samples");
    fs::create_dir_all(root.join("sources"))?;

    let images: [(&str, VitralImage); 4] = [
        ("window", window()),
        ("shadow", shadow()),
        ("stamp", stamp()),
        ("light", light()),
    ];
    for (name, img) in images {
        if name == "window" {
            let (w, h) = (img.width, img.height);
            fs::write(
                root.join("sources/window-over-black.png"),
                vitral::write_rgb8(w, h, &img.black)?,
            )?;
            fs::write(
                root.join("sources/window-over-white.png"),
                vitral::write_rgb8(w, h, &img.white)?,
            )?;
            fs::write(
                root.join("sources/window-over-transparent.png"),
                vitral::write_rgba8(&img.native)?,
            )?;
        }
        let bytes = vitral::encode(&img)?;
        fs::write(root.join(format!("{name}.vitral.png")), &bytes)?;
        println!(
            "{name:8} {:>4}x{:<4} {:>7} bytes",
            img.width,
            img.height,
            bytes.len()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- samples ---

/// A pane of the window: x0, y0, x1, y1, blend mode, colour and opacity.
type Pane = (f32, f32, f32, f32, BlendMode, [u8; 3], f32);

/// A stained glass window cut into rectangles: tinted panes in multiply, one
/// pane of light in screen, one that inverts in difference, joined by lead in
/// normal mode.
fn window() -> VitralImage {
    let (w, h) = (360, 360);
    let panes: [Pane; 7] = [
        (
            0.0,
            0.0,
            150.0,
            210.0,
            BlendMode::Multiply,
            [52, 84, 212],
            0.88,
        ),
        (
            150.0,
            0.0,
            360.0,
            110.0,
            BlendMode::Multiply,
            [242, 184, 48],
            0.9,
        ),
        (
            150.0,
            110.0,
            260.0,
            210.0,
            BlendMode::Screen,
            [255, 176, 96],
            0.95,
        ),
        (
            260.0,
            110.0,
            360.0,
            260.0,
            BlendMode::Multiply,
            [236, 92, 52],
            0.88,
        ),
        (
            0.0,
            210.0,
            90.0,
            360.0,
            BlendMode::Multiply,
            [40, 140, 128],
            0.88,
        ),
        (
            90.0,
            210.0,
            260.0,
            360.0,
            BlendMode::Multiply,
            [214, 216, 222],
            0.9,
        ),
        (
            260.0,
            260.0,
            360.0,
            360.0,
            BlendMode::Difference,
            [255, 255, 255],
            1.0,
        ),
    ];
    let pane_at = move |x: f32, y: f32| {
        panes
            .iter()
            .find(|p| x >= p.0 && x < p.2 && y >= p.1 && y < p.3)
            .copied()
    };
    let glass = |mode: BlendMode| {
        paint(w, h, move |x, y| match pane_at(x, y) {
            Some((x0, y0, x1, y1, m, colour, alpha)) if m == mode => {
                rgba(colour, coverage(rect(x, y, x0, y0, x1, y1) - 3.0) * alpha)
            }
            _ => [0.0; 4],
        })
    };
    let lead = paint(w, h, move |x, y| {
        let inner = pane_at(x, y).map_or(0.0, |(x0, y0, x1, y1, ..)| {
            coverage(3.5 - rect(x, y, x0, y0, x1, y1))
        });
        let frame = coverage(6.0 - rect(x, y, 0.0, 0.0, 360.0, 360.0));
        rgba([26, 26, 30], inner.max(frame))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(glass(BlendMode::Multiply), BlendMode::Multiply),
            Layer::new(glass(BlendMode::Screen), BlendMode::Screen),
            Layer::new(glass(BlendMode::Difference), BlendMode::Difference),
            Layer::new(lead, BlendMode::Normal),
        ],
    )
    .unwrap()
}

/// A ceramic ball and its shadow. The shadow is in multiply, so it darkens
/// whatever it falls on instead of greying it.
fn shadow() -> VitralImage {
    let (w, h) = (360, 260);
    let (cx, cy, r) = (176.0, 100.0, 72.0);
    let soft = |ex: f32, ey: f32, rx: f32, ry: f32, sigma: f32| {
        blur(
            &alpha_mask(w, h, |x, y| {
                let d = ((x - ex) / rx).powi(2) + ((y - ey) / ry).powi(2);
                if d <= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }),
            w,
            h,
            sigma,
        )
    };
    let cast = soft(196.0, 196.0, 104.0, 20.0, 14.0);
    let contact = soft(180.0, 176.0, 44.0, 7.0, 4.0);
    let shade = paint(w, h, |x, y| {
        let i = idx(w, x, y);
        // Multiplying by this grey keeps about a third of the backdrop's light,
        // and all of its hue.
        let a = (cast[i] * 0.9 + contact[i] * 0.6).min(1.0);
        rgba([86, 88, 104], a)
    });
    let ball = paint(w, h, |x, y| {
        let (nx, ny) = ((x - cx) / r, (y - cy) / r);
        let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
        let light = (-0.45 * nx - 0.6 * ny + 0.66 * nz).max(0.0);
        let spec = light.powi(24) * 0.35;
        let tone = 0.32 + 0.68 * light;
        let c = [236.0, 232.0, 224.0].map(|v: f32| (v * tone / 255.0 + spec).min(1.0));
        [c[0], c[1], c[2], coverage(circle(x, y, cx, cy, r))]
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(shade, BlendMode::Multiply),
            Layer::new(ball, BlendMode::Normal),
        ],
    )
    .unwrap()
}

/// A round logo printed in one ink: multiply, with a little unevenness, so it
/// sits on any backdrop like ink on paper.
fn stamp() -> VitralImage {
    let (w, h) = (300, 300);
    let (cx, cy) = (150.0, 150.0);
    let ink = paint(w, h, |x, y| {
        let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        let outer = coverage(140.0 - r) * coverage(r - 126.0);
        let inner = coverage(116.0 - r) * coverage(r - 112.0);
        let disc = coverage(circle(x, y, cx - 20.0, cy, 48.0));
        let square = coverage(rect(x, y, cx - 4.0, cy - 44.0, cx + 64.0, cy + 44.0));
        let mark = disc.max(square) * (1.0 - disc.min(square));
        let grain = 0.8 + 0.2 * noise((x / 2.0).floor(), (y / 2.0).floor());
        rgba([30, 52, 150], outer.max(inner).max(mark) * grain * 0.94)
    });
    VitralImage::from_layers(w, h, &[Layer::new(ink, BlendMode::Multiply)]).unwrap()
}

/// A beam of warm light in screen mode, like the light leaks photographers
/// buy: it can only brighten what it falls on.
fn light() -> VitralImage {
    let (w, h) = (420, 280);
    // Light fades out before the edges, so the frame of the image never shows.
    let edge = move |x: f32, y: f32| {
        let ramp = |d: f32| {
            let t = (d / 80.0).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        ramp(x) * ramp(w as f32 - x) * ramp(y) * ramp(h as f32 - y)
    };
    let glow = |gx: f32, gy: f32, rad: f32, colour: [u8; 3], strength: f32| {
        paint(w, h, move |x, y| {
            let d2 = ((x - gx).powi(2) + (y - gy).powi(2)) / (rad * rad);
            rgba(colour, (-d2).exp() * strength * edge(x, y))
        })
    };
    let beam = paint(w, h, |x, y| {
        // Distance from the line through (0, 30) along (1, 0.6), normalised.
        let (dx, dy) = (1.0 / 1.166, 0.6 / 1.166);
        let (px, py) = (x, y - 30.0);
        let across = px * dy - py * dx;
        let along = px * dx + py * dy;
        let fade = (1.0 - along / 460.0).clamp(0.0, 1.0);
        rgba(
            [255, 206, 150],
            (-(across / 34.0).powi(2)).exp() * fade * 0.8 * edge(x, y),
        )
    });
    let flare = paint(w, h, |x, y| {
        let d = circle(x, y, 318.0, 214.0, 22.0);
        let ring = coverage(d + 1.0) * coverage(1.0 - d);
        rgba([255, 236, 214], (coverage(d) * 0.35).max(ring * 0.6))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(
                glow(110.0, 84.0, 120.0, [255, 112, 48], 0.95),
                BlendMode::Screen,
            ),
            Layer::new(beam, BlendMode::Screen),
            Layer::new(flare, BlendMode::Screen),
        ],
    )
    .unwrap()
}

/// Repeatable value noise in [0, 1] for one cell.
fn noise(x: f32, y: f32) -> f32 {
    let mut n =
        (x as i32 as u32).wrapping_mul(374_761_393) ^ (y as i32 as u32).wrapping_mul(668_265_263);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65_535.0
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
