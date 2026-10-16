//! The conformance suite in `conformance/` (SPEC §11): Vitral files with the
//! status, K, W and renders a conforming decoder and renderer must produce.
//!
//! The committed files are the suite. This test checks that the reference
//! implementation still agrees with every one of them, and that the manifest
//! matches the list of cases below.
//!
//! To regenerate the files after a deliberate change to the format:
//!
//! ```text
//! VITRAL_WRITE_CONFORMANCE=1 cargo test -p vitral --test conformance
//! ```

use std::path::PathBuf;

use vitral::{
    decode, encode, read_rgba8, write_rgb8, write_rgb8_in, write_rgba8, write_rgba8_in, BlendMode,
    Layer, RgbSpace, RgbaImage, VitralImage, VitralStatus, CHUNK_TYPE,
};

const SIZE: u32 = 16;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

fn paint(f: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
    let mut data = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            data.extend_from_slice(&f(x, y));
        }
    }
    RgbaImage::new(SIZE, SIZE, data).unwrap()
}

/// Opaque backdrop: hue changes across, brightness down.
fn backdrop() -> RgbaImage {
    paint(|x, y| {
        [
            (x * 17) as u8,
            (y * 17) as u8,
            (255 - x * 8 - y * 7) as u8,
            255,
        ]
    })
}

/// Same colours, alpha fading from opaque (left) to fully transparent (right).
fn backdrop_transparent() -> RgbaImage {
    let mut img = backdrop();
    for (i, px) in img.data.chunks_mut(4).enumerate() {
        px[3] = 255 - (i as u32 % SIZE * 17) as u8;
    }
    img
}

/// Every pair (K, W) on a coarse grid, different on each channel, with the
/// native appearance approximated by the encoder (SPEC §5.1).
fn grid() -> VitralImage {
    let values: Vec<u8> = (0..=255u16).step_by(17).map(|v| v as u8).collect();
    let (mut black, mut white) = (Vec::new(), Vec::new());
    for &k in &values {
        for &w in &values {
            black.extend_from_slice(&[k, w, k / 2]);
            white.extend_from_slice(&[w, k, 255 - w / 3]);
        }
    }
    VitralImage::new(SIZE, SIZE, black, white).unwrap()
}

/// Layers as a design tool would stack them: a translucent multiply tint, a
/// screen glow, an exclusion stripe and a normal shape at half opacity. The
/// native appearance is exact and partly transparent.
fn layered() -> VitralImage {
    let layers = [
        Layer::new(
            paint(|x, _| [200, 40, 60, (x * 16) as u8]),
            BlendMode::Multiply,
        ),
        Layer::new(
            paint(|_, y| [250, 230, 120, (y * 12) as u8]),
            BlendMode::Screen,
        ),
        Layer::new(
            paint(|x, _| {
                if (4..8).contains(&x) {
                    [255, 255, 255, 255]
                } else {
                    [0; 4]
                }
            }),
            BlendMode::Exclusion,
        ),
        Layer::new(
            paint(|x, y| {
                if x + y > 20 {
                    [20, 90, 200, 255]
                } else {
                    [0; 4]
                }
            }),
            BlendMode::Normal,
        )
        .with_opacity(0.5),
    ];
    VitralImage::from_layers(SIZE, SIZE, &layers).unwrap()
}

/// Splits a PNG into (chunks before the first viTr or IEND, viTr payloads, IEND).
fn split(file: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut pos = 8;
    let mut main = file[..8].to_vec();
    let mut payloads = Vec::new();
    while pos + 12 <= file.len() {
        let len = u32::from_be_bytes(file[pos..pos + 4].try_into().unwrap()) as usize;
        let end = pos + 12 + len;
        if file[pos + 4..pos + 8] == CHUNK_TYPE {
            payloads.push(file[pos + 8..pos + 8 + len].to_vec());
        } else {
            main.extend_from_slice(&file[pos..end]);
        }
        pos = end;
    }
    (main, payloads)
}

/// Inserts viTr chunks with correct CRCs just before IEND.
fn with_chunks(main: &[u8], payloads: &[&[u8]]) -> Vec<u8> {
    let iend = main.len() - 12;
    let mut out = main[..iend].to_vec();
    for payload in payloads {
        let mut crc = crc32fast::Hasher::new();
        crc.update(&CHUNK_TYPE);
        crc.update(payload);
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&CHUNK_TYPE);
        out.extend_from_slice(payload);
        out.extend_from_slice(&crc.finalize().to_be_bytes());
    }
    out.extend_from_slice(&main[iend..]);
    out
}

fn mul255(a: u8, b: u8) -> u8 {
    let t = a as u32 * b as u32 + 128;
    ((t + (t >> 8)) >> 8) as u8
}

