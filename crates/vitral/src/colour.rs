//! Colour spaces: detecting the one a PNG declares, and converting between
//! the two that Vitral images can be in (SPEC §4.1).
//!
//! Vitral blends the encoded values of its colour space, as design tools and
//! CSS do, so an encoder must never take, say, Display P3 samples as sRGB.

use crate::pngio::chunks;
use crate::RgbaImage;

/// The colour space a PNG declares for its samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColourSpace {
    /// Declared sRGB: an `sRGB` chunk, an ICC profile with sRGB primaries, or
    /// `cICP` with BT.709 primaries and the sRGB transfer function.
    Srgb,
    /// Declared Display P3: an ICC profile with P3 primaries, or `cICP` with
    /// P3 (D65) primaries and the sRGB transfer function.
    DisplayP3,
    /// No colour information, which by convention means sRGB.
    Unspecified,
    /// Another colour space, or linear light, described for people (e.g.
    /// `ICC profile "Adobe RGB (1998)"`).
    Other(String),
}

impl ColourSpace {
    /// Whether the samples can be used as sRGB values.
    pub fn is_srgb(&self) -> bool {
        matches!(self, ColourSpace::Srgb | ColourSpace::Unspecified)
    }

    /// The Vitral colour space these samples are in, if Vitral can carry it.
    pub fn rgb_space(&self) -> Option<RgbSpace> {
        match self {
            ColourSpace::Srgb | ColourSpace::Unspecified => Some(RgbSpace::Srgb),
            ColourSpace::DisplayP3 => Some(RgbSpace::DisplayP3),
            ColourSpace::Other(_) => None,
        }
    }
}

/// The colour spaces a Vitral image can be in. Both use the sRGB transfer
/// function; they differ in their primaries (Display P3 has a wider gamut).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RgbSpace {
    #[default]
    Srgb,
    DisplayP3,
}

impl RgbSpace {
    /// The CSS name of the colour space (`srgb`, `display-p3`).
    pub fn css_name(self) -> &'static str {
        match self {
            RgbSpace::Srgb => "srgb",
            RgbSpace::DisplayP3 => "display-p3",
        }
    }

    /// The space that holds the colours of all the given ones: Display P3 if
    /// any is Display P3 (its gamut contains sRGB), otherwise sRGB.
    pub fn widest(spaces: impl IntoIterator<Item = RgbSpace>) -> RgbSpace {
        spaces
            .into_iter()
            .max_by_key(|s| *s == RgbSpace::DisplayP3)
            .unwrap_or_default()
    }
}

/// Linear sRGB to linear Display P3 (both D65).
const SRGB_TO_P3: [[f32; 3]; 3] = [
    [0.822_462_1, 0.177_538, 0.0],
    [0.033_194_2, 0.966_805_8, 0.0],
    [0.017_082_7, 0.072_397_4, 0.910_519_9],
];

/// Linear Display P3 to linear sRGB (both D65).
const P3_TO_SRGB: [[f32; 3]; 3] = [
    [1.224_940_2, -0.224_940_4, 0.0],
    [-0.042_056_9, 1.042_057_1, 0.0],
    [-0.019_637_6, -0.078_636_1, 1.098_273_7],
];

/// The sRGB transfer function, encoded 8-bit value to linear light.
fn to_linear_table() -> [f32; 256] {
    std::array::from_fn(|i| {
        let v = i as f32 / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    })
}

/// Linear light to an encoded 8-bit value, clipping to the gamut.
fn from_linear(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let e = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (e * 255.0).round() as u8
}

/// Converts colours from one Vitral colour space to another, in place.
/// `stride` is the number of bytes per pixel (3 for RGB, 4 for RGBA; any
/// fourth byte is left alone). Colours outside the target gamut are clipped.
pub fn convert_pixels(data: &mut [u8], stride: usize, from: RgbSpace, to: RgbSpace) {
    let matrix = match (from, to) {
        (RgbSpace::Srgb, RgbSpace::DisplayP3) => SRGB_TO_P3,
        (RgbSpace::DisplayP3, RgbSpace::Srgb) => P3_TO_SRGB,
        _ => return,
    };
    let linear = to_linear_table();
    // Images often repeat colours (flat areas, transparent pixels).
    let mut last: Option<([u8; 3], [u8; 3])> = None;
    for px in data.chunks_exact_mut(stride) {
        let rgb = [px[0], px[1], px[2]];
        let out = match last {
            Some((src, out)) if src == rgb => out,
            _ => {
                let l = rgb.map(|c| linear[c as usize]);
                let out =
                    matrix.map(|row| from_linear(row[0] * l[0] + row[1] * l[1] + row[2] * l[2]));
                last = Some((rgb, out));
                out
            }
        };
        px[..3].copy_from_slice(&out);
    }
}

