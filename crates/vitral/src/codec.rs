//! Reading and writing `.vitral.png` files.
//!
//! Layout of the `viTr` chunk payload (big-endian):
//!
//! | offset | size | field                                              |
//! |--------|------|----------------------------------------------------|
//! | 0      | 1    | major version (0 for drafts)                       |
//! | 1      | 1    | minor version (1)                                  |
//! | 2      | 1    | model (1 = affine, black/white)                    |
//! | 3      | 1    | flags (bit 0: K and W stored directly)             |
//! | 4      | 4    | CRC-32 of the main image pixels (see [`fallback_crc`]) |
//! | 8      | ...  | residual: a PNG datastream, 8-bit RGB, width x 2*height |
//!
//! The main PNG image is the native appearance (how the artwork looks with
//! nothing behind it), which is also what software without Vitral support
//! shows: the fallback.

use crate::math::mul255;
use crate::pngio::{chunks, read_rgb8_exact, read_rgba8_with_info, write_rgb8, write_with_chunk};
use crate::{Error, RgbaImage, VitralImage};

/// The PNG chunk type: ancillary, private, reserved bit clear, safe to copy.
/// The chunk depends on the pixel values but checks them itself (the CRC of
/// the main image), so lossless optimisers may keep it and any visible change
/// to the pixels is detected.
pub const CHUNK_TYPE: [u8; 4] = *b"viTr";

const VERSION_MAJOR: u8 = 0;
const VERSION_MINOR: u8 = 1;
const MODEL_AFFINE_KW: u8 = 1;
const HEADER_LEN: usize = 8;

/// Why a file did or did not yield Vitral data. In every case the fallback
/// image is still available.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VitralStatus {
    /// The file carries valid Vitral data.
    Ok,
    /// No `viTr` chunk: a plain PNG.
    NotVitral,
    /// The main image no longer matches the one the residual was computed
    /// against (the file was edited or recompressed lossily).
    FallbackMismatch,
    /// A version, model or feature this decoder does not support.
    Unsupported(String),
    /// The `viTr` chunk is damaged.
    Corrupt(String),
}

#[derive(Clone, Debug)]
pub struct Decoded {
    /// The main PNG image, as any viewer would show it.
    pub fallback: RgbaImage,
    /// The Vitral image, when `status` is `Ok`.
    pub vitral: Option<VitralImage>,
    pub status: VitralStatus,
}

impl Decoded {
    /// The Vitral image if present, otherwise the fallback interpreted as a
    /// plain normal-blended image. Always usable for rendering.
    pub fn image(&self) -> VitralImage {
        self.vitral
            .clone()
            .unwrap_or_else(|| from_fallback(&self.fallback))
    }
}

/// Encodes a Vitral image. Its native appearance becomes the main PNG image;
/// the residual makes the file lossless whatever that image is.
///
/// K and W are stored either as differences from the native image composited
/// over black and white (small where the image behaves like a plain PNG), or
/// directly (smaller where it blends a lot). The encoder keeps the smaller.
pub fn encode(img: &VitralImage) -> Result<Vec<u8>, Error> {
    let predicted = encode_with(img, Storage::Predicted)?;
    let direct = encode_with(img, Storage::Direct)?;
    Ok(if direct.len() < predicted.len() {
        direct
    } else {
        predicted
    })
}

/// How the residual image stores K and W (flag bit 0 of the payload).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Storage {
    /// `(K - Fk) mod 256` and `(W - Fw) mod 256`.
    Predicted,
    /// K and W themselves.
    Direct,
}

const FLAG_DIRECT: u8 = 1;

fn encode_with(img: &VitralImage, storage: Storage) -> Result<Vec<u8>, Error> {
    let fallback = &img.native;
    if fallback.width != img.width || fallback.height != img.height {
        return Err(Error::InvalidInput(
            "native image size differs from image size".into(),
        ));
    }
    let pixels = img.width as usize * img.height as usize;
    let mut residual = vec![0u8; pixels * 6];
    let (top, bottom) = residual.split_at_mut(pixels * 3);
    match storage {
        Storage::Direct => {
            top.copy_from_slice(&img.black);
            bottom.copy_from_slice(&img.white);
        }
        Storage::Predicted => {
            for i in 0..pixels {
                let a = fallback.data[i * 4 + 3];
                for c in 0..3 {
                    let fk = mul255(fallback.data[i * 4 + c], a);
                    let fw = fk + (255 - a);
                    top[i * 3 + c] = img.black[i * 3 + c].wrapping_sub(fk);
                    bottom[i * 3 + c] = img.white[i * 3 + c].wrapping_sub(fw);
                }
            }
        }
    }
    let residual_png = write_rgb8(img.width, img.height * 2, &residual)?;

    let flags = if storage == Storage::Direct {
        FLAG_DIRECT
    } else {
        0
    };
    let mut payload = Vec::with_capacity(HEADER_LEN + residual_png.len());
    payload.extend_from_slice(&[VERSION_MAJOR, VERSION_MINOR, MODEL_AFFINE_KW, flags]);
    payload.extend_from_slice(&fallback_crc(fallback).to_be_bytes());
    payload.extend_from_slice(&residual_png);

    write_with_chunk(
        fallback,
        Some((png::chunk::ChunkType(CHUNK_TYPE), &payload)),
    )
}

