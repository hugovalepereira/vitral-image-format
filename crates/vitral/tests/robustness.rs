//! Robustness tests: randomly damaged files must never make the decoder panic,
//! and lossless re-encodings of the main image (palette, greyscale) must keep
//! the Vitral data valid.

use std::io::Cursor;

use vitral::{decode, encode, BlendMode, Layer, RgbaImage, VitralImage, VitralStatus, CHUNK_TYPE};

/// Small deterministic generator (xorshift64*), so failures are reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Mutations per fuzz test; raise with VITRAL_FUZZ_ITERATIONS for a deep run.
fn iterations(default: usize) -> usize {
    std::env::var("VITRAL_FUZZ_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// A 16x16 image mixing normal, multiply and screen pixels.
fn sample() -> VitralImage {
    let paint = |f: &dyn Fn(u32, u32) -> [u8; 4]| {
        let mut data = Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                data.extend_from_slice(&f(x, y));
            }
        }
        RgbaImage::new(16, 16, data).unwrap()
    };
    let layers = [
        Layer::new(
            paint(&|x, _| [200, 40, 60, (x * 16) as u8]),
            BlendMode::Multiply,
        ),
        Layer::new(
            paint(&|_, y| [250, 230, 120, (y * 12) as u8]),
            BlendMode::Screen,
        ),
        Layer::new(
            paint(&|x, y| {
                if (x + y) % 5 == 0 {
                    [20, 20, 30, 255]
                } else {
                    [0; 4]
                }
            }),
            BlendMode::Normal,
        ),
    ];
    VitralImage::from_layers(16, 16, &layers).unwrap()
}

/// Splits a Vitral file into (main PNG without the vITr chunk, payload).
fn split(file: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut pos = 8;
    let mut main = file[..8].to_vec();
    let mut payload = Vec::new();
    while pos + 12 <= file.len() {
        let len = u32::from_be_bytes(file[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &file[pos + 4..pos + 8];
        let end = pos + 12 + len;
        if kind == CHUNK_TYPE {
            payload = file[pos + 8..pos + 8 + len].to_vec();
        } else {
            main.extend_from_slice(&file[pos..end]);
        }
        pos = end;
    }
    (main, payload)
}

/// Inserts a vITr chunk with a correct CRC just before IEND.
fn with_chunk(main: &[u8], payload: &[u8]) -> Vec<u8> {
    let iend = main.len() - 12;
    let mut out = main[..iend].to_vec();
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&CHUNK_TYPE);
    crc.update(payload);
    out.extend_from_slice(&CHUNK_TYPE);
    out.extend_from_slice(payload);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
    out.extend_from_slice(&main[iend..]);
    out
}

fn mutate(rng: &mut Rng, bytes: &mut Vec<u8>) {
    for _ in 0..1 + rng.below(4) {
        if bytes.is_empty() {
            return;
        }
        let at = rng.below(bytes.len());
        match rng.below(5) {
            0 => bytes[at] ^= 1 << rng.below(8),
            1 => bytes[at] = rng.next() as u8,
            2 => bytes.truncate(at),
            3 => {
                let len = rng.below(16).min(bytes.len() - at);
                let slice = bytes[at..at + len].to_vec();
                bytes.splice(at..at, slice);
            }
            _ => {
                bytes.remove(at);
            }
        }
    }
}

/// Mutates the chunk payload but keeps the chunk CRC valid, so the damage
/// reaches the header, flags and embedded residual parser.
#[test]
fn damaged_payloads_never_panic() {
    let file = encode(&sample()).unwrap();
    let (main, payload) = split(&file);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut statuses = std::collections::BTreeMap::new();
    for _ in 0..iterations(4000) {
        let mut damaged = payload.clone();
        mutate(&mut rng, &mut damaged);
        let decoded = decode(&with_chunk(&main, &damaged)).expect("main image is intact");
        if let Some(img) = &decoded.vitral {
            assert_eq!((img.width, img.height), (16, 16));
            assert_eq!(img.black.len(), 16 * 16 * 3);
        }
        let key = format!("{:?}", decoded.status);
        *statuses
            .entry(key.split('(').next().unwrap().to_string())
            .or_insert(0) += 1;
    }
    // The mutations must actually exercise the error paths.
    assert!(statuses.contains_key("Corrupt"), "{statuses:?}");
    assert!(statuses.contains_key("Unsupported"), "{statuses:?}");
}

/// Mutates anywhere in the file: decoding may fail, but must never panic.
#[test]
fn damaged_files_never_panic() {
    let file = encode(&sample()).unwrap();
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    for _ in 0..iterations(3000) {
        let mut damaged = file.clone();
        mutate(&mut rng, &mut damaged);
        let _ = decode(&damaged);
    }
}

fn png_with(
    width: u32,
    height: u32,
    colour: png::ColorType,
    setup: impl FnOnce(&mut png::Encoder<&mut Vec<u8>>),
    data: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(colour);
        enc.set_depth(png::BitDepth::Eight);
        setup(&mut enc);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(data).unwrap();
    }
    out
}

/// A lossless optimiser may store the main image as a palette or as greyscale.
/// The pixels decode to the same RGBA, so the Vitral data must stay valid.
#[test]
fn palette_and_greyscale_main_images_keep_the_vitral_data() {
    // Few colours, all grey: representable both as a palette and as grey+alpha.
    let shades = [
        [0, 0, 0, 0],
        [0, 0, 0, 128],
        [60, 60, 60, 255],
        [200, 200, 200, 90],
    ];
    let mut data = Vec::new();
    for i in 0..64 {
        data.extend_from_slice(&shades[i % 4]);
    }
    let layer = Layer::new(
        RgbaImage::new(8, 8, data.clone()).unwrap(),
        BlendMode::Multiply,
    );
    let img = VitralImage::from_layers(8, 8, &[layer]).unwrap();
    let (_, payload) = split(&encode(&img).unwrap());

    let palette: Vec<u8> = shades.iter().flat_map(|s| [s[0], s[1], s[2]]).collect();
    let trns: Vec<u8> = shades.iter().map(|s| s[3]).collect();
    let indices: Vec<u8> = (0..64).map(|i| (i % 4) as u8).collect();
    let indexed = png_with(
        8,
        8,
        png::ColorType::Indexed,
        |e| {
            e.set_palette(palette.clone());
            e.set_trns(trns.clone());
        },
        &indices,
    );
    let grey: Vec<u8> = data.chunks(4).flat_map(|p| [p[0], p[3]]).collect();
    let grey_alpha = png_with(8, 8, png::ColorType::GrayscaleAlpha, |_| {}, &grey);

    for (name, main) in [("palette", indexed), ("grey+alpha", grey_alpha)] {
        let decoded = decode(&with_chunk(&main, &payload)).unwrap();
        assert_eq!(decoded.status, VitralStatus::Ok, "{name}");
        let got = decoded.vitral.unwrap();
        assert_eq!(
            (got.black, got.white),
            (img.black.clone(), img.white.clone()),
            "{name}"
        );
    }
}

/// The main image may be stored interlaced (Adam7); `tests/data` holds one made
/// with ImageMagick from the stained glass sample.
#[test]
fn interlaced_main_image_keeps_the_vitral_data() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/interlaced.vitral.png"
    );
    let bytes = std::fs::read(path).unwrap();
    let mut decoder = png::Decoder::new(Cursor::new(&bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    assert!(
        decoder.read_info().unwrap().info().interlaced,
        "test file must be interlaced"
    );
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.status, VitralStatus::Ok);
}