/// Converts a straight-alpha image to another colour space (alpha unchanged).
pub fn convert_rgba(img: &RgbaImage, from: RgbSpace, to: RgbSpace) -> RgbaImage {
    let mut out = img.clone();
    convert_pixels(&mut out.data, 4, from, to);
    out
}

/// D50-adapted primaries of Display P3, as stored in ICC profiles.
const P3_ICC_PRIMARIES: [[f64; 3]; 3] = [
    [0.5151, 0.2412, -0.0011],
    [0.2920, 0.6922, 0.0419],
    [0.1571, 0.0666, 0.7841],
];

/// Display P3 chromaticities (white, red, green, blue), as in a `cHRM` chunk.
const P3_CHROMATICITIES: [f64; 8] = [0.3127, 0.3290, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060];

/// A Display P3 ICC profile (version 4, sRGB transfer curve) for the files
/// Vitral writes.
pub(crate) fn display_p3_icc() -> Vec<u8> {
    let s15 = |v: f64| ((v * 65536.0).round() as i32).to_be_bytes();
    let xyz = |v: [f64; 3]| -> Vec<u8> {
        let mut t = b"XYZ \0\0\0\0".to_vec();
        v.iter().for_each(|&c| t.extend_from_slice(&s15(c)));
        t
    };
    let mluc = |text: &str| -> Vec<u8> {
        let units: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let mut t = b"mluc\0\0\0\0".to_vec();
        for v in [1u32, 12] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        t.extend_from_slice(b"enUS");
        t.extend_from_slice(&(units.len() as u32).to_be_bytes());
        t.extend_from_slice(&28u32.to_be_bytes());
        t.extend_from_slice(&units);
        t
    };
    // Parametric curve type 3: the sRGB transfer function.
    let mut trc = b"para\0\0\0\0\0\x03\0\0".to_vec();
    for v in [2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.040_45] {
        trc.extend_from_slice(&s15(v));
    }
    // Bradford adaptation from D65 to D50.
    let mut chad = b"sf32\0\0\0\0".to_vec();
    for v in [
        1.047_882, 0.022_918, -0.050_217, 0.029_586, 0.990_478, -0.017_075, -0.009_233, 0.015_052,
        0.751_678,
    ] {
        chad.extend_from_slice(&s15(v));
    }
    let tags: [(&[u8; 4], Vec<u8>); 10] = [
        (b"desc", mluc("Display P3")),
        (b"cprt", mluc("No copyright, use freely")),
        (b"wtpt", xyz([0.9642, 1.0, 0.8249])),
        (b"chad", chad),
        (b"rXYZ", xyz(P3_ICC_PRIMARIES[0])),
        (b"gXYZ", xyz(P3_ICC_PRIMARIES[1])),
        (b"bXYZ", xyz(P3_ICC_PRIMARIES[2])),
        (b"rTRC", trc.clone()),
        (b"gTRC", trc.clone()),
        (b"bTRC", trc),
    ];
    let mut table = (tags.len() as u32).to_be_bytes().to_vec();
    let mut body = Vec::new();
    let start = 128 + 4 + 12 * tags.len();
    for (signature, data) in &tags {
        table.extend_from_slice(*signature);
        table.extend_from_slice(&((start + body.len()) as u32).to_be_bytes());
        table.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        body.resize(body.len().next_multiple_of(4), 0);
    }
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(&((start + body.len()) as u32).to_be_bytes());
    header[8..12].copy_from_slice(&[4, 0x30, 0, 0]);
    header[12..16].copy_from_slice(b"mntr");
    header[16..20].copy_from_slice(b"RGB ");
    header[20..24].copy_from_slice(b"XYZ ");
    header[24..36].copy_from_slice(&[0x07, 0xea, 0, 10, 0, 16, 0, 0, 0, 0, 0, 0]);
    header[36..40].copy_from_slice(b"acsp");
    for (i, v) in [0.9642, 1.0, 0.8249].into_iter().enumerate() {
        header[68 + i * 4..72 + i * 4].copy_from_slice(&s15(v));
    }
    [header, table, body].concat()
}

