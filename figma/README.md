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
3. Run it from **Plugins → Development → Vitral**. Export and import share one
   panel.

The plugin needs no network access.

## Export

Select one or more layers (a frame, a group, a component, a single shape) and
press **Export .vitral.png**. A layer becomes a file named after it
(`Logo@2x.vitral.png` at 2x). With several layers selected you choose **One
file each** (they arrive together in `Vitral export.zip`) or **One file**
(the layers rendered together, as they sit on the canvas, named after their
parent).

Behind the button, the plugin renders the layers three times, over black, over
white and over nothing, and encodes the three renders into one file with the
Vitral encoder (compiled to WebAssembly, running inside the plugin). It renders
copies in a temporary frame placed off screen, so nothing on the canvas
changes while it works. The renders use the document's colour profile, so a
Display P3 file gives a Display P3 Vitral. The panel then shows the result
next to a plain PNG over the same backdrop.

Before you export, the panel tells you about anything that won't survive:

* **A group or frame set to Normal** inside the selection (or the selected
  frame itself). Figma flattens it on its own first, so the blend modes inside
  only mix within it, and the file records exactly that. Set it to **Pass
  through** if they should reach the background.
* **A selected frame with a fill.** The blend modes inside mix with that fill,
  not with what is behind. Hide the fill to let them through.
* **Modes Vitral 0.1 can't keep exactly** where they meet the background. A
  Vitral 0.1 pixel reacts to the background along a straight line between how
  it looks over black and how it looks over white, so:
  * darken, lighten, difference, linear burn and linear dodge are exact over
    black and white and approximated in between. The error grows where the
    background is close to the layer's own colour: a difference square in
    RGB 248 193 91 over RGB 245 245 245 gives 17 67 161 instead of Figma's
    3 52 154.
  * overlay, soft light, colour dodge, colour burn, hue, saturation, colour and
    luminosity are only right over black and white.
* **Background blur**, which won't blur what is behind the file.

If the result turns out opaque, it looks the same as a regular PNG, and the
panel says so.

## Import

There are three ways in, all ending in a Pass through group with the file's
layers, bottom to top **difference**, **screen** and **multiply** (the
difference layer only when the file needs it), which together blend like the
file:

* **Drop a `.vitral.png` on the panel** (or click it to choose files). Select a
  frame first to place it inside that frame; otherwise it goes in the middle of
  the screen. **File scale** sets how many image pixels make one Figma pixel;
  a name with `@2x` picks 2x for you.
* **Drag a `.vitral.png` onto the canvas while the panel is open.** It is
  rebuilt where it lands, inside the frame you drop it on. Other files are left
  to Figma.
* **Select a Vitral that Figma placed as a flat image** (dragged in while the
  plugin was closed). The panel offers **Rebuild**, which swaps the image for
  the layers at the same size and place (checked in Figma: it keeps the
  file's bytes).

Notes:

* The layers need a **filled background** behind them. Over a frame without a
  fill, they can't show the file's own look (decision D9 in
  [`docs/pt/03-decisoes.md`](../docs/pt/03-decisoes.md)).
* In a Display P3 document the layers are made in Display P3.
* Figma takes images up to 4096 px a side.
* A PNG that is not a Vitral is placed as one ordinary image.
* Plugins only run while they are open, so a file dragged in with the plugin
  closed stays a flat image until you select it and rebuild it.

## Not checked yet in Figma itself

The automated test runs the plugin's code against a stand-in for the Figma API
and the panel in a real browser, with real Figma exports. These need a person
with Figma to confirm:

* That Figma blends in Display P3 in Display P3 files, which the plugin
  assumes when it keeps those exports in P3.
* That Figma reads the colour profile of imported layers in a Display P3
  document as the plugin expects.
* That Figma hands files dropped on the canvas to the plugin.

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
