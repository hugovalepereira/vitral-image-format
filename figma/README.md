# Vitral for Figma

A Figma plugin that exports layers as Vitral files, so their blend modes keep
working wherever the image is placed, and places Vitral files back in a
document.

It is a first version and is not on Figma Community yet.

## Load it

1. Get this folder on your computer: clone the repository, or download it as a
   ZIP from GitHub and unzip it.
2. In the **Figma desktop app**, open a design file and choose **Plugins →
   Development → Import plugin from manifest…**, then pick
   `figma/manifest.json`.
3. Run it from **Plugins → Development → Vitral**.

The plugin needs no network access.

## Export

Select one or more layers (a frame, a group, a component, a single shape) and
press **Export .vitral.png**. Each layer becomes its own file, named after it
(`Logo@2x.vitral.png` at 2x).

Behind the button, the plugin renders the layer three times, over black, over
white and over nothing, and encodes the three renders into one file with the
Vitral encoder (compiled to WebAssembly, running inside the plugin). The
renders use the document's colour profile, so a Display P3 file gives a
Display P3 Vitral. The panel then shows the result next to a plain PNG over the
same backdrop.

Before you export, the panel tells you about anything that won't survive:

* **A group or frame set to Normal** inside the selection (or the selected
  frame itself). Figma flattens it on its own first, so the blend modes inside
  only mix within it, and the file records exactly that. Set it to **Pass
  through** if they should reach the background.
* **A selected frame with a fill.** The blend modes inside mix with that fill,
  not with what is behind. Hide the fill to let them through.
* **Modes Vitral 0.1 can't keep** where they meet the background: overlay, soft
  light, colour dodge, colour burn, hue, saturation, colour and luminosity are
  only right over black and white; darken, lighten, difference, linear burn and
  linear dodge are approximated between those.
* **Background blur**, which won't blur what is behind the file.

If the result turns out opaque, it looks the same as a regular PNG, and the
panel says so.

## Import

Open the **Import** tab and drop a `.vitral.png` on it (or click to choose
one). The plugin places a Pass through group with the file's layers, bottom to
top **difference**, **screen** and **multiply** (the difference layer only when
the file needs it), which together blend like the file. Select a frame first
to place it inside that frame; otherwise it goes in the middle of the screen.

* The layers need a **filled background** behind them. Over a frame without a
  fill, they can't show the file's own look (decision D9 in
  [`docs/pt/03-decisoes.md`](../docs/pt/03-decisoes.md)).
* **File scale** sets how many image pixels make one Figma pixel; a name with
  `@2x` picks 2x for you.
* In a Display P3 document the layers are made in Display P3.
* Figma takes images up to 4096 px a side.
* A PNG that is not a Vitral is placed as one ordinary image.

## Not checked yet in Figma itself

The automated test runs the plugin's code against a stand-in for the Figma API
and the panel in a real browser, with real Figma exports. These need a person
with Figma to confirm:

* That Figma blends in Display P3 in Display P3 files, which the plugin
  assumes when it keeps those exports in P3.
* That Figma reads the colour profile of imported layers in a Display P3
  document as the plugin expects.

## Files

| File | |
|---|---|
| `manifest.json` | Plugin manifest |
| `code.js` | The part that talks to the document |
| `ui.src.html` | The panel (source) |
| `ui.html` | The panel with the WebAssembly module inlined; built, do not edit |
| `build.sh`, `inline.mjs` | Build `ui.html` (needs the same tools as `web/build.sh`) |
| `test/plugin.mjs` | Automated test (needs Playwright and the `vitral` CLI) |

```sh
cargo build --release -p vitral-cli
./figma/build.sh            # after changing ui.src.html or the Rust code
node figma/test/plugin.mjs
```
