//! Real exports from Figma, each artwork exported over black, over white and
//! over a transparent background (184 x 184 px). The Vitral file built from
//! them must give back each export exactly and follow the blend-mode formulas
//! over other backdrops.
//!
//! - `multiply`: an orange rectangle and a green circle, both in multiply.
//! - `mixed`: an orange rectangle in screen, a green circle in multiply and a
//!   cyan triangle in normal mode at 50% opacity.

use std::fs;

use vitral::{colour_space, decode, encode, read_rgba8, RgbaImage, VitralImage, VitralStatus};

const SETS: [&str; 2] = ["multiply", "mixed"];

fn export(set: &str, over: &str) -> (Vec<u8>, RgbaImage) {
    let path = format!(
        "{}/tests/data/figma/{set}-over-{over}.png",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = fs::read(path).unwrap();
    let image = read_rgba8(&bytes).unwrap();
    (bytes, image)
}

/// Premultiplied colour, so fully transparent pixels compare equal whatever
/// colour they store.
fn premultiplied(img: &RgbaImage) -> Vec<[u8; 4]> {
    img.data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&[r, g, b, a]| {
            let m = |c: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
            [m(r), m(g), m(b), a]
        })
        .collect()
}

fn vitral_image(set: &str) -> VitralImage {
    let img = VitralImage::from_renders(&export(set, "black").1, &export(set, "white").1)
        .unwrap()
        .with_native(export(set, "transparent").1)
        .unwrap();
    let decoded = decode(&encode(&img).unwrap()).unwrap();
    assert_eq!(decoded.status, VitralStatus::Ok);
    decoded.vitral.unwrap()
}

fn over_grey(img: &VitralImage, grey: u8) -> RgbaImage {
    let mut canvas = RgbaImage::filled(img.width, img.height, [grey, grey, grey, 255]);
    img.composite_onto(&mut canvas, 0, 0);
    canvas
}

fn rgb(img: &RgbaImage, i: usize) -> [u8; 3] {
    [img.data[i * 4], img.data[i * 4 + 1], img.data[i * 4 + 2]]
}

#[test]
fn figma_exports_are_srgb() {
    for set in SETS {
        for over in ["black", "white", "transparent"] {
            assert!(colour_space(&export(set, over).0).is_srgb(), "{set} {over}");
        }
    }
}

#[test]
fn figma_exports_are_reproduced_exactly() {
    for set in SETS {
        let img = vitral_image(set);
        let (w, h) = (img.width, img.height);
        for (over, backdrop) in [("black", [0, 0, 0, 255]), ("white", [255, 255, 255, 255])] {
            let mut canvas = RgbaImage::filled(w, h, backdrop);
            img.composite_onto(&mut canvas, 0, 0);
            assert_eq!(canvas, export(set, over).1, "{set} over {over}");
        }
        let mut canvas = RgbaImage::filled(w, h, [0, 0, 0, 0]);
        img.composite_onto(&mut canvas, 0, 0);
        assert_eq!(
            premultiplied(&canvas),
            premultiplied(&export(set, "transparent").1),
            "{set} over transparent"
        );
    }
}

/// Over any grey, two multiply layers give `backdrop * orange * green`.
#[test]
fn figma_multiply_holds_over_other_backdrops() {
    let img = vitral_image("multiply");
    let white = export("multiply", "white").1;
    for grey in [37u8, 128, 201] {
        let canvas = over_grey(&img, grey);
        for i in 0..(img.width * img.height) as usize {
            for (got, over_white) in rgb(&canvas, i).into_iter().zip(rgb(&white, i)) {
                let want = grey as f64 * over_white as f64 / 255.0;
                assert!((got as f64 - want).abs() <= 1.0, "grey {grey}");
            }
        }
    }
}

/// Each region of the mixed artwork follows its own blend-mode formula.
#[test]
fn figma_mixed_modes_hold_over_other_backdrops() {
    const ORANGE: [f64; 3] = [248.0, 193.0, 91.0];
    const GREEN: [f64; 3] = [23.0, 100.0, 18.0];
    const CYAN: [f64; 3] = [34.0, 201.0, 243.0];
    let screen = |b: f64, s: f64| b + s - b * s / 255.0;
    let img = vitral_image("mixed");
    let (black, white) = (export("mixed", "black").1, export("mixed", "white").1);
    let mut checked = [0usize; 4];
    for grey in [40u8, 128, 210] {
        let b = grey as f64;
        let canvas = over_grey(&img, grey);
        for i in 0..(img.width * img.height) as usize {
            // Identify fully covered pixels of each region by their two exports.
            let region = match (rgb(&black, i), rgb(&white, i)) {
                ([248, 193, 91], [255, 255, 255]) => 0, // rectangle: screen
                ([0, 0, 0], [23, 100, 18]) => 1,        // circle: multiply
                ([17, 101, 122], [145, 229, 249]) => 2, // triangle: normal 50%
                ([22, 76, 6], [23, 100, 18]) => 3,      // circle over rectangle
                _ => continue,
            };
            let want: [f64; 3] = std::array::from_fn(|c| match region {
                0 => screen(b, ORANGE[c]),
                1 => b * GREEN[c] / 255.0,
                2 => 0.5 * CYAN[c] + 0.5 * b,
                _ => screen(b, ORANGE[c]) * GREEN[c] / 255.0,
            });
            for (got, want) in rgb(&canvas, i).into_iter().zip(want) {
                assert!(
                    (got as f64 - want).abs() <= 1.0,
                    "region {region}, grey {grey}"
                );
            }
            checked[region] += 1;
        }
    }
    assert!(checked.iter().all(|&n| n > 100), "{checked:?}");
}
