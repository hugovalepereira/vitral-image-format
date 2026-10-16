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
//! encoder section starts from them. Its plain PNG, shown next to the Vitral
//! at the top of the page, is only the intention: the same shapes as flat,
//! half-transparent colour. The backdrops come from tools/demo_backdrops.py.

use std::f32::consts::PI;
use std::fs;
use std::path::Path;

use vitral::{BlendMode, Layer, RgbaImage, VitralImage};

const WHITE: [u8; 3] = [255, 255, 255];
const INK: [u8; 3] = [20, 20, 22];
const MAGENTA: [u8; 3] = [236, 40, 132];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/samples");
    fs::create_dir_all(root.join("sources"))?;

    let images: [(&str, VitralImage); 11] = [
        ("ring", ring()),
        ("overprint", overprint()),
        ("highlighter", highlighter()),
        ("sunglasses", sunglasses()),
        ("film", film()),
        ("blinds", blinds()),
        ("tape", tape()),
        ("halftone", halftone()),
        ("neon", neon()),
        ("postmark", postmark()),
        ("stencil", stencil()),
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

/// The object dropped on the page at the top: a disc of horizontal bands that
/// invert what is behind them, crossed by a bar of magenta ink in multiply on
/// top,
/// with the soft shadow the two cast, also in multiply. Its plain PNG shows
/// only the intention: the same shapes as flat, half-transparent colour.
fn ring() -> VitralImage {
    let (w, h) = (640, 640);
    let disc = |x: f32, y: f32| circle(x, y, 320.0, 316.0, 214.0);
    let bar = |x: f32, y: f32| rect(x, y, 266.0, 52.0, 374.0, 580.0);
    let both = move |x: f32, y: f32| disc(x, y).max(bar(x, y));
    let shadow = paint(w, h, move |x, y| {
        let cast = smooth(both(x - 16.0, y - 24.0), 30.0);
        rgba([70, 72, 86], cast * 0.42 * (1.0 - coverage(both(x, y))))
    });
    let ink = paint(w, h, move |x, y| rgba(MAGENTA, coverage(bar(x, y))));
    let bands = paint(w, h, move |x, y| {
        // Bands 28 px thick every 56 px.
        let band = coverage(14.0 - ((y - 116.0).rem_euclid(56.0) - 28.0).abs());
        rgba(WHITE, band * coverage(disc(x, y)))
    });
    let rim = paint(w, h, move |x, y| {
        let d = disc(x, y);
        rgba(INK, coverage(d + 1.5) * coverage(1.5 - d))
    });
    let native = flatten(&[(&shadow, 1.0), (&bands, 0.6), (&ink, 0.55), (&rim, 1.0)]);
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(shadow, BlendMode::Multiply),
            Layer::new(bands, BlendMode::Difference),
            Layer::new(ink, BlendMode::Multiply),
            Layer::new(rim, BlendMode::Normal),
        ],
    )
    .unwrap()
    .with_native(native)
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

