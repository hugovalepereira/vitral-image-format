//! WebAssembly bindings for the Vitral reference implementation, used by the
//! web polyfill and by design-tool plugins.

use wasm_bindgen::prelude::*;

use vitral::{RenderPlanes, RgbaImage, VitralImage, VitralStatus};

/// The three blend-mode layers of a decoded file, ready to be drawn into
/// canvases stacked as difference → screen → multiply.
#[wasm_bindgen]
pub struct Planes {
    image: VitralImage,
    planes: RenderPlanes,
    status: String,
}

fn opaque_rgba(rgb: &[u8]) -> Vec<u8> {
    rgb.as_chunks::<3>()
        .0
        .iter()
        .flat_map(|&[r, g, b]| [r, g, b, 255])
        .collect()
}

#[wasm_bindgen]
impl Planes {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.planes.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.planes.height
    }

    /// `"ok"`, `"not-vitral"`, `"fallback-mismatch"`, `"unsupported: …"` or
    /// `"corrupt: …"`. Anything but `"ok"` means: show the plain image.
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> String {
        self.status.clone()
    }

    #[wasm_bindgen(getter, js_name = isVitral)]
    pub fn is_vitral(&self) -> bool {
        self.status == "ok"
    }

    #[wasm_bindgen(getter, js_name = usesDifference)]
    pub fn uses_difference(&self) -> bool {
        self.planes.uses_difference
    }

    /// RGBA8 pixels of the bottom layer (blend with `difference`).
    pub fn difference(&self) -> Vec<u8> {
        self.planes.difference.clone()
    }

    /// RGBA8 pixels of the middle layer (blend with `screen`).
    pub fn screen(&self) -> Vec<u8> {
        self.planes.screen.clone()
    }

    /// RGBA8 pixels of the top layer (blend with `multiply`).
    pub fn multiply(&self) -> Vec<u8> {
        self.planes.multiply.clone()
    }

    /// RGBA8 pixels of the image as seen over black (`K`).
    #[wasm_bindgen(js_name = overBlack)]
    pub fn over_black(&self) -> Vec<u8> {
        opaque_rgba(&self.image.black)
    }

    /// RGBA8 pixels of the image as seen over white (`W`).
    #[wasm_bindgen(js_name = overWhite)]
    pub fn over_white(&self) -> Vec<u8> {
        opaque_rgba(&self.image.white)
    }

    /// RGBA8 pixels of the image's native look, with nothing behind it.
    pub fn native(&self) -> Vec<u8> {
        self.image.native.data.clone()
    }
}

fn status_code(status: &VitralStatus) -> String {
    match status {
        VitralStatus::Ok => "ok".into(),
        VitralStatus::NotVitral => "not-vitral".into(),
        VitralStatus::FallbackMismatch => "fallback-mismatch".into(),
        VitralStatus::Unsupported(what) => format!("unsupported: {what}"),
        VitralStatus::Corrupt(what) => format!("corrupt: {what}"),
    }
}

fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// Decodes a file and returns its render planes. Plain PNGs decode too (with
/// `isVitral == false`), so callers can leave them untouched.
#[wasm_bindgen(js_name = decodePlanes)]
pub fn decode_planes(bytes: &[u8]) -> Result<Planes, JsError> {
    let decoded = vitral::decode(bytes).map_err(js_err)?;
    let image = decoded.image();
    Ok(Planes {
        planes: image.render_planes(),
        image,
        status: status_code(&decoded.status),
    })
}

/// Encodes a Vitral file from RGBA8 renders of the same artwork: over a black
/// background, over a white background and, optionally, over a transparent
/// background (its native look; approximated when omitted).
#[wasm_bindgen(js_name = encodeFromRenders)]
pub fn encode_from_renders(
    width: u32,
    height: u32,
    over_black: Vec<u8>,
    over_white: Vec<u8>,
    over_transparent: Option<Vec<u8>>,
) -> Result<Vec<u8>, JsError> {
    let black = RgbaImage::new(width, height, over_black).map_err(js_err)?;
    let white = RgbaImage::new(width, height, over_white).map_err(js_err)?;
    let mut img = VitralImage::from_renders(&black, &white).map_err(js_err)?;
    if let Some(pixels) = over_transparent {
        let native = RgbaImage::new(width, height, pixels).map_err(js_err)?;
        img = img.with_native(native).map_err(js_err)?;
    }
    vitral::encode(&img).map_err(js_err)
}

/// Reference composite of a file over an RGBA8 backdrop (which may be partly
/// transparent) with the image's top-left corner at `(x, y)`. Returns the new
/// backdrop pixels.
#[wasm_bindgen]
pub fn composite(
    bytes: &[u8],
    backdrop: Vec<u8>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
) -> Result<Vec<u8>, JsError> {
    let decoded = vitral::decode(bytes).map_err(js_err)?;
    let mut canvas = RgbaImage::new(width, height, backdrop).map_err(js_err)?;
    decoded
        .image()
        .composite_onto(&mut canvas, x as i64, y as i64);
    Ok(canvas.data)
}
