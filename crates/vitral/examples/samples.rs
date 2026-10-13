//! Generates the sample images in `examples/` at the repository root.
//!
//!     cargo run --release -p vitral --example samples
//!
//! Each sample is authored as a stack of blend-mode layers (like in a design
//! tool), flattened into a `.vitral.png`, and rendered over a busy backdrop
//! next to what a plain PNG viewer would show.

use std::f32::consts::PI;
use std::fs;
use std::path::Path;

use vitral::{BlendMode, Layer, RgbaImage, VitralImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    fs::create_dir_all(root.join("renders"))?;

    let backdrop = scene(640, 400);
    fs::write(root.join("backdrop.png"), vitral::write_rgba8(&backdrop)?)?;

    let samples: [(&str, VitralImage); 4] = [
        ("stained-glass", stained_glass()),
        ("glass-badge", glass_badge()),
        ("light-leak", light_leak()),
        ("invert-ring", invert_ring()),
    ];
    for (name, img) in samples {
        let bytes = vitral::encode(&img)?;
        fs::write(root.join(format!("{name}.vitral.png")), &bytes)?;

        // Side by side: plain PNG viewer (left) vs Vitral-aware renderer (right).
        let decoded = vitral::decode(&bytes)?;
        let x = (backdrop.width as i64 - img.width as i64) / 2;
        let y = (backdrop.height as i64 - img.height as i64) / 2;
        let mut legacy = backdrop.clone();
        let fallback = Layer::new(decoded.fallback.clone(), BlendMode::Normal);
        VitralImage::from_layers(img.width, img.height, &[fallback])?
            .composite_onto(&mut legacy, x, y);
        let mut vitral_view = backdrop.clone();
        decoded.image().composite_onto(&mut vitral_view, x, y);
        let pair = side_by_side(&legacy, &vitral_view, 16);
        fs::write(root.join(format!("renders/{name}.png")), vitral::write_rgba8(&pair)?)?;

        let stats = img.stats();
        println!(
            "{name:14} {:>4}x{:<4} {:>7} bytes  vitral-only pixels: {:.1}%",
            img.width,
            img.height,
            bytes.len(),
            100.0 * stats.vitral as f64 / (img.width * img.height) as f64
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- samples ---

/// The namesake: Voronoi panes of tinted glass (multiply) joined by lead
/// (normal), with a soft sheen (screen).
fn stained_glass() -> VitralImage {
    let (w, h) = (300, 360);
    let (cx, top, bottom, half) = (150.0, 140.0, 345.0, 135.0);
    let window = move |x: f32, y: f32| -> f32 {
        // Signed distance to an arched window (positive inside).
        let d_rect = (half - (x - cx).abs()).min(bottom - y);
        if y < top {
            half - ((x - cx).powi(2) + (y - top).powi(2)).sqrt()
        } else {
            d_rect
        }
    };
    let seeds = [
        (150.0, 40.0), (95.0, 85.0), (205.0, 85.0), (150.0, 120.0), (60.0, 160.0),
        (240.0, 160.0), (115.0, 190.0), (185.0, 195.0), (55.0, 250.0), (150.0, 250.0),
        (245.0, 250.0), (95.0, 305.0), (205.0, 305.0), (150.0, 330.0),
    ];
    let palette = [
        [200, 40, 55], [40, 85, 205], [245, 185, 40], [40, 160, 95], [135, 60, 175],
        [40, 170, 195], [235, 110, 40],
    ];
    let voronoi = move |x: f32, y: f32| {
        let mut best = (f32::MAX, f32::MAX, 0usize);
        for (i, (sx, sy)) in seeds.iter().enumerate() {
            let d = ((x - sx).powi(2) + (y - sy).powi(2)).sqrt();
            if d < best.0 {
                best = (d, best.0, i);
            } else if d < best.1 {
                best.1 = d;
            }
        }
        ((best.1 - best.0) / 2.0, best.2)
    };
    let glass = paint(w, h, |x, y| {
        let inside = coverage(window(x, y));
        let (_, cell) = voronoi(x, y);
        rgba(palette[cell % palette.len()], inside * 0.9)
    });
    let sheen = paint(w, h, |x, y| {
        let band = (-(((x - 0.8 * y) - 20.0) / 60.0).powi(2)).exp();
        rgba([255, 255, 255], coverage(window(x, y)) * band * 0.35)
    });
    let lead = paint(w, h, |x, y| {
        let d = window(x, y);
        let (edge, _) = voronoi(x, y);
        let joint = coverage(2.2 - edge) * coverage(d);
        let frame = coverage(d + 4.0) * coverage(5.0 - d);
        rgba([38, 36, 40], joint.max(frame))
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(glass, BlendMode::Multiply),
            Layer::new(sheen, BlendMode::Screen),
            Layer::new(lead, BlendMode::Normal),
        ],
    )
    .unwrap()
}

/// An amber glass card: soft multiply shadow, tinted body, screen
/// highlight and a thin white rim.
fn glass_badge() -> VitralImage {
    let (w, h) = (360, 260);
    let card = |dy: f32| move |x: f32, y: f32| rounded_rect(x, y - dy, 180.0, 120.0, 140.0, 80.0, 36.0);
    let shadow_alpha = blur(&alpha_mask(w, h, |x, y| coverage(card(16.0)(x, y))), w, h, 14.0);
    let shadow = paint(w, h, |x, y| rgba([10, 20, 40], shadow_alpha[idx(w, x, y)] * 0.55));
    let body = paint(w, h, |x, y| rgba([255, 175, 70], coverage(card(0.0)(x, y)) * 0.9));
    let highlight = paint(w, h, |x, y| {
        let fade = ((150.0 - y) / 110.0).clamp(0.0, 1.0).powf(1.5);
        rgba([255, 255, 255], coverage(card(0.0)(x, y) - 3.0) * fade * 0.55)
    });
    let rim = paint(w, h, |x, y| {
        let d = card(0.0)(x, y);
        rgba([255, 255, 255], coverage(d) * coverage(1.5 - d) * 0.8)
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(shadow, BlendMode::Multiply),
            Layer::new(body, BlendMode::Multiply),
            Layer::new(highlight, BlendMode::Screen),
            Layer::new(rim, BlendMode::Normal),
        ],
    )
    .unwrap()
}

/// Warm light blobs in screen mode, like the light leaks sold to video editors.
fn light_leak() -> VitralImage {
    let (w, h) = (420, 280);
    let blob = |cx: f32, cy: f32, r: f32, colour: [u8; 3], strength: f32| {
        paint(w, h, move |x, y| {
            let d2 = ((x - cx).powi(2) + (y - cy).powi(2)) / (r * r);
            rgba(colour, (-d2).exp() * strength)
        })
    };
    let layers = [
        Layer::new(blob(90.0, 70.0, 150.0, [255, 110, 30], 0.95), BlendMode::Screen),
        Layer::new(blob(300.0, 200.0, 120.0, [255, 40, 120], 0.8), BlendMode::Screen),
        Layer::new(blob(200.0, 120.0, 60.0, [255, 230, 150], 0.9), BlendMode::Screen),
    ];
    VitralImage::from_layers(w, h, &layers).unwrap()
}

/// A ring and a star that invert whatever is behind them.
fn invert_ring() -> VitralImage {
    let (w, h) = (260, 260);
    let (cx, cy) = (130.0, 130.0);
    let shape = paint(w, h, |x, y| {
        let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        let ring = coverage(120.0 - r) * coverage(r - 96.0);
        let star = coverage(star_sdf(x - cx, y - cy, 80.0, 34.0));
        rgba([255, 255, 255], ring.max(star))
    });
    VitralImage::from_layers(w, h, &[Layer::new(shape, BlendMode::Difference)]).unwrap()
}

/// A busy backdrop: sunset with a sun, a hue ramp, and black/white stripes.
fn scene(w: u32, h: u32) -> RgbaImage {
    paint(w, h, |x, y| {
        let u = x / w as f32;
        let v = y / h as f32;
        let c = if u < 0.36 {
            let sky = lerp3([30.0, 40.0, 110.0], [250.0, 140.0, 70.0], v);
            let sun = coverage(40.0 - ((x - 120.0).powi(2) + (y - 230.0).powi(2)).sqrt());
            let hill = coverage(y - (300.0 + 25.0 * (x / 30.0).sin()));
            let lit = lerp3(sky, [255.0, 235.0, 170.0], sun);
            lerp3(lit, [25.0, 30.0, 45.0], hill)
        } else if u < 0.68 {
            hue(v + 0.15 * (x / 40.0).sin())
        } else if v < 0.5 {
            let stripe = ((x + y) / 18.0).floor() as i32 % 2 == 0;
            if stripe { [245.0; 3] } else { [20.0; 3] }
        } else {
            let check = ((x / 24.0).floor() + (y / 24.0).floor()) as i32 % 2 == 0;
            if check { [235.0, 225.0, 205.0] } else { [70.0, 110.0, 90.0] }
        };
        [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0]
    })
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
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, alpha]
}

/// Antialiased coverage from a signed distance (positive inside).
fn coverage(d: f32) -> f32 {
    (d + 0.5).clamp(0.0, 1.0)
}

fn rounded_rect(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (x - cx).abs() - (hw - r);
    let qy = (y - cy).abs() - (hh - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    r - outside - qx.max(qy).min(0.0)
}

/// Signed distance to a five-pointed star (positive inside), approximated by
/// a polar profile.
fn star_sdf(x: f32, y: f32, outer: f32, inner: f32) -> f32 {
    let r = (x * x + y * y).sqrt();
    let a = y.atan2(x) + PI / 2.0;
    let sector = (a.rem_euclid(2.0 * PI / 5.0) / (2.0 * PI / 5.0) - 0.5).abs() * 2.0;
    let edge = inner + (outer - inner) * (1.0 - sector);
    (edge - r) * 0.8
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

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn hue(t: f32) -> [f32; 3] {
    let t = t.rem_euclid(1.0) * 6.0;
    let f = |n: f32| {
        let k = (n + t) % 6.0;
        255.0 * (1.0 - (k.min(4.0 - k).clamp(0.0, 1.0)))
    };
    [f(5.0), f(3.0), f(1.0)]
}

fn side_by_side(a: &RgbaImage, b: &RgbaImage, gap: u32) -> RgbaImage {
    let w = a.width + gap + b.width;
    let h = a.height.max(b.height);
    let mut out = RgbaImage::filled(w, h, [255, 255, 255, 0]);
    for (img, x0) in [(a, 0), (b, a.width + gap)] {
        for y in 0..img.height {
            let src = (y * img.width * 4) as usize;
            let dst = ((y * w + x0) * 4) as usize;
            out.data[dst..dst + (img.width * 4) as usize]
                .copy_from_slice(&img.data[src..src + (img.width * 4) as usize]);
        }
    }
    out
}
