//! Real exports from Figma: two shapes in multiply mode (an orange rectangle
//! and a green circle that overlaps it), exported over black, over white and
//! over a transparent background. The Vitral file built from them must give
//! back each export exactly.

use std::fs;

use vitral::{colour_space, decode, encode, read_rgba8, RgbaImage, VitralImage, VitralStatus};

fn export(name: &str) -> (Vec<u8>, RgbaImage) {
    let path = format!(
        "{}/tests/data/figma/multiply-over-{name}.png",
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

fn vitral_file() -> Vec<u8> {
    let (_, black) = export("black");
    let (_, white) = export("white");
    let (_, transparent) = export("transparent");
    let img = VitralImage::from_renders(&black, &white)
        .unwrap()
        .with_native(transparent)
        .unwrap();
    encode(&img).unwrap()
}

#[test]
fn figma_exports_are_srgb() {
    for name in ["black", "white", "transparent"] {
        assert!(colour_space(&export(name).0).is_srgb(), "{name}");
    }
}

#[test]
fn figma_exports_are_reproduced_exactly() {
    let decoded = decode(&vitral_file()).unwrap();
    assert_eq!(decoded.status, VitralStatus::Ok);
    let img = decoded.vitral.unwrap();
    let (w, h) = (img.width, img.height);

    for (name, backdrop) in [("black", [0, 0, 0, 255]), ("white", [255, 255, 255, 255])] {
        let mut canvas = RgbaImage::filled(w, h, backdrop);
        img.composite_onto(&mut canvas, 0, 0);
        assert_eq!(canvas, export(name).1, "over {name}");
    }

    let mut canvas = RgbaImage::filled(w, h, [0, 0, 0, 0]);
    img.composite_onto(&mut canvas, 0, 0);
    assert_eq!(
        premultiplied(&canvas),
        premultiplied(&export("transparent").1)
    );
}

/// Over any grey, two multiply layers give `backdrop * orange * green`.
#[test]
fn figma_multiply_holds_over_other_backdrops() {
    let img = decode(&vitral_file()).unwrap().vitral.unwrap();
    let (_, white) = export("white");
    for grey in [37u8, 128, 201] {
        let mut canvas = RgbaImage::filled(img.width, img.height, [grey, grey, grey, 255]);
        img.composite_onto(&mut canvas, 0, 0);
        let pixels = canvas.data.as_chunks::<4>().0.iter();
        for (got, over_white) in pixels.zip(white.data.as_chunks::<4>().0) {
            for c in 0..3 {
                let want = (grey as f64 * over_white[c] as f64 / 255.0).round() as i32;
                assert!((got[c] as i32 - want).abs() <= 1, "grey {grey}");
            }
        }
    }
}
