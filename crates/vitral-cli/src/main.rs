use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use vitral::{
    convert_rgba, BlendMode, ColourSpace, Fidelity, Layer, RgbSpace, RgbaImage, VitralImage,
    VitralStatus,
};

/// Create, inspect and render Vitral images (.vitral.png): images that carry
/// their own blend behaviour.
#[derive(Parser)]
#[command(name = "vitral", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a Vitral image from two exports of the same artwork, one over a
    /// black background and one over a white background. Exports may be sRGB
    /// or Display P3; the file keeps their colour space.
    Encode {
        #[arg(long)]
        black: PathBuf,
        #[arg(long)]
        white: PathBuf,
        /// The artwork exported over a transparent background: how it looks
        /// with nothing behind it, and what software without Vitral support
        /// shows. If omitted, an approximation is computed.
        #[arg(long)]
        transparent: Option<PathBuf>,
        /// Use the samples as sRGB even if a file declares a colour space
        /// Vitral does not take (e.g. Adobe RGB or linear light). Colours and
        /// blending will be off.
        #[arg(long)]
        assume_srgb: bool,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Turn a regular PNG into a Vitral image that always blends with the
    /// given mode (e.g. multiply, screen, hard-light, difference).
    FromImage {
        input: PathBuf,
        #[arg(long, default_value = "normal")]
        mode: BlendMode,
        /// Layer opacity, from 0 to 1.
        #[arg(long, default_value_t = 1.0, value_parser = parse_opacity)]
        opacity: f32,
        /// Proceed even with modes this version cannot represent.
        #[arg(long)]
        force: bool,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Flatten a stack of layers (bottom first) into one Vitral image. Each
    /// layer is FILE:MODE or FILE:MODE:OPACITY.
    Compose {
        #[arg(long = "layer", required = true)]
        layers: Vec<String>,
        /// Proceed even with modes this version cannot represent.
        #[arg(long)]
        force: bool,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Write the over-black, over-white, native (over-transparent) and
    /// render-plane images.
    Decode {
        input: PathBuf,
        #[arg(long)]
        out_dir: PathBuf,
    },
    /// Composite a Vitral image over a backdrop image or colour.
    Render {
        input: PathBuf,
        /// Backdrop image (its size sets the output size).
        #[arg(long, conflicts_with = "color")]
        backdrop: Option<PathBuf>,
        /// Backdrop colour as #rrggbb (output has the Vitral image's size).
        #[arg(long)]
        color: Option<String>,
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        x: i64,
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        y: i64,
        /// Show what software without Vitral support would display.
        #[arg(long)]
        legacy: bool,
        /// Blend in this colour space instead of the image's own, as a host
        /// that blends there would (CSS usually blends in srgb).
        #[arg(long, value_parser = parse_space)]
        blend_space: Option<RgbSpace>,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Print information about a file.
    Inspect { input: PathBuf },
    /// Compare two images of the same size channel by channel (RGB). Exits
    /// with an error if any channel differs by more than the tolerance.
    Diff {
        a: PathBuf,
        b: PathBuf,
        #[arg(long, default_value_t = 0)]
        tolerance: u8,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn run(command: Command) -> Result<()> {
    match command {
        Command::Encode {
            black,
            white,
            transparent,
            assume_srgb,
            output,
        } => {
            if transparent.is_none() {
                eprintln!(
                    "note: no --transparent export given; the look over transparent areas \
                     and in software without Vitral support is approximated"
                );
            }
            let img = if assume_srgb {
                for path in [Some(&black), Some(&white), transparent.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    warn_if_not_srgb(path)?;
                }
                let mut img = VitralImage::from_renders(&read_png(&black)?, &read_png(&white)?)?;
                if let Some(path) = &transparent {
                    img = img.with_native(read_png(path)?)?;
                }
                img
            } else {
                let transparent = transparent.as_deref().map(read_bytes).transpose()?;
                VitralImage::from_exports(
                    &read_bytes(&black)?,
                    &read_bytes(&white)?,
                    transparent.as_deref(),
                )
                .map_err(|e| format!("{e}. Export in sRGB or Display P3, or pass --assume-srgb"))?
            };
            report_space(img.colour_space);
            write(&output, &vitral::encode(&img)?)
        }
        Command::FromImage {
            input,
            mode,
            opacity,
            force,
            output,
        } => {
            let (image, space) = read_input(&input)?;
            let (w, h) = (image.width, image.height);
            check_fidelity(mode, force)?;
            let layer = Layer::new(image, mode).with_opacity(opacity);
            let img = VitralImage::from_layers(w, h, &[layer])?.in_colour_space(space);
            report_space(space);
            write(&output, &vitral::encode(&img)?)
        }
        Command::Compose {
            layers,
            force,
            output,
        } => {
            let specs = layers
                .iter()
                .map(|spec| parse_layer(spec))
                .collect::<Result<Vec<_>>>()?;
            for (_, mode, _) in &specs {
                check_fidelity(*mode, force)?;
            }
            let inputs = specs
                .iter()
                .map(|(file, _, _)| read_input(Path::new(file)))
                .collect::<Result<Vec<_>>>()?;
            // Layers in different colour spaces are converted to the widest.
            let space = RgbSpace::widest(inputs.iter().map(|(_, s)| *s));
            let layers: Vec<Layer> = specs
                .iter()
                .zip(inputs)
                .map(|((_, mode, opacity), (image, from))| {
                    Layer::new(convert_rgba(&image, from, space), *mode).with_opacity(*opacity)
                })
                .collect();
            let (w, h) = (layers[0].image.width, layers[0].image.height);
            let img = VitralImage::from_layers(w, h, &layers)?.in_colour_space(space);
            report_space(space);
            write(&output, &vitral::encode(&img)?)
        }
        Command::Decode { input, out_dir } => {
            let decoded = vitral::decode(&fs::read(&input)?)?;
            report_status(&decoded.status);
            let img = decoded.image();
            fs::create_dir_all(&out_dir)?;
            // Every output is in the image's colour space: the planes are
            // meant for a host that blends in that space.
            let space = img.colour_space;
            let planes = img.render_planes();
            let rgba = |data: Vec<u8>| -> Result<Vec<u8>> {
                let layer = RgbaImage::new(img.width, img.height, data)?;
                Ok(vitral::write_rgba8_in(&layer, space)?)
            };
            let rgb = |data: &[u8]| vitral::write_rgb8_in(img.width, img.height, data, space);
            let files: [(&str, Vec<u8>); 6] = [
                ("black.png", rgb(&img.black)?),
                ("white.png", rgb(&img.white)?),
                ("native.png", vitral::write_rgba8_in(&img.native, space)?),
                ("plane-1-difference.png", rgba(planes.difference)?),
                ("plane-2-screen.png", rgba(planes.screen)?),
                ("plane-3-multiply.png", rgba(planes.multiply)?),
            ];
            for (name, bytes) in files {
                write(&out_dir.join(name), &bytes)?;
            }
            Ok(())
        }
        Command::Render {
            input,
            backdrop,
            color,
            x,
            y,
            legacy,
            blend_space,
            output,
        } => {
            let decoded = vitral::decode(&fs::read(&input)?)?;
            report_status(&decoded.status);
            let image = decoded.image();
            let img = if legacy {
                // What a plain PNG viewer shows: the fallback, normally blended.
                VitralImage::from_layers(
                    decoded.fallback.width,
                    decoded.fallback.height,
                    &[Layer::new(decoded.fallback.clone(), BlendMode::Normal)],
                )?
                .in_colour_space(image.colour_space)
            } else {
                image
            };
            let (backdrop, backdrop_space) = match (backdrop, color) {
                (Some(path), _) => read_input(&path)?,
                (None, Some(hex)) => {
                    let [r, g, b] = parse_hex(&hex)?;
                    let fill = RgbaImage::filled(img.width, img.height, [r, g, b, 255]);
                    (fill, RgbSpace::Srgb)
                }
                (None, None) => return Err("give --backdrop FILE or --color #rrggbb".into()),
            };
            // Blend in the image's colour space (or the one asked for), then
            // bring the result back to the backdrop's.
            let space = blend_space.unwrap_or(img.colour_space);
            let mut canvas = convert_rgba(&backdrop, backdrop_space, space);
            img.to_colour_space(space).composite_onto(&mut canvas, x, y);
            let result = convert_rgba(&canvas, space, backdrop_space);
            write(&output, &vitral::write_rgba8_in(&result, backdrop_space)?)
        }
        Command::Inspect { input } => {
            let bytes = fs::read(&input)?;
            let decoded = vitral::decode(&bytes)?;
            let img = decoded.image();
            let stats = img.stats();
            let total = (stats.identity + stats.normal + stats.vitral).max(1) as f64;
            let pct = |n: u64| 100.0 * n as f64 / total;
            println!("file:        {}", input.display());
            println!(
                "size:        {} x {} px, {} bytes",
                img.width,
                img.height,
                bytes.len()
            );
            println!("status:      {}", status_text(&decoded.status));
            println!("colour:      {}", space_text(&decoded.colour_space));
            println!("pixels:");
            println!("  transparent      {:6.2}%", pct(stats.identity));
            println!("  plain (PNG-like) {:6.2}%", pct(stats.normal));
            println!("  vitral-only      {:6.2}%", pct(stats.vitral));
            println!(
                "needs difference layer: {}",
                img.render_planes().uses_difference
            );
            Ok(())
        }
        Command::Diff { a, b, tolerance } => {
            let (a, b) = (read_png(&a)?, read_png(&b)?);
            if (a.width, a.height) != (b.width, b.height) {
                return Err(format!(
                    "sizes differ: {}x{} vs {}x{}",
                    a.width, a.height, b.width, b.height
                )
                .into());
            }
            let (mut max, mut sum, mut over) = (0u8, 0u64, 0u64);
            let (pixels_a, pixels_b) = (a.data.as_chunks::<4>().0, b.data.as_chunks::<4>().0);
            for (pa, pb) in pixels_a.iter().zip(pixels_b) {
                for c in 0..3 {
                    let d = pa[c].abs_diff(pb[c]);
                    max = max.max(d);
                    sum += d as u64;
                    over += (d > tolerance) as u64;
                }
            }
            let channels = (a.width as u64 * a.height as u64 * 3).max(1);
            println!(
                "max difference {max}, mean {:.4}, channels over tolerance {over}",
                sum as f64 / channels as f64
            );
            if max > tolerance {
                return Err(format!("difference {max} exceeds tolerance {tolerance}").into());
            }
            Ok(())
        }
    }
}

/// With --assume-srgb: warn about inputs that are not sRGB.
fn warn_if_not_srgb(path: &Path) -> Result<()> {
    let space = vitral::colour_space(&read_bytes(path)?);
    if !space.is_srgb() {
        eprintln!(
            "warning: {} declares {}; using its values as sRGB",
            path.display(),
            space_text(&space)
        );
    }
    Ok(())
}

/// Reads an input image and the colour space it declares; refuses colour
/// spaces Vitral cannot carry.
fn read_input(path: &Path) -> Result<(RgbaImage, RgbSpace)> {
    let bytes = read_bytes(path)?;
    Ok(vitral::read_rgba8_in_space(&bytes).map_err(|e| format!("{}: {e}", path.display()))?)
}

fn report_space(space: RgbSpace) {
    if space == RgbSpace::DisplayP3 {
        eprintln!("note: Display P3 input; the file is Display P3 and blends there");
    }
}

fn space_text(space: &ColourSpace) -> String {
    match space {
        ColourSpace::Srgb => "sRGB".into(),
        ColourSpace::DisplayP3 => "Display P3".into(),
        ColourSpace::Unspecified => "not declared (taken as sRGB)".into(),
        ColourSpace::Other(what) => what.clone(),
    }
}

fn parse_space(text: &str) -> std::result::Result<RgbSpace, String> {
    match text {
        "srgb" => Ok(RgbSpace::Srgb),
        "display-p3" => Ok(RgbSpace::DisplayP3),
        _ => Err(format!("colour space '{text}' must be srgb or display-p3")),
    }
}

fn check_fidelity(mode: BlendMode, force: bool) -> Result<()> {
    match mode.fidelity() {
        Fidelity::Exact => Ok(()),
        Fidelity::Approximate => {
            eprintln!(
                "note: '{}' is exact over black and white backdrops and linear in between",
                mode.name()
            );
            Ok(())
        }
        Fidelity::Lost if force => {
            eprintln!(
                "warning: '{}' cannot be represented in Vitral v0.1",
                mode.name()
            );
            Ok(())
        }
        Fidelity::Lost => Err(format!(
            "'{}' leaves black and white backdrops untouched, so Vitral v0.1 cannot \
             represent it (planned for v2); use --force to encode anyway",
            mode.name()
        )
        .into()),
    }
}

/// Parses FILE:MODE[:OPACITY], splitting from the right so paths may contain ':'.
fn parse_layer(spec: &str) -> Result<(String, BlendMode, f32)> {
    let err = || format!("layer '{spec}' must be FILE:MODE[:OPACITY]");
    let (rest, last) = spec.rsplit_once(':').ok_or_else(err)?;
    let (file, mode, opacity) = match last.parse::<f32>() {
        Ok(_) => {
            let (file, mode) = rest.rsplit_once(':').ok_or_else(err)?;
            (file, mode, parse_opacity(last)?)
        }
        Err(_) => (rest, last, 1.0),
    };
    Ok((file.to_string(), mode.parse::<BlendMode>()?, opacity))
}

/// Accepts a number from 0 to 1 (rejects NaN, infinities and out-of-range values).
fn parse_opacity(text: &str) -> std::result::Result<f32, String> {
    match text.trim().parse::<f32>() {
        Ok(v) if (0.0..=1.0).contains(&v) => Ok(v),
        _ => Err(format!("opacity '{text}' must be a number from 0 to 1")),
    }
}

/// Accepts #rrggbb (or rrggbb) with exactly six hexadecimal digits.
fn parse_hex(hex: &str) -> Result<[u8; 3]> {
    let digits = hex.trim_start_matches('#');
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("colour '{hex}' must be #rrggbb").into());
    }
    let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16);
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    Ok(fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
}

fn read_png(path: &Path) -> Result<RgbaImage> {
    let bytes = read_bytes(path)?;
    Ok(vitral::read_rgba8(&bytes).map_err(|e| format!("{}: {e}", path.display()))?)
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

fn status_text(status: &VitralStatus) -> String {
    match status {
        VitralStatus::Ok => "Vitral data OK".into(),
        VitralStatus::NotVitral => "plain PNG (no viTr chunk)".into(),
        VitralStatus::FallbackMismatch => {
            "main image was modified after encoding; Vitral data ignored".into()
        }
        VitralStatus::Unsupported(what) => format!("unsupported: {what}; Vitral data ignored"),
        VitralStatus::Corrupt(what) => format!("corrupt viTr chunk: {what}; Vitral data ignored"),
    }
}

fn report_status(status: &VitralStatus) {
    if *status != VitralStatus::Ok {
        eprintln!("note: {}", status_text(status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_must_be_six_hex_digits() {
        assert_eq!(parse_hex("#ff8800").unwrap(), [255, 136, 0]);
        assert_eq!(parse_hex("0a0B0c").unwrap(), [10, 11, 12]);
        for bad in ["#aéabc", "#+f+f+f", "#fff", "#ff88001", "#gg0000", ""] {
            assert!(parse_hex(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn opacity_must_be_between_zero_and_one() {
        for good in ["0", "0.5", "1", " 0.25 "] {
            assert!(parse_opacity(good).is_ok(), "{good}");
        }
        for bad in ["nan", "NaN", "inf", "-0.5", "1.5", "half"] {
            assert!(parse_opacity(bad).is_err(), "{bad}");
        }
    }
}
