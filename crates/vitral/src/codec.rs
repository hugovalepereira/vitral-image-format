//! Reading and writing `.vitral.png` files.
//!
//! Layout of the `vITR` chunk payload (big-endian):
//!
//! | offset | size | field                                              |
//! |--------|------|----------------------------------------------------|
//! | 0      | 1    | major version (0 for drafts)                       |
//! | 1      | 1    | minor version (1)                                  |
//! | 2      | 1    | model (1 = affine, black/white)                    |
//! | 3      | 1    | flags (0)                                          |
//! | 4      | 4    | CRC-32 of the fallback pixels, as decoded to RGBA8 |
//! | 8      | ...  | residual: a PNG datastream, 8-bit RGB, width x 2*height |

use crate::fallback::least_squares_fallback;
use crate::math::mul255;
use crate::pngio::{chunks, read_rgb8_strict, read_rgba8_with_info, write_rgb8, write_with_chunk};
use crate::{Error, RgbaImage, VitralImage};

/// The PNG chunk type: ancillary, private, reserved bit clear, unsafe to copy
/// (it depends on the image data, so editors must drop it).
pub const CHUNK_TYPE: [u8; 4] = *b"vITR";

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
    /// No `vITR` chunk: a plain PNG.
    NotVitral,
    /// The main image no longer matches the one the residual was computed
    /// against (the file was edited or recompressed lossily).
    FallbackMismatch,
    /// A version, model or feature this decoder does not support.
    Unsupported(String),
    /// The `vITR` chunk is damaged.
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
        self.vitral.clone().unwrap_or_else(|| from_fallback(&self.fallback))
    }
}

/// Encodes a Vitral image with the default least-squares fallback.
pub fn encode(img: &VitralImage) -> Result<Vec<u8>, Error> {
    encode_with_fallback(img, &least_squares_fallback(img))
}

/// Encodes a Vitral image with a caller-chosen fallback (e.g. a render over the
/// background the artwork will most often be seen on). The residual makes the
/// result lossless whatever the fallback is.
pub fn encode_with_fallback(img: &VitralImage, fallback: &RgbaImage) -> Result<Vec<u8>, Error> {
    if fallback.width != img.width || fallback.height != img.height {
        return Err(Error::InvalidInput("fallback size differs from image size".into()));
    }
    let pixels = img.width as usize * img.height as usize;
    let mut residual = vec![0u8; pixels * 6];
    let (top, bottom) = residual.split_at_mut(pixels * 3);
    for i in 0..pixels {
        let a = fallback.data[i * 4 + 3];
        for c in 0..3 {
            let fk = mul255(fallback.data[i * 4 + c], a);
            let fw = fk + (255 - a);
            top[i * 3 + c] = img.black[i * 3 + c].wrapping_sub(fk);
            bottom[i * 3 + c] = img.white[i * 3 + c].wrapping_sub(fw);
        }
    }
    let residual_png = write_rgb8(img.width, img.height * 2, &residual)?;

    let mut payload = Vec::with_capacity(HEADER_LEN + residual_png.len());
    payload.extend_from_slice(&[VERSION_MAJOR, VERSION_MINOR, MODEL_AFFINE_KW, 0]);
    payload.extend_from_slice(&crc32fast::hash(&fallback.data).to_be_bytes());
    payload.extend_from_slice(&residual_png);

    write_with_chunk(fallback, Some((png::chunk::ChunkType(CHUNK_TYPE), &payload)))
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
        (Some(_), n) if n > 1 => VitralStatus::Corrupt("more than one vITR chunk".into()),
        (Some((_, false)), _) => VitralStatus::Corrupt("vITR chunk CRC mismatch".into()),
        _ if source.bit_depth_16 => VitralStatus::Unsupported("16-bit images".into()),
        _ if source.animated => VitralStatus::Unsupported("animated PNG".into()),
        (Some((payload, true)), _) => match parse_payload(payload, &fallback) {
            Ok(img) => {
                return Ok(Decoded { fallback, vitral: Some(img), status: VitralStatus::Ok });
            }
            Err(status) => status,
        },
    };
    Ok(Decoded { fallback, vitral: None, status })
}