/// sRGB primaries adapted to D50, as stored in ICC profiles (rXYZ, gXYZ, bXYZ).
const SRGB_ICC_PRIMARIES: [[f64; 3]; 3] = [
    [0.4361, 0.2225, 0.0139],
    [0.3851, 0.7169, 0.0971],
    [0.1431, 0.0606, 0.7141],
];

/// sRGB chromaticities (white, red, green, blue), as in a `cHRM` chunk.
const SRGB_CHROMATICITIES: [f64; 8] = [0.3127, 0.3290, 0.64, 0.33, 0.30, 0.60, 0.15, 0.06];

/// Reads the colour space a PNG declares. Follows the PNG precedence: `cICP`,
/// then `iCCP`, then `sRGB`, then `cHRM`.
pub fn colour_space(png: &[u8]) -> ColourSpace {
    let (mut cicp, mut iccp, mut srgb, mut chrm, mut gama) = (None, None, false, None, None);
    for (kind, data, crc_ok) in chunks(png) {
        if !crc_ok {
            continue;
        }
        match &kind {
            b"cICP" if data.len() >= 2 => cicp = Some((data[0], data[1])),
            b"iCCP" => iccp = Some(data),
            b"sRGB" => srgb = true,
            b"cHRM" if data.len() == 32 => chrm = Some(data),
            b"gAMA" if data.len() == 4 => {
                gama = Some(u32::from_be_bytes(data.try_into().unwrap()) as f64 / 100_000.0)
            }
            b"IDAT" => break,
            _ => {}
        }
    }
    if let Some((primaries, transfer)) = cicp {
        return match (primaries, transfer) {
            (1, 13) => ColourSpace::Srgb,
            (12, 13) => ColourSpace::DisplayP3,
            (_, 8) => ColourSpace::Other(format!("linear light (cICP primaries {primaries})")),
            _ => ColourSpace::Other(format!("cICP primaries {primaries}, transfer {transfer}")),
        };
    }
    if let Some(data) = iccp {
        return icc_colour_space(data);
    }
    if srgb {
        return ColourSpace::Srgb;
    }
    // A file gamma near 1 means the samples are linear light. Other gAMA
    // values are taken as an approximation of the sRGB curve.
    if gama.is_some_and(|g| (g - 1.0).abs() < 0.1) {
        return ColourSpace::Other("linear light (gAMA 1.0)".into());
    }
    if let Some(data) = chrm {
        let values: Vec<f64> = data
            .chunks(4)
            .map(|v| u32::from_be_bytes(v.try_into().unwrap()) as f64 / 100_000.0)
            .collect();
        let close = |reference: [f64; 8]| {
            values
                .iter()
                .zip(reference)
                .all(|(a, b)| (a - b).abs() < 0.01)
        };
        return if close(SRGB_CHROMATICITIES) {
            ColourSpace::Srgb
        } else if close(P3_CHROMATICITIES) {
            ColourSpace::DisplayP3
        } else {
            ColourSpace::Other("cHRM primaries other than sRGB or Display P3".into())
        };
    }
    ColourSpace::Unspecified
}

/// Classifies an `iCCP` chunk by the primaries of its RGB profile.
fn icc_colour_space(chunk: &[u8]) -> ColourSpace {
    let Some(name_end) = chunk.iter().position(|&b| b == 0) else {
        return ColourSpace::Other("unreadable ICC profile".into());
    };
    let keyword = String::from_utf8_lossy(&chunk[..name_end]).into_owned();
    let compressed = chunk.get(name_end + 2..).unwrap_or_default();
    let Ok(profile) = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(compressed, 1 << 22)
    else {
        return ColourSpace::Other(format!("unreadable ICC profile \"{keyword}\""));
    };
    let name = icc_description(&profile).unwrap_or(keyword);
    if profile.get(16..20) != Some(b"RGB ") {
        return ColourSpace::Other(format!("ICC profile \"{name}\" (not RGB)"));
    }
    if icc_gamma(&profile).is_some_and(|g| (g - 1.0).abs() < 0.1) {
        return ColourSpace::Other(format!("linear light (ICC profile \"{name}\")"));
    }
    let primaries = [b"rXYZ", b"gXYZ", b"bXYZ"].map(|tag| icc_xyz(&profile, tag));
    let matches = |reference: [[f64; 3]; 3]| {
        primaries.iter().zip(reference).all(|(found, expected)| {
            found.is_some_and(|xyz| xyz.iter().zip(expected).all(|(a, b)| (a - b).abs() < 0.01))
        })
    };
    if matches(SRGB_ICC_PRIMARIES) {
        ColourSpace::Srgb
    } else if matches(P3_ICC_PRIMARIES) {
        ColourSpace::DisplayP3
    } else {
        ColourSpace::Other(format!("ICC profile \"{name}\""))
    }
}