/// A viTr payload storing K and W the given way (SPEC §4.3, §4.5), whatever
/// the reference encoder would have chosen.
fn payload(img: &VitralImage, direct: bool, crc: [u8; 4]) -> Vec<u8> {
    let pixels = (img.width * img.height) as usize;
    let mut residual = vec![0u8; pixels * 6];
    for i in 0..pixels {
        let a = img.native.data[i * 4 + 3];
        for c in 0..3 {
            let (k, w) = (img.black[i * 3 + c], img.white[i * 3 + c]);
            let (rk, rw) = if direct {
                (k, w)
            } else {
                let fk = mul255(img.native.data[i * 4 + c], a);
                (k.wrapping_sub(fk), w.wrapping_sub(fk + (255 - a)))
            };
            residual[i * 3 + c] = rk;
            residual[(pixels + i) * 3 + c] = rw;
        }
    }
    let mut out = vec![0, 1, 1, direct as u8];
    out.extend_from_slice(&crc);
    out.extend(write_rgb8(img.width, img.height * 2, &residual).unwrap());
    out
}

struct Case {
    name: &'static str,
    description: &'static str,
    file: Vec<u8>,
    /// The image the file must decode to, for valid files.
    expected: Option<VitralImage>,
    status: &'static str,
}

fn status_name(status: &VitralStatus) -> &'static str {
    match status {
        VitralStatus::Ok => "ok",
        VitralStatus::NotVitral => "not-vitral",
        VitralStatus::FallbackMismatch => "fallback-mismatch",
        VitralStatus::Unsupported(_) => "unsupported",
        VitralStatus::Corrupt(_) => "corrupt",
    }
}

fn cases() -> Vec<Case> {
    let layered = layered();
    let encoded = encode(&layered).unwrap();
    let (main, payloads) = split(&encoded);
    let crc: [u8; 4] = payloads[0][4..8].try_into().unwrap();
    let predicted = payload(&layered, false, crc);
    let direct = payload(&layered, true, crc);
    let edit = |f: &dyn Fn(&mut Vec<u8>)| {
        let mut p = predicted.clone();
        f(&mut p);
        with_chunks(&main, &[&p])
    };

    // The main image with one visible pixel changed, as an editor would save it.
    let mut edited = layered.native.clone();
    let last = edited.data.len() - 4;
    edited.data[last..].copy_from_slice(&[255, 0, 255, 255]);
    let (edited_main, _) = split(&write_rgba8(&edited).unwrap());

    let mut damaged_crc = with_chunks(&main, &[&predicted]);
    let crc_at = damaged_crc.len() - 12 - 4;
    damaged_crc[crc_at] ^= 0xff;

    let p3 = layered.clone().in_colour_space(RgbSpace::DisplayP3);
    vec![
        Case {
            name: "ok-grid",
            description: "Every (K, W) pair on a 16-step grid, different per channel, \
                          including inversion (W < K). Native appearance approximated.",
            file: encode(&grid()).unwrap(),
            expected: Some(grid()),
            status: "ok",
        },
        Case {
            name: "ok-predicted",
            description: "Multiply, screen, exclusion and normal at 50% over a partly \
                          transparent native image. Residual stored as differences (flag 0).",
            file: with_chunks(&main, &[&predicted]),
            expected: Some(layered.clone()),
            status: "ok",
        },
        Case {
            name: "ok-direct",
            description: "Same image, residual storing K and W directly (flag bit 0 set).",
            file: with_chunks(&main, &[&direct]),
            expected: Some(layered.clone()),
            status: "ok",
        },
        Case {
            name: "ok-display-p3",
            description: "Same values, declared as Display P3 (iCCP and cICP). Blending \
                          happens on the encoded P3 values.",
            file: encode(&p3).unwrap(),
            expected: Some(p3.clone()),
            status: "ok",
        },
        Case {
            name: "not-vitral",
            description: "A plain PNG: rendered as a normal image with alpha.",
            file: write_rgba8(&layered.native).unwrap(),
            expected: None,
            status: "not-vitral",
        },
        Case {
            name: "fallback-mismatch",
            description: "The main image was edited after encoding (last pixel); the \
                          fallback CRC no longer matches, so the chunk is ignored.",
            file: with_chunks(&edited_main, &[&predicted]),
            expected: None,
            status: "fallback-mismatch",
        },
        Case {
            name: "corrupt-chunk-crc",
            description: "The viTr chunk's PNG CRC is wrong.",
            file: damaged_crc,
            expected: None,
            status: "corrupt",
        },
        Case {
            name: "corrupt-residual",
            description: "The embedded residual PNG is cut in half.",
            file: edit(&|p| p.truncate(8 + (p.len() - 8) / 2)),
            expected: None,
            status: "corrupt",
        },
        Case {
            name: "corrupt-short-payload",
            description: "The payload is shorter than its 8-byte header.",
            file: edit(&|p| p.truncate(5)),
            expected: None,
            status: "corrupt",
        },
        Case {
            name: "corrupt-two-chunks",
            description: "Two viTr chunks, each valid on its own.",
            file: with_chunks(&main, &[&predicted, &direct]),
            expected: None,
            status: "corrupt",
        },
        Case {
            name: "unsupported-version",
            description: "Major version 1, which a 0.x decoder does not know.",
            file: edit(&|p| p[0] = 1),
            expected: None,
            status: "unsupported",
        },
        Case {
            name: "unsupported-model",
            description: "Model 2, reserved for layered images (SPEC §12).",
            file: edit(&|p| p[2] = 2),
            expected: None,
            status: "unsupported",
        },
    ]
}

