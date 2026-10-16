// Vitral polyfill — makes <img> elements that point to .vitral.png files blend
// with whatever is behind them.
//
//   <img src="badge.vitral.png" alt="…">
//   <script type="module" src="vitral.js"></script>
//
// Without JavaScript (or for cross-origin files without CORS) the browser shows
// the file's fallback image, because a Vitral file is a valid PNG.
//
// How it works: the file is decoded (WebAssembly) into three layers that are
// stacked over the image with CSS blend modes — difference, screen, multiply —
// which together reproduce `result = K + (W − K) × backdrop` per channel.
// The layers are built for blending in sRGB, the CSS default; a Display P3
// file is converted first (exact over black and white, close elsewhere).
//
// Limitation shared with CSS `mix-blend-mode`: the image only blends with
// content inside the same stacking context. An ancestor with opacity,
// transform, filter, isolation or a z-index creates an isolated group.

import init, { decodePlanes, encodeFromExports, encodeFromRenders } from './pkg/vitral_wasm.js';

const SELECTOR = 'img[data-vitral], img[src$=".vitral.png"]';
const LAYERS = [
  ['difference', (p) => p.difference()],
  ['screen', (p) => p.screen()],
  ['multiply', (p) => p.multiply()],
];
const observers = new WeakMap();

let ready;
const loadWasm = () =>
  (ready ??= init({ module_or_path: new URL('./pkg/vitral_wasm_bg.wasm', import.meta.url) }));

/**
 * Decodes the bytes of a file. Returns the WebAssembly `Planes` object (call
 * `.free()` when done); `planes.isVitral` is false for plain PNGs.
 */
export async function decode(bytes) {
  await loadWasm();
  return decodePlanes(bytes);
}

/**
 * Encodes a .vitral.png from RGBA8 renders of the same artwork: over black,
 * over white and, optionally, over transparency (its look with nothing behind
 * it; approximated when omitted). The pixels are in `colourSpace` ("srgb" or
 * "display-p3"). Returns the file bytes.
 */
export async function encode(width, height, overBlack, overWhite, overTransparent, colourSpace = 'srgb') {
  await loadWasm();
  return encodeFromRenders(width, height, overBlack, overWhite, overTransparent, colourSpace);
}

/**
 * Encodes a .vitral.png from the PNG files a design tool exported (bytes):
 * over black, over white and, optionally, over transparency. sRGB and
 * Display P3 exports are both kept in their colour space. Returns the file.
 */
export async function encodeExports(overBlack, overWhite, overTransparent) {
  await loadWasm();
  return encodeFromExports(overBlack, overWhite, overTransparent);
}

/** Puts RGBA8 pixels in `colourSpace` into a new canvas of the given size. */
export function toCanvas(width, height, pixels, colourSpace = 'srgb') {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const options = colourSpace === 'srgb' ? undefined : { colorSpace: colourSpace };
  const context = canvas.getContext('2d', options);
  // Browsers return null when a page uses up its canvas memory (e.g. on iOS).
  if (!context) throw new Error('could not create a canvas');
  context.putImageData(new ImageData(new Uint8ClampedArray(pixels), width, height, options), 0, 0);
  return canvas;
}

/**
 * Upgrades one <img>. Resolves to the decoder status: "ok" when the image now
 * blends, anything else when it was left as a plain image.
 */
export async function upgrade(img) {
  const state = img.dataset.vitralState;
  if (state === 'loading' || state === 'ok') return state;
  img.dataset.vitralState = 'loading';
  let status;
  try {
    const response = await fetch(img.currentSrc || img.src);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const planes = await decode(new Uint8Array(await response.arrayBuffer()));
    try {
      status = planes.status;
      // The image may have been turned off or removed while loading.
      if (planes.isVitral && img.dataset.vitralState === 'loading' && img.isConnected) {
        attach(img, planes);
      }
    } finally {
      planes.free();
    }
  } catch (error) {
    status = `error: ${error.message}`;
  }
  if (img.dataset.vitralState === 'loading') img.dataset.vitralState = status;
  return img.dataset.vitralState;
}

function attach(img, planes) {
  // Build every canvas before touching the page: if one fails, the <img> is
  // left exactly as it was and keeps showing its plain PNG.
  const canvases = [];
  for (const [mode, pixels] of LAYERS) {
    if (mode === 'difference' && !planes.usesDifference) continue;
    const canvas = toCanvas(planes.width, planes.height, pixels(planes));
    canvas.dataset.vitralLayer = mode;
    canvas.style.cssText = `position:absolute;pointer-events:none;mix-blend-mode:${mode}`;
    canvases.push(canvas);
  }

  const wrapper = document.createElement('span');
  wrapper.className = 'vitral';
  const display = getComputedStyle(img).display;
  // No z-index, opacity or transform: the wrapper must not isolate blending.
  wrapper.style.cssText =
    `position:relative;display:${display.startsWith('inline') ? 'inline-block' : 'block'};line-height:0`;
  img.replaceWith(wrapper);
  wrapper.append(img, ...canvases);
  // Keep the <img> for layout, accessibility, dragging and "save image as",
  // but hide its fallback pixels.
  img.style.opacity = '0';

  // Cover exactly the picture inside the <img> (its content box, without
  // border or padding), whatever CSS does to its size or position.
  const fit = () => {
    const css = getComputedStyle(img);
    const padLeft = parseFloat(css.paddingLeft);
    const padTop = parseFloat(css.paddingTop);
    const box = {
      left: img.offsetLeft + img.clientLeft + padLeft,
      top: img.offsetTop + img.clientTop + padTop,
      width: img.clientWidth - padLeft - parseFloat(css.paddingRight),
      height: img.clientHeight - padTop - parseFloat(css.paddingBottom),
    };
    for (const canvas of canvases) {
      for (const [key, value] of Object.entries(box)) canvas.style[key] = `${value}px`;
    }
  };
  fit();
  // The wrapper changes size when the page reflows around a centred image,
  // even if the image itself keeps its size.
  const observer = new ResizeObserver(fit);
  observer.observe(img);
  observer.observe(wrapper);
  observers.set(img, observer);
}

/**
 * Removes the blend layers and shows the plain fallback image again. The image
 * is marked "off" so automatic upgrading leaves it alone; call upgrade() to
 * turn it back on.
 */
export function downgrade(img) {
  observers.get(img)?.disconnect();
  observers.delete(img);
  const wrapper = img.parentElement;
  if (wrapper?.classList.contains('vitral')) {
    wrapper.replaceWith(img);
  }
  img.style.opacity = '';
  img.dataset.vitralState = 'off';
}

/** Upgrades every matching image now and whenever new ones are added. */
export function upgradeAll(root = document) {
  // Automatic upgrades skip images that were already handled (or turned off).
  const auto = (img) => img.dataset.vitralState || upgrade(img);
  const run = (node) => node.querySelectorAll?.(SELECTOR).forEach(auto);
  run(root);
  new MutationObserver((records) => {
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (node.matches?.(SELECTOR)) auto(node);
        else run(node);
      }
    }
  }).observe(root === document ? document.documentElement : root, { childList: true, subtree: true });
}

// Set `window.VITRAL_MANUAL = true` before loading to call upgrade() yourself.
if (!globalThis.VITRAL_MANUAL) {
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => upgradeAll(), { once: true });
  } else {
    upgradeAll();
  }
}