/// Decodes any PNG. Fails only if the main image itself cannot be decoded;
/// problems with the Vitral data are reported through [`Decoded::status`].
pub fn decode(bytes: &[u8]) -> Result<Decoded, Error> {
    let (fallback, source) = read_rgba8_with_info(bytes)?;
    let mut found = None;
    let mut count = 0;
    for (kind, data, crc_ok) in chunks(bytes) {
        if kind == CHUNK_TYPE {
            count += 1;
            found = Some((data, crc_ok));
        }
    }
    let status = match (found, count) {
        (None, _) => VitralStatus::NotVitral,
        (Some(_), n) if n > 1 => VitralStatus::Corrupt("more than one viTr chunk".into()),
        (Some((_, false)), _) => VitralStatus::Corrupt("viTr chunk CRC mismatch".into()),
        _ if source.bit_depth_16 => VitralStatus::Unsupported("16-bit images".into()),
        _ if source.animated => VitralStatus::Unsupported("animated PNG".into()),
        (Some((payload, true)), _) => match parse_payload(payload, &fallback) {
            Ok(img) => {
                return Ok(Decoded {
                    fallback,
                    vitral: Some(img),
                    status: VitralStatus::Ok,
                });
            }
            Err(status) => status,
        },
    };
    Ok(Decoded {
        fallback,
        vitral: None,
        status,
    })
}

fn parse_payload(payload: &[u8], fallback: &RgbaImage) -> Result<VitralImage, VitralStatus> {
    if payload.len() < HEADER_LEN {
        return Err(VitralStatus::Corrupt("viTr payload too short".into()));
    }
    let (major, minor, model) = (payload[0], payload[1], payload[2]);
    if major != VERSION_MAJOR {
        return Err(VitralStatus::Unsupported(format!(
            "version {major}.{minor}"
        )));
    }
    if model != MODEL_AFFINE_KW {
        return Err(VitralStatus::Unsupported(format!("model {model}")));
    }
    let stored_crc = u32::from_be_bytes(payload[4..8].try_into().unwrap());
    if fallback_crc(fallback) != stored_crc {
        return Err(VitralStatus::FallbackMismatch);
    }
    let residual_height = fallback
        .height
        .checked_mul(2)
        .ok_or_else(|| VitralStatus::Unsupported("image too tall".into()))?;
    let residual = read_rgb8_exact(&payload[HEADER_LEN..], fallback.width, residual_height)
        .map_err(|e| VitralStatus::Corrupt(format!("residual: {e}")))?;

    let pixels = fallback.width as usize * fallback.height as usize;
    let (top, bottom) = residual.split_at(pixels * 3);
    if payload[3] & FLAG_DIRECT != 0 {
        return Ok(VitralImage {
            width: fallback.width,
            height: fallback.height,
            black: top.to_vec(),
            white: bottom.to_vec(),
            native: fallback.clone(),
        });
    }
    let mut black = vec![0u8; pixels * 3];
    let mut white = vec![0u8; pixels * 3];
    for i in 0..pixels {
        let a = fallback.data[i * 4 + 3];
        for c in 0..3 {
            let fk = mul255(fallback.data[i * 4 + c], a);
            let fw = fk + (255 - a);
            black[i * 3 + c] = fk.wrapping_add(top[i * 3 + c]);
            white[i * 3 + c] = fw.wrapping_add(bottom[i * 3 + c]);
        }
    }
    Ok(VitralImage {
        width: fallback.width,
        height: fallback.height,
        black,
        white,
        native: fallback.clone(),
    })
}

/// The fallback CRC (SPEC §4.4): CRC-32 of the main image as RGBA8, with
/// every fully transparent pixel counted as (0, 0, 0, 0).
///
/// The colour of a pixel with alpha 0 is invisible and plays no part in K and
/// W, and optimisers such as `oxipng -a` or `pngcrush -blacken` rewrite it.
/// Leaving it out keeps the Vitral data valid after such a cleanup.
fn fallback_crc(img: &RgbaImage) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    let row_len = img.width as usize * 4;
    let mut row = Vec::with_capacity(row_len);
    for src in img.data.chunks(row_len.max(1)) {
        row.clear();
        for px in src.as_chunks::<4>().0 {
            row.extend_from_slice(if px[3] == 0 { &[0; 4] } else { px });
        }
        hasher.update(&row);
    }
    hasher.finalize()
}

