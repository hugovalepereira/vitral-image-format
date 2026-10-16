//! WebAssembly bindings for the Vitral reference implementation, used by the
//! web polyfill and by design-tool plugins.

use wasm_bindgen::prelude::*;

use vitral::{RenderPlanes, RgbSpace, RgbaImage, VitralImage, VitralStatus};

/// The three blend-mode layers of a decoded file, ready to be drawn into
/// canvases stacked as difference → screen → multiply.
///
/// The layers are in sRGB, the colour space CSS blends in by default: a
/// Display P3 file is converted first, which is exact over black and white
/// backdrops and close elsewhere (SPEC §7.2). K, W and the native look are
/// returned in the file's own colour space (see `colourSpace`).
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

    /// `"srgb"` or `"display-p3"`: the colour space of the file, and of
    /// `overBlack`, `overWhite` and `native`.
    #[wasm_bindgen(getter, js_name = colourSpace)]
    pub fn colour_space(&self) -> String {
        self.image.colour_space.css_name().into()
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
        planes: image.to_colour_space(RgbSpace::Srgb).render_planes(),
        image,
        status: status_code(&decoded.status),
    })
}

/// The three blend-mode layers of a file as PNG files, for design tools that
/// place them as image layers (difference → screen → multiply) and blend in
/// their document's colour space, as Figma does.
#[wasm_bindgen]
pub struct LayerPngs {
    width: u32,
    height: u32,
    status: String,
    uses_difference: bool,
    difference: Vec<u8>,
    screen: Vec<u8>,
    multiply: Vec<u8>,
}

#[wasm_bindgen]
impl LayerPngs {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Same codes as `Planes.status`. Anything but `"ok"` means: place the
    /// file as a plain image; the layers are then empty.
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> String {
        self.status.clone()
    }

    /// False when the bottom (difference) layer would be black everywhere and
    /// can be left out.
    #[wasm_bindgen(getter, js_name = usesDifference)]
    pub fn uses_difference(&self) -> bool {
        self.uses_difference
    }

    pub fn difference(&self) -> Vec<u8> {
        self.difference.clone()
    }

    pub fn screen(&self) -> Vec<u8> {
        self.screen.clone()
    }

    pub fn multiply(&self) -> Vec<u8> {
        self.multiply.clone()
    }
}

/// Decodes a file into its three layers as PNG files in `colour_space`
/// (`"srgb"`, the default, or `"display-p3"`), tagged with it. Converting a
/// file to another space than its own is exact over black and white and close
/// elsewhere (SPEC §7.2).
#[wasm_bindgen(js_name = decodeLayerPngs)]
pub fn decode_layer_pngs(bytes: &[u8], colour_space: Option<String>) -> Result<LayerPngs, JsError> {
    let space = parse_space(colour_space)?;
    let decoded = vitral::decode(bytes).map_err(js_err)?;
    let status = status_code(&decoded.status);
    let Some(image) = decoded.vitral.filter(|_| status == "ok") else {
        return Ok(LayerPngs {
            width: decoded.fallback.width,
            height: decoded.fallback.height,
            status,
            uses_difference: false,
            difference: Vec::new(),
            screen: Vec::new(),
            multiply: Vec::new(),
        });
    };
    let planes = image.to_colour_space(space).render_planes();
    let png = |data: Vec<u8>| {
        let layer = RgbaImage::new(planes.width, planes.height, data).map_err(js_err)?;
        vitral::write_rgba8_in(&layer, space).map_err(js_err)
    };
    Ok(LayerPngs {
        width: planes.width,
        height: planes.height,
        status,
        uses_difference: planes.uses_difference,
        difference: png(planes.difference.clone())?,
        screen: png(planes.screen.clone())?,
        multiply: png(planes.multiply.clone())?,
    })
}

fn parse_space(name: Option<String>) -> Result<RgbSpace, JsError> {
    match name.as_deref() {
        None | Some("srgb") => Ok(RgbSpace::Srgb),
        Some("display-p3") => Ok(RgbSpace::DisplayP3),
        Some(other) => Err(JsError::new(&format!(
            "colour space '{other}' must be srgb or display-p3"
        ))),
    }
}

/// Encodes a Vitral file from RGBA8 renders of the same artwork: over a black
/// background, over a white background and, optionally, over a transparent
/// background (its native look; approximated when omitted). The pixels are in
/// `colour_space` (`"srgb"`, the default, or `"display-p3"`).
#[wasm_bindgen(js_name = encodeFromRenders)]
pub fn encode_from_renders(
    width: u32,
    height: u32,
    over_black: Vec<u8>,
    over_white: Vec<u8>,
    over_transparent: Option<Vec<u8>>,
    colour_space: Option<String>,
) -> Result<Vec<u8>, JsError> {
    let space = parse_space(colour_space)?;
    let black = RgbaImage::new(width, height, over_black).map_err(js_err)?;
    let white = RgbaImage::new(width, height, over_white).map_err(js_err)?;
    let mut img = VitralImage::from_renders(&black, &white)
        .map_err(js_err)?
        .in_colour_space(space);
    if let Some(pixels) = over_transparent {
        let native = RgbaImage::new(width, height, pixels).map_err(js_err)?;
        img = img.with_native(native).map_err(js_err)?;
    }
    vitral::encode(&img).map_err(js_err)
}

/// Encodes a Vitral file from the PNG files a design tool exported: the
/// artwork over black, over white and, optionally, over nothing. Exports may
/// be sRGB or Display P3, and the file keeps their colour space; this is the
/// path to use for exports from Figma.
#[wasm_bindgen(js_name = encodeFromExports)]
pub fn encode_from_exports(
    over_black: &[u8],
    over_white: &[u8],
    over_transparent: Option<Vec<u8>>,
) -> Result<Vec<u8>, JsError> {
    let img = VitralImage::from_exports(over_black, over_white, over_transparent.as_deref())
        .map_err(js_err)?;
    vitral::encode(&img).map_err(js_err)
}

/// Reference composite of a file over an RGBA8 sRGB backdrop (which may be
/// partly transparent) with the image's top-left corner at `(x, y)`, blending
/// in sRGB as CSS does. Returns the new backdrop pixels.
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
        .to_colour_space(RgbSpace::Srgb)
        .composite_onto(&mut canvas, x as i64, y as i64);
    Ok(canvas.data)
}