fn render(img: &VitralImage, backdrop: &RgbaImage) -> RgbaImage {
    let mut out = backdrop.clone();
    img.composite_onto(&mut out, 0, 0);
    out
}

fn manifest(cases: &[Case]) -> String {
    let mut out = String::from(
        "{\n  \"suite\": \"Vitral conformance\",\n  \"spec_version\": \"0.1\",\n  \
         \"backdrop\": \"backdrop.png\",\n  \"backdrop_transparent\": \"backdrop-transparent.png\",\n  \
         \"cases\": [\n",
    );
    for (i, case) in cases.iter().enumerate() {
        let space = if case.name.contains("p3") {
            "display-p3"
        } else {
            "srgb"
        };
        out += &format!("    {{\n      \"name\": \"{}\",\n", case.name);
        out += &format!("      \"file\": \"{}.vitral.png\",\n", case.name);
        out += &format!("      \"status\": \"{}\",\n", case.status);
        out += &format!("      \"colour_space\": \"{space}\",\n");
        if case.expected.is_some() {
            out += &format!("      \"k\": \"{}.k.png\",\n", case.name);
            out += &format!("      \"w\": \"{}.w.png\",\n", case.name);
        }
        out += &format!("      \"render\": \"{}.render.png\",\n", case.name);
        out += &format!(
            "      \"render_transparent\": \"{}.render-transparent.png\",\n",
            case.name
        );
        out += &format!("      \"description\": \"{}\"\n", case.description);
        out += if i + 1 < cases.len() {
            "    },\n"
        } else {
            "    }\n"
        };
    }
    out + "  ]\n}\n"
}

fn rgb(img: &RgbaImage) -> Vec<u8> {
    img.data.chunks(4).flat_map(|px| px[..3].to_vec()).collect()
}

fn write_suite(cases: &[Case]) {
    let dir = dir();
    std::fs::create_dir_all(&dir).unwrap();
    let put = |name: &str, bytes: Vec<u8>| std::fs::write(dir.join(name), bytes).unwrap();
    put("backdrop.png", write_rgba8(&backdrop()).unwrap());
    put(
        "backdrop-transparent.png",
        write_rgba8(&backdrop_transparent()).unwrap(),
    );
    for case in cases {
        put(&format!("{}.vitral.png", case.name), case.file.clone());
        let decoded = decode(&case.file).unwrap();
        let img = decoded.image();
        let space = img.colour_space;
        if let Some(expected) = &case.expected {
            put(
                &format!("{}.k.png", case.name),
                write_rgb8_in(SIZE, SIZE, &expected.black, space).unwrap(),
            );
            put(
                &format!("{}.w.png", case.name),
                write_rgb8_in(SIZE, SIZE, &expected.white, space).unwrap(),
            );
        }
        put(
            &format!("{}.render.png", case.name),
            write_rgba8_in(&render(&img, &backdrop()), space).unwrap(),
        );
        put(
            &format!("{}.render-transparent.png", case.name),
            write_rgba8_in(&render(&img, &backdrop_transparent()), space).unwrap(),
        );
    }
    std::fs::write(dir.join("manifest.json"), manifest(cases)).unwrap();
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(dir().join(name)).unwrap_or_else(|e| panic!("conformance/{name}: {e}"))
}

#[test]
fn reference_implementation_passes_the_conformance_suite() {
    let cases = cases();
    if std::env::var_os("VITRAL_WRITE_CONFORMANCE").is_some() {
        write_suite(&cases);
    }
    assert_eq!(
        String::from_utf8(read("manifest.json")).unwrap(),
        manifest(&cases),
        "manifest.json is out of date; regenerate the suite"
    );
    let backdrop = read_rgba8(&read("backdrop.png")).unwrap();
    let transparent = read_rgba8(&read("backdrop-transparent.png")).unwrap();
    assert_eq!(backdrop, self::backdrop());
    assert_eq!(transparent, backdrop_transparent());

    for case in &cases {
        let decoded = decode(&read(&format!("{}.vitral.png", case.name))).unwrap();
        assert_eq!(status_name(&decoded.status), case.status, "{}", case.name);
        if let Some(expected) = &case.expected {
            let img = decoded.vitral.as_ref().unwrap();
            assert_eq!(img, expected, "{}: decoded image", case.name);
            let k = read_rgba8(&read(&format!("{}.k.png", case.name))).unwrap();
            let w = read_rgba8(&read(&format!("{}.w.png", case.name))).unwrap();
            assert_eq!(rgb(&k), img.black, "{}: K", case.name);
            assert_eq!(rgb(&w), img.white, "{}: W", case.name);
        }
        let img = decoded.image();
        for (suffix, over) in [("render", &backdrop), ("render-transparent", &transparent)] {
            let expected = read_rgba8(&read(&format!("{}.{suffix}.png", case.name))).unwrap();
            assert_eq!(render(&img, over), expected, "{}: {suffix}", case.name);
        }
    }
}