fn parse_payload(payload: &[u8], fallback: &RgbaImage) -> Result<VitralImage, VitralStatus> {
    if payload.len() < HEADER_LEN {
        return Err(VitralStatus::Corrupt("vITR payload too short".into()));
    }
    let (major, minor, model) = (payload[0], payload[1], payload[2]);
    if major != VERSION_MAJOR {
        return Err(VitralStatus::Unsupported(format!("version {major}.{minor}")));
    }
    if model != MODEL_AFFINE_KW {
        return Err(VitralStatus::Unsupported(format!("model {model}")));
    }
    let stored_crc = u32::from_be_bytes(payload[4..8].try_into().unwrap());
    if crc32fast::hash(&fallback.data) != stored_crc {
        return Err(VitralStatus::FallbackMismatch);
    }
    let (rw, rh, residual) = read_rgb8_strict(&payload[HEADER_LEN..])
        .map_err(|e| VitralStatus::Corrupt(format!("residual: {e}")))?;
    if rw != fallback.width || rh as u64 != fallback.height as u64 * 2 {
        return Err(VitralStatus::Corrupt(format!(
            "residual is {rw}x{rh}, expected {}x{}",
            fallback.width,
            fallback.height as u64 * 2
        )));
    }

    let pixels = fallback.width as usize * fallback.height as usize;
    let mut black = vec![0u8; pixels * 3];
    let mut white = vec![0u8; pixels * 3];
    let (top, bottom) = residual.split_at(pixels * 3);
    for i in 0..pixels {
        let a = fallback.data[i * 4 + 3];
        for c in 0..3 {
            let fk = mul255(fallback.data[i * 4 + c], a);
            let fw = fk + (255 - a);
            black[i * 3 + c] = fk.wrapping_add(top[i * 3 + c]);
            white[i * 3 + c] = fw.wrapping_add(bottom[i * 3 + c]);
        }
    }
    Ok(VitralImage { width: fallback.width, height: fallback.height, black, white })
}

/// Interprets a plain RGBA image as a Vitral image with normal blending.
fn from_fallback(fallback: &RgbaImage) -> VitralImage {
    let pixels = fallback.width as usize * fallback.height as usize;
    let mut black = Vec::with_capacity(pixels * 3);
    let mut white = Vec::with_capacity(pixels * 3);
    for px in fallback.data.chunks_exact(4) {
        for &c in &px[..3] {
            let fk = mul255(c, px[3]);
            black.push(fk);
            white.push(fk + (255 - px[3]));
        }
    }
    VitralImage { width: fallback.width, height: fallback.height, black, white }
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

    #[test]
    fn round_trip_is_lossless() {
        let img = sample();
        let bytes = encode(&img).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.status, VitralStatus::Ok);
        assert_eq!(decoded.vitral.unwrap(), img);
    }

    #[test]
    fn round_trip_with_custom_fallback() {
        let img = sample();
        let fallback = RgbaImage::filled(img.width, img.height, [255, 0, 255, 77]);
        let bytes = encode_with_fallback(&img, &fallback).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.fallback, fallback);
        assert_eq!(decoded.vitral.unwrap(), img);
    }

    #[test]
    fn output_is_a_valid_png_with_the_fallback_as_main_image() {
        let img = sample();
        let bytes = encode(&img).unwrap();
        let main = read_rgba8(&bytes).unwrap();
        assert_eq!(main, least_squares_fallback(&img));
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
        let payload: Vec<u8> = chunks(&bytes).find(|c| c.0 == CHUNK_TYPE).unwrap().1.to_vec();
        let tampered =
            write_with_chunk(&main, Some((png::chunk::ChunkType(CHUNK_TYPE), &payload))).unwrap();
        let decoded = decode(&tampered).unwrap();
        assert_eq!(decoded.status, VitralStatus::FallbackMismatch);
        assert!(decoded.vitral.is_none());
    }

    #[test]
    fn damaged_chunk_is_detected() {
        let bytes = encode(&sample()).unwrap();
        let mut damaged = bytes.clone();
        // Flip a byte inside the vITR payload (after its 8-byte chunk header).
        let pos = bytes.windows(4).position(|w| w == CHUNK_TYPE).unwrap() + 4 + 10;
        damaged[pos] ^= 0x55;
        let decoded = decode(&damaged).unwrap();
        assert!(matches!(decoded.status, VitralStatus::Corrupt(_)));
    }

    #[test]
    fn unknown_major_version_is_unsupported() {
        let img = sample();
        let bytes = encode(&img).unwrap();
        let mut payload: Vec<u8> = chunks(&bytes).find(|c| c.0 == CHUNK_TYPE).unwrap().1.to_vec();
        payload[0] = 9;
        let fallback = read_rgba8(&bytes).unwrap();
        let file =
            write_with_chunk(&fallback, Some((png::chunk::ChunkType(CHUNK_TYPE), &payload))).unwrap();
        assert!(matches!(decode(&file).unwrap().status, VitralStatus::Unsupported(_)));
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
        let (_, _, residual) = read_rgb8_strict(&payload[HEADER_LEN..]).unwrap();
        assert!(residual.iter().all(|&v| v == 0));
    }
}
