//! Detecting the colour space a PNG declares. Vitral 0.1 blends sRGB-encoded
//! values, so encoders must not take, say, Display P3 samples as sRGB.

use crate::pngio::chunks;

/// The colour space a PNG declares for its samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColourSpace {
    /// Declared sRGB: an `sRGB` chunk, an ICC profile with sRGB primaries, or
    /// `cICP` with BT.709 primaries and the sRGB transfer function.
    Srgb,
    /// No colour information, which by convention means sRGB.
    Unspecified,
    /// Another colour space, described for people (e.g. `ICC profile "Display P3"`).
    Other(String),
}

impl ColourSpace {
    /// Whether the samples can be used as sRGB values.
    pub fn is_srgb(&self) -> bool {
        !matches!(self, ColourSpace::Other(_))
    }
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
    let (mut cicp, mut iccp, mut srgb, mut chrm) = (None, None, false, None);
    for (kind, data, crc_ok) in chunks(png) {
        if !crc_ok {
            continue;
        }
        match &kind {
            b"cICP" if data.len() >= 2 => cicp = Some((data[0], data[1])),
            b"iCCP" => iccp = Some(data),
            b"sRGB" => srgb = true,
            b"cHRM" if data.len() == 32 => chrm = Some(data),
            b"IDAT" => break,
            _ => {}
        }
    }
    if let Some((primaries, transfer)) = cicp {
        return if (primaries, transfer) == (1, 13) {
            ColourSpace::Srgb
        } else {
            ColourSpace::Other(format!("cICP primaries {primaries}, transfer {transfer}"))
        };
    }
    if let Some(data) = iccp {
        return icc_colour_space(data);
    }
    if srgb {
        return ColourSpace::Srgb;
    }
    if let Some(data) = chrm {
        let values: Vec<f64> = data
            .chunks(4)
            .map(|v| u32::from_be_bytes(v.try_into().unwrap()) as f64 / 100_000.0)
            .collect();
        let close = values
            .iter()
            .zip(SRGB_CHROMATICITIES)
            .all(|(a, b)| (a - b).abs() < 0.01);
        return if close {
            ColourSpace::Srgb
        } else {
            ColourSpace::Other("cHRM primaries other than sRGB".into())
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
    let primaries = [b"rXYZ", b"gXYZ", b"bXYZ"].map(|tag| icc_xyz(&profile, tag));
    let matches = primaries
        .iter()
        .zip(SRGB_ICC_PRIMARIES)
        .all(|(found, srgb)| {
            found.is_some_and(|xyz| xyz.iter().zip(srgb).all(|(a, b)| (a - b).abs() < 0.01))
        });
    if matches {
        ColourSpace::Srgb
    } else {
        ColourSpace::Other(format!("ICC profile \"{name}\""))
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
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
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

    #[test]
    fn detects_declared_colour_spaces() {
        let p3 = [
            [0.5151, 0.2412, -0.0011],
            [0.2920, 0.6922, 0.0419],
            [0.1571, 0.0666, 0.7841],
        ];
        let cases: [(&str, Vec<(&[u8; 4], Vec<u8>)>, ColourSpace); 6] = [
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
                ColourSpace::Other("cICP primaries 12, transfer 13".into()),
            ),
            (
                "sRGB ICC",
                vec![(b"iCCP", iccp(&icc(SRGB_ICC_PRIMARIES, "sRGB IEC61966-2.1")))],
                ColourSpace::Srgb,
            ),
            (
                "P3 ICC",
                vec![(b"iCCP", iccp(&icc(p3, "Display P3")))],
                ColourSpace::Other("ICC profile \"Display P3\"".into()),
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
}
