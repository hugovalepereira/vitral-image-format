# Vitral

**PNG has transparency. Vitral has light and filter.**

Vitral is an image format that carries its own blend modes. A `.vitral.png` file
tints, lightens, darkens or inverts whatever is behind it, pixel by pixel, the way
multiply, screen and their relatives do in Photoshop, Illustrator or Figma, but
without anyone having to set a blend mode where the image is used.

It is also a valid PNG. Where nothing is behind it, and in software that has
never heard of Vitral, it shows each pixel's own colour: the same image a normal
PNG export would give you.

| Any PNG viewer (left) vs Vitral (right), same file over the same backdrop |
|---|
| ![Stained glass window over a busy backdrop](examples/renders/stained-glass.png) |
| ![Light leak](examples/renders/light-leak.png) |
| ![Inverting ring](examples/renders/invert-ring.png) |

> Status: **draft 0.1** (October 2026). The format, the Rust reference
> implementation, the command-line tool and the web polyfill work; everything may
> still change. This project was first published as "GFI (Graphic Function
> Image)".

## How it works

Every pixel channel is a function of the backdrop value *b* behind it:

```
result = K + (W − K) × b
```

*K* is how the pixel looks over black and *W* how it looks over white. That
straight line reproduces normal, multiply, screen, hard light, exclusion and
inversion exactly, mixed freely per pixel and stacked in any order. A plain PNG
pixel is the special case where W − K is the same grey on all three channels.

The file stores the artwork's native look (its own colours, as exported over a
transparent background) as the main PNG image, plus a `vITR` chunk holding the
residual needed to rebuild K and W exactly. To draw it, a renderer stacks three
layers with blend modes every host already has: **difference**, **screen**,
**multiply**. Read the full [specification](SPEC.md).

## Quick start

### Make a Vitral image from any design tool

Export your artwork over a black background and over a white background (keep
the layers' blend modes active), and optionally once more with a transparent
background, then:

```sh
cargo install --path crates/vitral-cli        # installs the `vitral` command
vitral encode --black art-on-black.png --white art-on-white.png \
              --transparent art.png -o art.vitral.png
```

Other commands:

```sh
vitral from-image shadow.png --mode multiply -o shadow.vitral.png
vitral compose --layer glass.png:multiply --layer shine.png:screen:0.4 -o card.vitral.png
vitral render art.vitral.png --backdrop photo.png --x 40 --y 20 -o preview.png
vitral inspect art.vitral.png
vitral decode art.vitral.png --out-dir layers/
```

### Use it on a website

```html
<img src="art.vitral.png" alt="…">
<script type="module" src="web/vitral.js"></script>
```

The script decodes the file with WebAssembly and stacks three canvases with CSS
`mix-blend-mode`. Without JavaScript the page shows the fallback. As with CSS
blend modes, the image blends with content in its own stacking context.

The demo page (`web/index.html`) compares a plain PNG viewer with Vitral over
several backdrops and includes an encoder. Serve the repository root with any
static server and open `/web/`:

```sh
python3 -m http.server 8000    # then open http://localhost:8000/web/
```

## Repository

| Path | Contents |
|---|---|
| [`SPEC.md`](SPEC.md) | Format specification (draft 0.1) |
| [`crates/vitral`](crates/vitral) | Rust reference implementation: codec, reference compositor, W3C blend modes, render planes |
| [`crates/vitral-cli`](crates/vitral-cli) | `vitral` command-line tool |
| [`crates/vitral-wasm`](crates/vitral-wasm) | WebAssembly bindings |
| [`web/`](web) | Polyfill (`vitral.js`), demo page, prebuilt WebAssembly in `web/pkg`, browser test |
| [`examples/`](examples) | Sample files, the two source exports of one sample, comparison renders |
| [`docs/pt/`](docs/pt) | Planning documents in Portuguese: state of the art, requirements, decisions, roadmap |

## Development

```sh
cargo test --release                                  # unit and round-trip tests
cargo run --release -p vitral --example samples       # regenerate examples/
./web/build.sh                                        # rebuild web/pkg (needs wasm-bindgen-cli 0.2.129)
node web/test/run.mjs                                 # Chromium vs reference compositor (needs Playwright)
```

## Roadmap

1. **Done in 0.1**: specification draft, Rust implementation, CLI, web polyfill,
   demo.
2. **Next**: Figma plugin (export and import), playground and packages on
   crates.io and npm, conformance test suite, fuzzing.
3. **Then**: Illustrator, After Effects and Photoshop scripts; Vitral video for
   light leaks and overlays.
4. **v2**: layers with overlay, soft light and HSL modes; backdrop blur and
   refraction; 16-bit and wide gamut.

The detailed plan, including promotion, is in
[`docs/pt/04-plano.md`](docs/pt/04-plano.md).

## License

MIT. The specification is free to implement without royalties.
