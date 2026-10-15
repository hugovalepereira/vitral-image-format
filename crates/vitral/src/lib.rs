//! # Vitral
//!
//! Reference implementation of the Vitral image format: an image that carries,
//! inside the file, how it reacts to whatever is behind it.
//!
//! Every pixel channel of a Vitral image is a function of the backdrop value
//! `b` (all values normalised to `[0, 1]`):
//!
//! ```text
//! result = K + (W - K) * b
//! ```
//!
//! where `K` is how the pixel looks over **black** and `W` how it looks over
//! **white**. This captures normal, multiply, screen, hard-light, exclusion and
//! inversion exactly — per pixel, with opacity, and for any stack of them.
//!
//! Where there is nothing behind the image, each pixel shows its own native
//! colour, as a layer does in a design tool.
//!
//! A Vitral file is a valid PNG: the main image is that native appearance,
//! which software without Vitral support shows as a fallback, and an ancillary
//! `viTr` chunk carries the residual needed to recover `K` and `W` exactly. See
//! `SPEC.md` for the normative text.

mod blend;
mod codec;
mod colour;
mod error;
mod fallback;
mod math;
mod model;
mod pngio;

pub use blend::{BlendMode, Fidelity, Layer};
pub use codec::{decode, encode, Decoded, VitralStatus, CHUNK_TYPE};
pub use colour::{colour_space, ColourSpace};
pub use error::Error;
pub use fallback::least_squares_native;
pub use model::{RenderPlanes, VitralImage};
pub use pngio::{read_rgba8, write_rgb8, write_rgba8, RgbaImage};