/// Interprets a plain RGBA image as a Vitral image with normal blending.
fn from_fallback(fallback: &RgbaImage) -> VitralImage {
    let pixels = fallback.width as usize * fallback.height as usize;
    let mut black = Vec::with_capacity(pixels * 3);
    let mut white = Vec::with_capacity(pixels * 3);
    for px in fallback.data.as_chunks::<4>().0 {
        for &c in &px[..3] {
            let fk = mul255(c, px[3]);
            black.push(fk);
            white.push(fk + (255 - px[3]));
        }
    }
    VitralImage {
        width: fallback.width,
        height: fallback.height,
        black,
        white,
        native: fallback.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_rgba8, write_rgba8, BlendMode, Layer};

    /// A small but varied image: every (K, W) combination on a coarse grid.
    fn sample() -> VitralImage {
        let values: Vec<u8> = (0..=255u16).step_by(17).map(|v| v as u8).collect();
        let n = values.len() as u32;
        let (mut black, mut white) = (Vec::new(), Vec::new());
        for &k in &values {
            for &w in &values {
                black.extend_from_slice(&[k, w, k / 2]);
                white.extend_from_slice(&[w, k, 255 - w / 3]);
            }
        }
        VitralImage::new(n, n, black, white).unwrap()
    }

    /// The four letter cases of the chunk name carry its properties.
    #[test]
    fn chunk_type_is_ancillary_private_and_safe_to_copy() {
        use png::chunk::{is_critical, is_private, reserved_set, safe_to_copy, ChunkType};
        let kind = ChunkType(CHUNK_TYPE);
        assert!(!is_critical(kind));
        assert!(is_private(kind));
        assert!(!reserved_set(kind));
        assert!(safe_to_copy(kind));
    }

    #[test]
    fn round_trip_is_lossless() {
        let img = sample();
        let bytes = encode(&img).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.status, VitralStatus::Ok);
        assert_eq!(decoded.vitral.unwrap(), img);
    }

    #[test]
    fn both_storage_modes_round_trip() {
        let img = sample();
        for storage in [Storage::Predicted, Storage::Direct] {
            let decoded = decode(&encode_with(&img, storage).unwrap()).unwrap();
            assert_eq!(decoded.vitral.unwrap(), img, "{storage:?}");
        }
    }

    #[test]
    fn round_trip_with_any_native_image() {
        let native = RgbaImage::filled(16, 16, [255, 0, 255, 77]);
        let img = sample().with_native(native.clone()).unwrap();
        let bytes = encode(&img).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.fallback, native);
        assert_eq!(decoded.vitral.unwrap(), img);
    }

    #[test]
    fn output_is_a_valid_png_with_the_native_image_as_main_image() {
        let img = sample();
        let bytes = encode(&img).unwrap();
        let main = read_rgba8(&bytes).unwrap();
        assert_eq!(main, img.native);
    }

    #[test]
    fn plain_png_is_reported_as_not_vitral() {
        let png = write_rgba8(&RgbaImage::filled(3, 2, [1, 2, 3, 4])).unwrap();
        let decoded = decode(&png).unwrap();
        assert_eq!(decoded.status, VitralStatus::NotVitral);
        // It still renders as a normal image.
        assert_eq!(decoded.image().width, 3);
    }

    #[test]
    fn edited_main_image_is_detected() {
        let img = sample();
        let bytes = encode(&img).unwrap();
        // Re-encode the main image with one pixel changed but keep the chunk,
        // as a careless tool would.
        let mut main = read_rgba8(&bytes).unwrap();
        main.data[0] ^= 0xff;
        let payload: Vec<u8> = chunks(&bytes)
            .find(|c| c.0 == CHUNK_TYPE)
            .unwrap()
            .1
            .to_vec();
        let tampered =
            write_with_chunk(&main, Some((png::chunk::ChunkType(CHUNK_TYPE), &payload))).unwrap();
        let decoded = decode(&tampered).unwrap();
        assert_eq!(decoded.status, VitralStatus::FallbackMismatch);
        assert!(decoded.vitral.is_none());
    }

    /// Optimisers may rewrite the colour of fully transparent pixels; that
    /// colour is invisible, so the Vitral data must survive it.
    #[test]
    fn colour_of_transparent_pixels_is_ignored() {
        let mut native = RgbaImage::filled(16, 16, [200, 30, 90, 255]);
        for px in native.data.chunks_mut(4).step_by(3) {
            px.copy_from_slice(&[12, 34, 56, 0]);
        }
        let img = sample().with_native(native).unwrap();
        let bytes = encode(&img).unwrap();
        let mut main = read_rgba8(&bytes).unwrap();
        for px in main.data.chunks_mut(4).filter(|px| px[3] == 0) {
            px.copy_from_slice(&[0, 0, 0, 0]);
        }
        let payload: Vec<u8> = chunks(&bytes)
            .find(|c| c.0 == CHUNK_TYPE)
            .unwrap()
            .1
            .to_vec();
        let cleaned =
            write_with_chunk(&main, Some((png::chunk::ChunkType(CHUNK_TYPE), &payload))).unwrap();
        let decoded = decode(&cleaned).unwrap();
        assert_eq!(decoded.status, VitralStatus::Ok);
        let vitral = decoded.vitral.unwrap();
        assert_eq!((vitral.black, vitral.white), (img.black, img.white));
    }

    #[test]
    fn fallback_crc_ignores_only_fully_transparent_colour() {
        let a = RgbaImage::filled(2, 1, [9, 8, 7, 0]);
        let b = RgbaImage::filled(2, 1, [0, 0, 0, 0]);
        assert_eq!(fallback_crc(&a), fallback_crc(&b));
        assert_eq!(fallback_crc(&b), crc32fast::hash(&b.data));
        let c = RgbaImage::filled(2, 1, [9, 8, 7, 1]);
        let d = RgbaImage::filled(2, 1, [0, 0, 0, 1]);
        assert_ne!(fallback_crc(&c), fallback_crc(&d));
    }

    #[test]
    fn damaged_chunk_is_detected() {
        let bytes = encode(&sample()).unwrap();
        let mut damaged = bytes.clone();
        // Flip a byte inside the viTr payload (after its 8-byte chunk header).
        let pos = bytes.windows(4).position(|w| w == CHUNK_TYPE).unwrap() + 4 + 10;
        damaged[pos] ^= 0x55;
        let decoded = decode(&damaged).unwrap();
        assert!(matches!(decoded.status, VitralStatus::Corrupt(_)));
    }

    #[test]
    fn unknown_major_version_is_unsupported() {
        let img = sample();
        let bytes = encode(&img).unwrap();
        let mut payload: Vec<u8> = chunks(&bytes)
            .find(|c| c.0 == CHUNK_TYPE)
            .unwrap()
            .1
            .to_vec();
        payload[0] = 9;
        let fallback = read_rgba8(&bytes).unwrap();
        let file = write_with_chunk(
            &fallback,
            Some((png::chunk::ChunkType(CHUNK_TYPE), &payload)),
        )
        .unwrap();
        assert!(matches!(
            decode(&file).unwrap().status,
            VitralStatus::Unsupported(_)
        ));
    }

    #[test]
    fn normal_regions_have_an_empty_residual() {
        // A plain semi-transparent image: the residual must be all zeros, so
        // the file costs barely more than the equivalent PNG.
        let mut data = Vec::new();
        for i in 0..64u32 {
            data.extend_from_slice(&[(i * 4) as u8, 255 - (i * 3) as u8, 90, (i * 4 + 3) as u8]);
        }
        let layer = Layer::new(RgbaImage::new(8, 8, data).unwrap(), BlendMode::Normal);
        let img = VitralImage::from_layers(8, 8, &[layer]).unwrap();
        let bytes = encode(&img).unwrap();
        let payload = chunks(&bytes).find(|c| c.0 == CHUNK_TYPE).unwrap().1;
        assert_eq!(
            payload[3] & FLAG_DIRECT,
            0,
            "the encoder should pick the residual"
        );
        let residual = read_rgb8_exact(&payload[HEADER_LEN..], 8, 16).unwrap();
        assert!(residual.iter().all(|&v| v == 0));
    }

    #[test]
    fn residual_with_wrong_dimensions_is_rejected() {
        let img = sample();
        let fallback = img.native.clone();
        // A residual that claims to be far larger than the image.
        let mut payload = vec![VERSION_MAJOR, VERSION_MINOR, MODEL_AFFINE_KW, 0];
        payload.extend_from_slice(&fallback_crc(&fallback).to_be_bytes());
        payload.extend_from_slice(&write_rgb8(1, 1, &[0, 0, 0]).unwrap());
        let mut big = payload.clone();
        // Patch the embedded IHDR width/height (bytes 16..24 of the PNG).
        big[HEADER_LEN + 16..HEADER_LEN + 24].copy_from_slice(&[0, 1, 0, 0, 0, 1, 0, 0]);
        for p in [payload, big] {
            let file =
                write_with_chunk(&fallback, Some((png::chunk::ChunkType(CHUNK_TYPE), &p))).unwrap();
            assert!(matches!(
                decode(&file).unwrap().status,
                VitralStatus::Corrupt(_)
            ));
        }
    }
}