/// The exponent of the red tone curve when it is a pure power function
/// (`curv` with 0 or 1 entries, or `para` type 0); `None` for anything else,
/// such as the sRGB curve.
fn icc_gamma(profile: &[u8]) -> Option<f64> {
    let data = icc_tag(profile, b"rTRC")?;
    let read_u32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
    };
    match data.get(..4)? {
        b"curv" => match read_u32(8)? {
            0 => Some(1.0),
            1 => Some(u16::from_be_bytes(data.get(12..14)?.try_into().ok()?) as f64 / 256.0),
            _ => None,
        },
        b"para" if data.get(8..10)? == [0, 0] => Some(read_u32(12)? as i32 as f64 / 65536.0),
        _ => None,
    }
}

/// Finds a tag in an ICC profile and returns its data.
fn icc_tag<'a>(profile: &'a [u8], signature: &[u8; 4]) -> Option<&'a [u8]> {
    let read_u32 = |at: usize| -> Option<usize> {
        Some(u32::from_be_bytes(profile.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    let count = read_u32(128)?;
    for i in 0..count.min(1024) {
        let entry = 132 + i * 12;
        if profile.get(entry..entry + 4)? == signature {
            let (offset, size) = (read_u32(entry + 4)?, read_u32(entry + 8)?);
            return profile.get(offset..offset.checked_add(size)?);
        }
    }
    None
}

/// Reads an XYZType tag (one s15Fixed16 triple).
fn icc_xyz(profile: &[u8], signature: &[u8; 4]) -> Option<[f64; 3]> {
    let data = icc_tag(profile, signature)?;
    if data.get(..4)? != b"XYZ " {
        return None;
    }
    let fixed = |at: usize| -> Option<f64> {
        Some(i32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as f64 / 65536.0)
    };
    Some([fixed(8)?, fixed(12)?, fixed(16)?])
}

/// Reads the profile description (`desc`, as ICC v2 text or v4 `mluc`).
fn icc_description(profile: &[u8]) -> Option<String> {
    let data = icc_tag(profile, b"desc")?;
    let read_u32 = |at: usize| -> Option<usize> {
        Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    let text = match data.get(..4)? {
        b"desc" => {
            let len = read_u32(8)?;
            String::from_utf8_lossy(data.get(12..12 + len)?).into_owned()
        }
        b"mluc" => {
            let (len, offset) = (read_u32(20)?, read_u32(24)?);
            let units: Vec<u16> = data
                .get(offset..offset + len)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_be_bytes(pair))
                .collect();
            String::from_utf16_lossy(&units)
        }
        _ => return None,
    };
    let text = text.trim_end_matches('\0').trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{write_rgba8, RgbaImage};

    /// A PNG with `extra` chunks inserted right after IHDR.
    fn png_with(extra: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let base = write_rgba8(&RgbaImage::filled(1, 1, [1, 2, 3, 255])).unwrap();
        // Drop the sRGB chunk the writer adds, then insert ours after IHDR.
        let mut out = base[..33].to_vec();
        for (kind, data) in extra {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut crc = crc32fast::Hasher::new();
            crc.update(*kind);
            crc.update(data);
            out.extend_from_slice(*kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&crc.finalize().to_be_bytes());
        }
        let mut pos = 33;
        while pos < base.len() {
            let len = u32::from_be_bytes(base[pos..pos + 4].try_into().unwrap()) as usize;
            if &base[pos + 4..pos + 8] != b"sRGB" {
                out.extend_from_slice(&base[pos..pos + 12 + len]);
            }
            pos += 12 + len;
        }
        out
    }

    /// A minimal ICC v2 profile with the given primaries and description.
    fn icc(primaries: [[f64; 3]; 3], description: &str) -> Vec<u8> {
        let xyz = |v: [f64; 3]| {
            let mut t = b"XYZ \0\0\0\0".to_vec();
            for c in v {
                t.extend_from_slice(&((c * 65536.0).round() as i32).to_be_bytes());
            }
            t
        };
        let mut desc = b"desc\0\0\0\0".to_vec();
        desc.extend_from_slice(&(description.len() as u32 + 1).to_be_bytes());
        desc.extend_from_slice(description.as_bytes());
        desc.push(0);
        let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"desc", desc),
            (b"rXYZ", xyz(primaries[0])),
            (b"gXYZ", xyz(primaries[1])),
            (b"bXYZ", xyz(primaries[2])),
        ];
        let mut header = vec![0u8; 128];
        header[12..16].copy_from_slice(b"mntr");
        header[16..20].copy_from_slice(b"RGB ");
        let mut table = (tags.len() as u32).to_be_bytes().to_vec();
        let mut body = Vec::new();
        let mut offset = 128 + 4 + 12 * tags.len();
        for (sig, data) in &tags {
            table.extend_from_slice(*sig);
            table.extend_from_slice(&(offset as u32).to_be_bytes());
            table.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            offset += data.len();
        }
        [header, table, body].concat()
    }

    fn iccp(profile: &[u8]) -> Vec<u8> {
        let mut chunk = b"Profile\0\0".to_vec();
        chunk.extend(miniz_oxide::deflate::compress_to_vec_zlib(profile, 6));
        chunk
    }

    /// The same profile as [`icc`] with a pure power tone curve.
    fn icc_with_gamma(primaries: [[f64; 3]; 3], gamma: f64) -> Vec<u8> {
        let mut profile = icc(primaries, "Linear");
        let count = u32::from_be_bytes(profile[128..132].try_into().unwrap());
        let mut curve = b"curv\0\0\0\0".to_vec();
        curve.extend_from_slice(&1u32.to_be_bytes());
        curve.extend_from_slice(&((gamma * 256.0) as u16).to_be_bytes());
        // Append the tag at the end and add it to the table (which is
        // followed directly by tag data, so rebuild the table in place).
        let offset = profile.len() as u32 + 12;
        let mut table = (count + 1).to_be_bytes().to_vec();
        table.extend_from_slice(&profile[132..132 + 12 * count as usize]);
        for entry in table[4..].chunks_mut(12) {
            let shifted = u32::from_be_bytes(entry[4..8].try_into().unwrap()) + 12;
            entry[4..8].copy_from_slice(&shifted.to_be_bytes());
        }
        table.extend_from_slice(b"rTRC");
        table.extend_from_slice(&offset.to_be_bytes());
        table.extend_from_slice(&(curve.len() as u32).to_be_bytes());
        let body = profile.split_off(132 + 12 * count as usize);
        profile.truncate(128);
        [profile, table, body, curve].concat()
    }

    #[test]
    fn detects_declared_colour_spaces() {
        let p3 = P3_ICC_PRIMARIES;
        type Chunks = Vec<(&'static [u8; 4], Vec<u8>)>;
        let linear = |what: &str| ColourSpace::Other(format!("linear light ({what})"));
        let cases: [(&str, Chunks, ColourSpace); 12] = [
            ("none", vec![], ColourSpace::Unspecified),
            ("sRGB chunk", vec![(b"sRGB", vec![0])], ColourSpace::Srgb),
            (
                "cICP sRGB",
                vec![(b"cICP", vec![1, 13, 0, 1])],
                ColourSpace::Srgb,
            ),
            (
                "cICP P3",
                vec![(b"cICP", vec![12, 13, 0, 1])],
                ColourSpace::DisplayP3,
            ),
            (
                "cICP BT.2020 PQ",
                vec![(b"cICP", vec![9, 16, 0, 1])],
                ColourSpace::Other("cICP primaries 9, transfer 16".into()),
            ),
            (
                "cICP linear",
                vec![(b"cICP", vec![1, 8, 0, 1])],
                linear("cICP primaries 1"),
            ),
            (
                "sRGB ICC",
                vec![(b"iCCP", iccp(&icc(SRGB_ICC_PRIMARIES, "sRGB IEC61966-2.1")))],
                ColourSpace::Srgb,
            ),
            (
                "P3 ICC",
                vec![(b"iCCP", iccp(&icc(p3, "Display P3")))],
                ColourSpace::DisplayP3,
            ),
            (
                "our P3 ICC",
                vec![(b"iCCP", iccp(&display_p3_icc()))],
                ColourSpace::DisplayP3,
            ),
            (
                "linear ICC",
                vec![(b"iCCP", iccp(&icc_with_gamma(SRGB_ICC_PRIMARIES, 1.0)))],
                linear("ICC profile \"Linear\""),
            ),
            (
                "gamma 2.2 ICC",
                vec![(b"iCCP", iccp(&icc_with_gamma(SRGB_ICC_PRIMARIES, 2.2)))],
                ColourSpace::Srgb,
            ),
            (
                "gAMA 1.0",
                vec![(b"gAMA", 100_000u32.to_be_bytes().to_vec())],
                linear("gAMA 1.0"),
            ),
        ];
        for (name, chunks, expected) in cases {
            assert_eq!(colour_space(&png_with(&chunks)), expected, "{name}");
        }
    }

    #[test]
    fn our_own_files_are_srgb() {
        let png = write_rgba8(&RgbaImage::filled(2, 2, [9, 9, 9, 9])).unwrap();
        assert_eq!(colour_space(&png), ColourSpace::Srgb);
    }

    #[test]
    fn our_display_p3_files_are_display_p3() {
        let img = RgbaImage::filled(2, 2, [9, 9, 9, 9]);
        let png = crate::write_rgba8_in(&img, RgbSpace::DisplayP3).unwrap();
        assert_eq!(colour_space(&png), ColourSpace::DisplayP3);
        assert_eq!(crate::read_rgba8(&png).unwrap(), img);
    }

    /// Reference values from Chromium and littlecms.
    #[test]
    fn converts_between_srgb_and_display_p3() {
        let mut px = vec![
            255, 0, 0, 200, 50, 50, 128, 128, 128, 0, 0, 0, 255, 255, 255,
        ];
        convert_pixels(&mut px, 3, RgbSpace::Srgb, RgbSpace::DisplayP3);
        assert_eq!(
            px,
            [234, 51, 35, 184, 63, 57, 128, 128, 128, 0, 0, 0, 255, 255, 255]
        );
        convert_pixels(&mut px, 3, RgbSpace::DisplayP3, RgbSpace::Srgb);
        assert_eq!(
            px,
            [255, 0, 0, 200, 50, 50, 128, 128, 128, 0, 0, 0, 255, 255, 255]
        );
        // Pure P3 red is outside sRGB and clips to sRGB red.
        let mut red = vec![255, 0, 0, 77];
        convert_pixels(&mut red, 4, RgbSpace::DisplayP3, RgbSpace::Srgb);
        assert_eq!(red, [255, 0, 0, 77]);
    }

    /// 8-bit Display P3 cannot hold every sRGB colour exactly. Bright
    /// channels come back within 2 levels; dark channels of saturated colours
    /// can move more, where the sRGB curve is steep and the change is hard to
    /// see.
    #[test]
    fn srgb_round_trips_through_display_p3_closely() {
        let mut original = Vec::new();
        for r in (0..=255u8).step_by(5) {
            for g in (0..=255u8).step_by(5) {
                for b in (0..=255u8).step_by(5) {
                    original.extend_from_slice(&[r, g, b]);
                }
            }
        }
        let mut px = original.clone();
        convert_pixels(&mut px, 3, RgbSpace::Srgb, RgbSpace::DisplayP3);
        convert_pixels(&mut px, 3, RgbSpace::DisplayP3, RgbSpace::Srgb);
        for (&a, &b) in px.iter().zip(&original) {
            let limit = if a.max(b) > 64 { 2 } else { 8 };
            assert!(a.abs_diff(b) <= limit, "{b} came back as {a}");
        }
    }

    #[test]
    fn widest_space_holds_all() {
        use RgbSpace::*;
        assert_eq!(RgbSpace::widest([Srgb, Srgb]), Srgb);
        assert_eq!(RgbSpace::widest([Srgb, DisplayP3]), DisplayP3);
        assert_eq!(RgbSpace::widest([]), Srgb);
    }
}