/// Sunglasses drawn like a screen print: a heavy black frame, lenses of
/// bottle-green glass in multiply and a glint of light in screen.
fn sunglasses() -> VitralImage {
    let (w, h) = (720, 300);
    let lenses = [180.0, 540.0];
    // A lens is wider at the top and rounder at the bottom.
    let lens = move |x: f32, y: f32| {
        lenses
            .iter()
            .map(|&cx| {
                let (u, v) = (x - cx, y - 150.0);
                let hw = 130.0 - 24.0 * ((v + 86.0) / 172.0).clamp(0.0, 1.0);
                let r = if v < 0.0 { 18.0 } else { 70.0 };
                let qx = u.abs() - hw + r;
                let qy = v.abs() - 86.0 + r;
                (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
            })
            .fold(f32::MAX, f32::min)
    };
    let tint = paint(w, h, move |x, y| {
        let t = ((y - 64.0) / 172.0).clamp(0.0, 1.0);
        let c = [40.0, 66.0, 56.0].map(|v: f32| (v + 70.0 * t) / 255.0);
        [c[0], c[1], c[2], coverage(-lens(x, y))]
    });
    let glint = paint(w, h, move |x, y| {
        let u = x + 0.6 * y;
        let a = lenses
            .iter()
            .map(|&cx| {
                let o = cx + 50.0;
                coverage(16.0 - (u - o).abs()).max(coverage(5.0 - (u - o - 36.0).abs()))
            })
            .fold(0.0, f32::max);
        rgba(WHITE, a * 0.7 * coverage(-lens(x, y) - 10.0))
    });
    let frame = paint(w, h, move |x, y| {
        let d = lens(x, y);
        let rim = coverage(18.0 - d);
        let brow = coverage(-boxd(x, y, 360.0, 74.0, 336.0, 18.0, 0.0, 8.0));
        let hinges = coverage(-boxd(x, y, 22.0, 84.0, 22.0, 12.0, 0.0, 3.0))
            .max(coverage(-boxd(x, y, 698.0, 84.0, 22.0, 12.0, 0.0, 3.0)));
        rgba(INK, (rim.max(brow).max(hinges)) * (1.0 - coverage(-d)))
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

/// A strip of colour negative, torn off at one end: frames that invert what
/// is behind them under the orange base in multiply, sprocket holes cut out,
/// scratches and dust in screen, and the frame picked out in grease pencil.
fn film() -> VitralImage {
    let (w, h) = (760, 420);
    let deg = -5.0;
    let strip = move |x: f32, y: f32| {
        let (u, v) = local(x, y, 380.0, 210.0, deg);
        let torn = -352.0 + 22.0 * noise(0.0, v / 9.0) + 8.0 * noise(3.0, v / 2.5);
        (u - torn).min(360.0 - u).min(132.0 - v.abs())
    };
    let hole = move |x: f32, y: f32| {
        let (u, v) = local(x, y, 380.0, 210.0, deg);
        let cu = (u / 40.0).floor() * 40.0 + 20.0;
        coverage(-boxd(u, v.abs(), cu, 112.0, 9.0, 12.0, 0.0, 3.0))
    };
    let frame = move |x: f32, y: f32| {
        let (u, v) = local(x, y, 380.0, 210.0, deg);
        let cu = ((u + 110.0) / 220.0).floor() * 220.0;
        coverage(-boxd(u, v, cu, 0.0, 100.0, 88.0, 0.0, 4.0))
    };
    let invert = paint(w, h, move |x, y| {
        rgba(WHITE, frame(x, y) * coverage(strip(x, y)))
    });
    let base = paint(w, h, move |x, y| {
        let density = 0.86 + 0.14 * noise(x / 90.0, y / 90.0);
        rgba(
            [214, 124, 64],
            coverage(strip(x, y)) * (1.0 - hole(x, y)) * density,
        )
    });
    let wear = paint(w, h, move |x, y| {
        let (u, v) = local(x, y, 380.0, 210.0, deg);
        let scratch = [(-38.0, 0.0), (27.0, 1.0), (61.0, 2.0)]
            .iter()
            .map(|&(at, seed)| {
                let wobble = at + 3.0 * noise(seed, u / 60.0);
                coverage(0.8 - (v - wobble).abs())
                    * smooth(noise(seed + 5.0, u / 120.0) - 0.35, 0.2)
            })
            .fold(0.0, f32::max);
        let dust = coverage(1.6 - hash((x / 3.0) as i32, (y / 3.0) as i32 + 999) * 900.0);
        rgba(
            WHITE,
            scratch.max(dust) * coverage(strip(x, y)) * (1.0 - hole(x, y)) * 0.8,
        )
    });
    let pencil = paint(w, h, move |x, y| {
        let (u, v) = local(x, y, 380.0, 210.0, deg);
        // A loop around the middle frame, drawn fast by hand: lopsided, and
        // the stroke runs on past where it started.
        let (cu, cv) = (u - 6.0, v + 4.0);
        let a = cv.atan2(cu);
        let r = ((cu / 124.0).powi(2) + (cv / 104.0).powi(2)).sqrt();
        let target = 1.0 + 0.05 * (a * 2.0 + 0.7).sin() + 0.04 * (noise(7.0, a * 2.5) - 0.5);
        let width = 6.0 + 4.0 * noise(8.0, a * 4.0);
        let first = coverage(width - (r - target).abs() * 112.0);
        let overrun = (a + 2.2).clamp(0.0, 1.4) * (1.4 - (a + 2.2).clamp(0.0, 1.4)) * 0.25;
        let second = coverage(width * 0.8 - (r - target - overrun).abs() * 112.0)
            * if (-2.2..-0.8).contains(&a) { 1.0 } else { 0.0 };
        let grain = 0.65 + 0.35 * noise(x / 2.0, y / 2.0);
        rgba([214, 32, 44], first.max(second) * grain)
    });
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(invert, BlendMode::Difference),
            Layer::new(base, BlendMode::Multiply),
            Layer::new(wear, BlendMode::Screen),
            Layer::new(pencil, BlendMode::Normal),
        ],
    )
    .unwrap()
}

/// Afternoon sun through a window with half-open blinds, cast on the wall:
/// a slanted patch of warm light in screen, cut by the slats and the window
/// bars, sharp near the sill and softer as it goes up.
fn blinds() -> VitralImage {
    let (w, h) = (760, 520);
    let light = paint(w, h, move |x, y| {
        // Wall coordinates to window coordinates (s across, t up), in pixels.
        let t = 470.0 - y;
        let s = x - 70.0 - 0.5 * t;
        let soft = 2.0 + 0.08 * t;
        let pane = (s.min(370.0 - s)).min(t.min(370.0 - t));
        let bars = (s - 185.0).abs().min((t - 185.0).abs()) - 7.0;
        let slat = (t.rem_euclid(28.0) - 14.0).abs() - 7.5;
        let lit = smooth(pane, soft) * smooth(bars, soft) * smooth(-slat, soft);
        let bloom = (-(pane.min(0.0) / 60.0).powi(2)).exp() * 0.12;
        let inside = smooth(rect(x, y, 0.0, 0.0, w as f32, h as f32) - 10.0, 40.0);
        rgba([255, 204, 150], (lit * 0.85).max(bloom * inside))
    });
    VitralImage::from_layers(w, h, &[Layer::new(light, BlendMode::Screen)]).unwrap()
}

/// Two strips of tape torn off the roll, a cream masking tape and a blue
/// painter's tape, in multiply: the paper is a little uneven and the ends
/// are ragged.
fn tape() -> VitralImage {
    let (w, h) = (680, 480);
    let strip = |cx: f32, cy: f32, deg: f32, ink: [u8; 3], seed: f32| {
        paint(w, h, move |x, y| {
            let (u, v) = local(x, y, cx, cy, deg);
            let tear = |side: f32| {
                270.0 - 9.0 * noise(seed + side, v / 6.0) - 5.0 * noise(seed + side + 2.0, v / 1.8)
            };
            let end = if u > 0.0 { tear(0.0) } else { tear(5.0) };
            let d = (end - u.abs()).min(42.0 - v.abs());
            let fibre =
                0.84 + 0.1 * noise(seed + u / 50.0, v / 2.5) + 0.06 * noise(u / 4.0, v / 4.0);
            let edge = 1.0 - 0.1 * coverage(v.abs() - 38.0);
            let c = ink.map(|c| (c as f32 * edge) as u8);
            rgba(c, coverage(d) * fibre)
        })
    };
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(
                strip(330.0, 210.0, -14.0, [232, 214, 170], 0.0),
                BlendMode::Multiply,
            ),
            Layer::new(
                strip(350.0, 290.0, 19.0, [128, 176, 222], 10.0),
                BlendMode::Multiply,
            ),
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

/// A neon sign that says OPEN in a frame: tubes in screen with a white-hot
/// core and a soft glow that fades out before the edge of the image.
fn neon() -> VitralImage {
    let (w, h) = (800, 460);
    let letters = |x: f32, y: f32| {
        let (x, y) = (x - 20.0, y - 20.0);
        let o = {
            let (u, v) = ((x - 180.0) / 58.0, (y - 210.0) / 84.0);
            ((u * u + v * v).sqrt() - 1.0).abs() * 62.0
        };
        let p = segment(x, y, 280.0, 126.0, 280.0, 294.0)
            .min(segment(x, y, 280.0, 126.0, 312.0, 126.0))
            .min(segment(x, y, 280.0, 214.0, 312.0, 214.0))
            .min(if x > 312.0 {
                (((x - 312.0).powi(2) + (y - 170.0).powi(2)).sqrt() - 44.0).abs()
            } else {
                f32::MAX
            });
        let e = segment(x, y, 400.0, 126.0, 400.0, 294.0)
            .min(segment(x, y, 400.0, 126.0, 466.0, 126.0))
            .min(segment(x, y, 400.0, 210.0, 452.0, 210.0))
            .min(segment(x, y, 400.0, 294.0, 466.0, 294.0));
        let n = segment(x, y, 520.0, 294.0, 520.0, 126.0)
            .min(segment(x, y, 520.0, 126.0, 600.0, 294.0))
            .min(segment(x, y, 600.0, 294.0, 600.0, 126.0));
        o.min(p).min(e).min(n)
    };
    let frame = |x: f32, y: f32| boxd(x, y, 400.0, 230.0, 330.0, 164.0, 0.0, 44.0).abs();
    let fade = move |x: f32, y: f32| smooth(rect(x, y, 0.0, 0.0, w as f32, h as f32), 50.0);
    let tube = move |d: f32, colour: [u8; 3], x: f32, y: f32| {
        let core = coverage(4.5 - d);
        let glow =
            ((-(d / 22.0).powi(2)).exp() * 0.7 + (-(d / 56.0).powi(2)).exp() * 0.16) * fade(x, y);
        let hot = core * 0.7;
        let c = colour.map(|v| v as f32 / 255.0 * (1.0 - hot) + hot);
        [c[0], c[1], c[2], core.max(glow)]
    };
    let pink = paint(w, h, move |x, y| tube(letters(x, y), [255, 64, 150], x, y));
    let blue = paint(w, h, move |x, y| tube(frame(x, y), [70, 200, 255], x, y));
    VitralImage::from_layers(
        w,
        h,
        &[
            Layer::new(blue, BlendMode::Screen),
            Layer::new(pink, BlendMode::Screen),
        ],
    )
    .unwrap()
}

/// A rubber-stamp postmark, inked unevenly, in multiply: a solid date disc
/// with the date band knocked out, a heavy ring and thick wavy cancellation
/// bars.
fn postmark() -> VitralImage {
    let (w, h) = (640, 420);
    let ink = paint(w, h, |x, y| {
        let r = ((x - 190.0).powi(2) + (y - 210.0).powi(2)).sqrt();
        let ring = coverage(9.0 - (r - 160.0).abs());
        let band = coverage(17.0 - (y - 210.0).abs()) * coverage(126.0 - r);
        let date = [-62.0, -10.0, 42.0]
            .iter()
            .map(|&dx| coverage(-boxd(x, y, 190.0 + dx, 210.0, 18.0, 7.0, 0.0, 2.0)))
            .fold(0.0, f32::max);
        let disc = coverage(122.0 - r) * (1.0 - band) + date;
        let waves = (0..6)
            .map(|i| {
                let base = 100.0 + 44.0 * i as f32;
                let wave = base + 11.0 * ((x - 330.0) / 70.0 * 2.0 * PI).sin();
                coverage(8.0 - (y - wave).abs()) * coverage(x - 336.0) * coverage(620.0 - x)
            })
            .fold(0.0, f32::max);
        // Uneven inking: grain everywhere and specks the stamp missed.
        let grain = 0.7 + 0.3 * noise(x / 5.0, y / 5.0);
        let missed = if hash((x / 4.0) as i32, (y / 4.0) as i32) < 0.08 {
            0.3
        } else {
            1.0
        };
        rgba([30, 44, 128], disc.max(ring).max(waves) * grain * missed)
    });
    VitralImage::from_layers(w, h, &[Layer::new(ink, BlendMode::Multiply)]).unwrap()
}

/// An arrow sprayed through a stencil in red, in multiply: the bridges that
/// hold the stencil together leave gaps, and the overspray settles around the
/// edges as a fine mist of dots.
fn stencil() -> VitralImage {
    let (w, h) = (640, 420);
    let paint_layer = paint(w, h, |x, y| {
        let shaft = rect(x, y, 70.0, 168.0, 352.0, 252.0);
        let (u, v) = (x - 352.0, (y - 210.0).abs());
        let head = (u + 4.0).min((130.0 * (1.0 - u / 212.0) - v) * 0.85);
        let arrow = shaft.max(head);
        let bridges = (x - 214.0).abs().min((x - 356.0).abs()) - 6.0;
        let shape = smooth(arrow, 3.0) * smooth(bridges, 2.0);
        let mist = (-(arrow.min(0.0) / 16.0).powi(2)).exp();
        let speck = if hash(x as i32, y as i32) < mist * 0.45 {
            0.8
        } else {
            0.0
        };
        let grain = 0.85 + 0.15 * noise(x / 3.0, y / 3.0);
        rgba([226, 36, 46], (shape * grain).max(speck * (1.0 - shape)))
    });
    VitralImage::from_layers(w, h, &[Layer::new(paint_layer, BlendMode::Multiply)]).unwrap()
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

/// Stacks layers with plain alpha compositing, each faded by its opacity: what
/// a PNG export shows when the blend modes are lost.
fn flatten(layers: &[(&RgbaImage, f32)]) -> RgbaImage {
    let (w, h) = (layers[0].0.width, layers[0].0.height);
    let mut out = vec![0.0f32; (w * h * 4) as usize];
    for (layer, opacity) in layers {
        for (o, p) in out.chunks_mut(4).zip(layer.data.chunks(4)) {
            let a = p[3] as f32 / 255.0 * opacity;
            let keep = o[3] * (1.0 - a);
            let alpha = a + keep;
            for c in 0..3 {
                let v = p[c] as f32 / 255.0;
                o[c] = if alpha > 0.0 {
                    (v * a + o[c] * keep) / alpha
                } else {
                    0.0
                };
            }
            o[3] = alpha;
        }
    }
    let data = out.iter().map(|v| (v * 255.0 + 0.5) as u8).collect();
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

/// Distance to the segment from (ax, ay) to (bx, by).
fn segment(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (px, py, dx, dy) = (x - ax, y - ay, bx - ax, by - ay);
    let t = ((px * dx + py * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - dx * t).powi(2) + (py - dy * t).powi(2)).sqrt()
}

/// A repeatable pseudo-random value in `[0, 1)` for an integer lattice point.
fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    (h ^ (h >> 16)) as f32 / 4_294_967_296.0
}

/// Smooth value noise in `[0, 1]`, varying over one unit.
fn noise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (ix as i32, iy as i32);
    let top = hash(ix, iy) + (hash(ix + 1, iy) - hash(ix, iy)) * sx;
    let bottom = hash(ix, iy + 1) + (hash(ix + 1, iy + 1) - hash(ix, iy + 1)) * sx;
    top + (bottom - top) * sy
}
