//! Small PNG helpers: decoding any 8-bit PNG to RGBA8 and encoding RGB8/RGBA8.

use std::io::Cursor;

use crate::Error;

/// A straight-alpha (non-premultiplied) RGBA8 image, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, Error> {
        let expected = width as usize * height as usize * 4;
        if data.len() != expected {
            return Err(Error::InvalidInput(format!(
                "RGBA buffer has {} bytes, expected {expected} for {width}x{height}",
                data.len()
            )));
        }
        Ok(RgbaImage {
            width,
            height,
            data,
        })
    }

    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let data = rgba.repeat(width as usize * height as usize);
        RgbaImage {
            width,
            height,
            data,
        }
    }
}

/// Information about the source PNG that the Vitral decoder needs.
pub(crate) struct SourceInfo {
    pub bit_depth_16: bool,
    pub animated: bool,
}

/// Decodes any PNG into RGBA8, following the PNG expansion rules (palette and
/// `tRNS` to RGBA, grayscale replicated to RGB, missing alpha set to 255).
/// 16-bit samples are reduced to 8 bits.
pub fn read_rgba8(bytes: &[u8]) -> Result<RgbaImage, Error> {
    read_rgba8_with_info(bytes).map(|(img, _)| img)
}

pub(crate) fn read_rgba8_with_info(bytes: &[u8]) -> Result<(RgbaImage, SourceInfo), Error> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    let source = SourceInfo {
        bit_depth_16: info.bit_depth == png::BitDepth::Sixteen,
        animated: info.animation_control.is_some(),
    };
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::Png("image too large".into()))?;
    let mut buf = vec![0; size];
    let frame = reader.next_frame(&mut buf)?;
    buf.truncate(frame.buffer_size());

    let (width, height) = (frame.width, frame.height);
    let pixels = width as usize * height as usize;
    let data = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(pixels * 4);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(pixels * 4);
            for px in buf.chunks_exact(2) {
                out.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(pixels * 4);
            for &g in &buf {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        png::ColorType::Indexed => {
            return Err(Error::Png("indexed PNG was not expanded".into()));
        }
    };
    Ok((
        RgbaImage {
            width,
            height,
            data,
        },
        source,
    ))
}

/// Decodes a PNG that must be 8-bit RGB (used for the embedded residual).
pub(crate) fn read_rgb8_strict(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), Error> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    if info.color_type != png::ColorType::Rgb || info.bit_depth != png::BitDepth::Eight {
        return Err(Error::Png("residual must be 8-bit RGB".into()));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::Png("residual too large".into()))?;
    let mut buf = vec![0; size];
    let frame = reader.next_frame(&mut buf)?;
    buf.truncate(frame.buffer_size());
    Ok((frame.width, frame.height, buf))
}

fn encoder<'a>(
    out: &'a mut Vec<u8>,
    width: u32,
    height: u32,
    color: png::ColorType,
) -> png::Encoder<'a, &'a mut Vec<u8>> {
    let mut enc = png::Encoder::new(out, width, height);
    enc.set_color(color);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::High);
    enc
}

/// Encodes an RGBA8 image as a PNG tagged as sRGB.
pub fn write_rgba8(img: &RgbaImage) -> Result<Vec<u8>, Error> {
    write_with_chunk(img, None)
}

/// Encodes an RGBA8 image, optionally appending one extra chunk after `IDAT`.
pub(crate) fn write_with_chunk(
    img: &RgbaImage,
    extra: Option<(png::chunk::ChunkType, &[u8])>,
) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    {
        let mut enc = encoder(&mut out, img.width, img.height, png::ColorType::Rgba);
        enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = enc.write_header()?;
        writer.write_image_data(&img.data)?;
        if let Some((kind, data)) = extra {
            writer.write_chunk(kind, data)?;
        }
        writer.finish()?;
    }
    Ok(out)
}

/// Encodes an RGB8 buffer as a plain PNG (no colour-space chunks).
pub fn write_rgb8(width: u32, height: u32, rgb: &[u8]) -> Result<Vec<u8>, Error> {
    if rgb.len() != width as usize * height as usize * 3 {
        return Err(Error::InvalidInput("RGB buffer size mismatch".into()));
    }
    let mut out = Vec::new();
    {
        let enc = encoder(&mut out, width, height, png::ColorType::Rgb);
        let mut writer = enc.write_header()?;
        writer.write_image_data(rgb)?;
        writer.finish()?;
    }
    Ok(out)
}

/// Iterates over the chunks of a PNG datastream, yielding
/// `(type, data, crc_ok)`. Stops at `IEND` or at the first malformed chunk.
pub(crate) fn chunks(bytes: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8], bool)> {
    const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    let mut pos = if bytes.starts_with(&SIGNATURE) {
        8
    } else {
        bytes.len()
    };
    std::iter::from_fn(move || {
        if pos + 12 > bytes.len() {
            return None;
        }
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = bytes[pos + 4..pos + 8].try_into().unwrap();
        let data_start = pos + 8;
        let data_end = data_start.checked_add(len)?;
        if data_end + 4 > bytes.len() {
            return None;
        }
        let data = &bytes[data_start..data_end];
        let stored = u32::from_be_bytes(bytes[data_end..data_end + 4].try_into().unwrap());
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&kind);
        hasher.update(data);
        let crc_ok = hasher.finalize() == stored;
        pos = if &kind == b"IEND" {
            bytes.len()
        } else {
            data_end + 4
        };
        Some((kind, data, crc_ok))
    })
}
