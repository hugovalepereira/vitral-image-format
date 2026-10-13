use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use vitral::{BlendMode, Fidelity, Layer, RgbaImage, VitralImage, VitralStatus};

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
    /// black background and one over a white background.
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
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Turn a regular PNG into a Vitral image that always blends with the
    /// given mode (e.g. multiply, screen, hard-light, difference).
    FromImage {
        input: PathBuf,
        #[arg(long, default_value = "normal")]
        mode: BlendMode,
        #[arg(long, default_value_t = 1.0)]
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
            output,
        } => {
            let mut img = VitralImage::from_renders(&read_png(&black)?, &read_png(&white)?)?;
            match transparent {
                Some(path) => img = img.with_native(read_png(&path)?)?,
                None => eprintln!(
                    "note: no --transparent export given; the look over transparent areas \
                     and in software without Vitral support is approximated"
                ),
            }
            write(&output, &vitral::encode(&img)?)
        }
        Command::FromImage {
            input,
            mode,
            opacity,
            force,
            output,
        } => {
            let image = read_png(&input)?;
            let (w, h) = (image.width, image.height);
            check_fidelity(mode, force)?;
            let layer = Layer::new(image, mode).with_opacity(opacity);
            write(
                &output,
                &vitral::encode(&VitralImage::from_layers(w, h, &[layer])?)?,
            )
        }
        Command::Compose {
            layers,
            force,
            output,
        } => {
            let layers = layers
                .iter()
                .map(|spec| parse_layer(spec))
                .collect::<Result<Vec<_>>>()?;
            for layer in &layers {
                check_fidelity(layer.mode, force)?;
            }
            let (w, h) = (layers[0].image.width, layers[0].image.height);
            write(
                &output,
                &vitral::encode(&VitralImage::from_layers(w, h, &layers)?)?,
            )
        }
        Command::Decode { input, out_dir } => {
            let decoded = vitral::decode(&fs::read(&input)?)?;
            report_status(&decoded.status);
            let img = decoded.image();
            fs::create_dir_all(&out_dir)?;
            let planes = img.render_planes();
            let rgba = |data: Vec<u8>| RgbaImage::new(img.width, img.height, data);
            let files: [(&str, Vec<u8>); 6] = [
                (
                    "black.png",
                    vitral::write_rgb8(img.width, img.height, &img.black)?,
                ),
                (
                    "white.png",
                    vitral::write_rgb8(img.width, img.height, &img.white)?,
                ),
                ("native.png", vitral::write_rgba8(&img.native)?),
                (
                    "plane-1-difference.png",
                    vitral::write_rgba8(&rgba(planes.difference)?)?,
                ),
                (
                    "plane-2-screen.png",
                    vitral::write_rgba8(&rgba(planes.screen)?)?,
                ),
                (
                    "plane-3-multiply.png",
                    vitral::write_rgba8(&rgba(planes.multiply)?)?,
                ),
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
            output,
        } => {
            let decoded = vitral::decode(&fs::read(&input)?)?;
            report_status(&decoded.status);
            let img = if legacy {
                // What a plain PNG viewer shows: the fallback, normally blended.
                VitralImage::from_layers(
                    decoded.fallback.width,
                    decoded.fallback.height,
                    &[Layer::new(decoded.fallback.clone(), BlendMode::Normal)],
                )?
            } else {
                decoded.image()
            };
            let mut canvas = match (backdrop, color) {
                (Some(path), _) => read_png(&path)?,
                (None, Some(hex)) => {
                    let [r, g, b] = parse_hex(&hex)?;
                    RgbaImage::filled(img.width, img.height, [r, g, b, 255])
                }
                (None, None) => return Err("give --backdrop FILE or --color #rrggbb".into()),
            };
            img.composite_onto(&mut canvas, x, y);
            write(&output, &vitral::write_rgba8(&canvas)?)
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
fn parse_layer(spec: &str) -> Result<Layer> {
    let err = || format!("layer '{spec}' must be FILE:MODE[:OPACITY]");
    let (rest, last) = spec.rsplit_once(':').ok_or_else(err)?;
    let (file, mode, opacity) = match last.parse::<f32>() {
        Ok(opacity) => {
            let (file, mode) = rest.rsplit_once(':').ok_or_else(err)?;
            (file, mode, opacity)
        }
        Err(_) => (rest, last, 1.0),
    };
    Ok(Layer::new(read_png(Path::new(file))?, mode.parse::<BlendMode>()?).with_opacity(opacity))
}

fn parse_hex(hex: &str) -> Result<[u8; 3]> {
    let digits = hex.trim_start_matches('#');
    if digits.len() != 6 {
        return Err(format!("colour '{hex}' must be #rrggbb").into());
    }
    let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16);
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

fn read_png(path: &Path) -> Result<RgbaImage> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
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
        VitralStatus::NotVitral => "plain PNG (no vITR chunk)".into(),
        VitralStatus::FallbackMismatch => {
            "main image was modified after encoding; Vitral data ignored".into()
        }
        VitralStatus::Unsupported(what) => format!("unsupported: {what}; Vitral data ignored"),
        VitralStatus::Corrupt(what) => format!("corrupt vITR chunk: {what}; Vitral data ignored"),
    }
}

fn report_status(status: &VitralStatus) {
    if *status != VitralStatus::Ok {
        eprintln!("note: {}", status_text(status));
    }
}
