// Figma plugin test: runs code.js against a stand-in for the Figma API and the
// built ui.html in a real browser, wired together as Figma does (the UI in an
// iframe, messages through postMessage).
//
//   cargo build --release -p vitral-cli && ./figma/build.sh
//   node figma/test/plugin.mjs
//
// The stand-in can't render, so its exports return real Figma exports from
// crates/vitral/tests/data/figma. What is checked: the layer analysis, how
// the three renders are set up, that the UI's encoder gives the same file as
// the CLI, and that an imported file's three layers composite back to the
// reference result. Uses Chromium (CHROMIUM_PATH to pick a binary).

import { readFileSync, mkdtempSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const playwright = (() => {
  try {
    return require('playwright');
  } catch {
    const globalRoot = execFileSync('npm', ['root', '-g']).toString().trim();
    return require(join(globalRoot, 'playwright'));
  }
})();

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const data = join(root, 'crates/vitral/tests/data/figma');
const code = readFileSync(join(root, 'figma/code.js'), 'utf8');
const html = readFileSync(join(root, 'figma/ui.html'), 'utf8');

let failed = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : `: ${JSON.stringify(detail)}`}`);
  if (!ok) failed++;
};

// --- a stand-in for the parts of the Figma API the plugin uses -------------

let ids = 0;
function node(props) {
  const n = {
    id: `${++ids}:1`,
    visible: true,
    blendMode: 'PASS_THROUGH',
    fills: [],
    strokes: [],
    effects: [],
    parent: null,
    x: 0,
    y: 0,
    width: 0,
    height: 0,
    ...props,
    appendChild(child) {
      if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1);
      this.children.push(child);
      child.parent = this;
    },
    remove() {
      if (this.parent) this.parent.children.splice(this.parent.children.indexOf(this), 1);
      this.parent = null;
      this.removed = true;
    },
    resize(width, height) {
      this.width = width;
      this.height = height;
    },
    insertChild(index, child) {
      if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1);
      this.children.splice(index, 0, child);
      child.parent = this;
    },
    clone() {
      const copy = node({ ...this, id: undefined, parent: null });
      copy.id = `${++ids}:1`;
      if (this.children) {
        copy.children = [];
        for (const c of this.children) copy.appendChild(c.clone());
      }
      if (this.parent) this.parent.appendChild(copy);
      return copy;
    },
  };
  if (props.children) {
    n.children = [];
    for (const c of props.children) n.appendChild(c);
  }
  return n;
}

const solid = (r, g, b, opacity = 1) => ({ type: 'SOLID', color: { r, g, b }, opacity, visible: true, blendMode: 'NORMAL' });

function fakeFigma({ selection = [], profile = 'SRGB', exports = 'mixed' } = {}) {
  const log = { frames: [], exports: [], images: [], notifications: [] };
  const handlers = {};
  const stored = {};
  const page = node({ type: 'PAGE', name: 'Page 1', children: [] });
  page.selection = selection;
  for (const n of selection) if (!n.parent) page.appendChild(n);
  const figma = {
    log,
    root: { documentColorProfile: profile },
    currentPage: page,
    command: '',
    handlers,
    stored,
    viewport: { center: { x: 500, y: 400 }, bounds: { x: 100.5, y: -200.25, width: 800, height: 1200 } },
    ui: { postMessage: () => {}, onmessage: null },
    showUI() {},
    on(name, fn) {
      handlers[name] = fn;
    },
    getImageByHash(hash) {
      return stored[hash] ? { getBytesAsync: async () => stored[hash] } : null;
    },
    notify(message) {
      log.notifications.push(message);
    },
    createFrame() {
      const frame = node({ type: 'FRAME', name: 'Frame', children: [], fills: [solid(1, 1, 1)], blendMode: 'PASS_THROUGH' });
      frame.width = frame.height = 100;
      frame.exportAsync = async (settings) => {
        const fill = frame.fills[0];
        const over = !fill ? 'transparent' : fill.color.r === 0 ? 'black' : 'white';
        const copy = frame.children[0];
        log.exports.push({
          copies: frame.children.map((c) => c.name),
          over,
          settings,
          size: [frame.width, frame.height],
          at: [frame.x, frame.y],
          copyTransform: copy && copy.relativeTransform,
          copyBlend: copy && copy.blendMode,
        });
        return new Uint8Array(readFileSync(join(data, `${exports}-over-${over}.png`)));
      };
      log.frames.push(frame);
      page.appendChild(frame);
      return frame;
    },
    createRectangle() {
      const rect = node({ type: 'RECTANGLE', name: 'Rectangle', blendMode: 'NORMAL' });
      page.appendChild(rect);
      return rect;
    },
    createImage(bytes) {
      log.images.push(bytes);
      return { hash: `image-${log.images.length - 1}` };
    },
    group(nodes, parent) {
      const group = node({ type: 'GROUP', name: 'Group', children: [] });
      parent.appendChild(group);
      for (const n of nodes) group.appendChild(n);
      return group;
    },
  };
  return figma;
}

function loadPlugin(figma) {
  return new Function('figma', '__html__', `${code}\nreturn { analyse };`)(figma, '');
}

// --- layer analysis -------------------------------------------------------

{
  const figma = fakeFigma();
  const { analyse } = loadPlugin(figma);
  const rect = (name, blendMode, extra = {}) => node({ type: 'RECTANGLE', name, blendMode, ...extra });

  const art = node({
    type: 'FRAME',
    name: 'Art',
    children: [
      rect('Shadow', 'MULTIPLY'),
      rect('Glow', 'SCREEN'),
      rect('Tint', 'OVERLAY'),
      rect('Dark', 'DARKEN'),
      rect('Hidden', 'COLOR_BURN', { visible: false }),
      rect('Glass', 'NORMAL', { effects: [{ type: 'BACKGROUND_BLUR', visible: true, radius: 8 }] }),
      rect('Paint', 'NORMAL', { fills: [{ ...solid(1, 0, 0), blendMode: 'LUMINOSITY' }] }),
      node({ type: 'GROUP', name: 'Flat group', blendMode: 'NORMAL', children: [rect('Inner', 'SCREEN')] }),
      node({ type: 'BOOLEAN_OPERATION', name: 'Union', blendMode: 'NORMAL', children: [rect('Part', 'HUE')] }),
    ],
  });
  const r = analyse(art);
  check('analysis: counts the modes that reach the background', r.reaching === 5, r);
  check('analysis: names isolated groups', JSON.stringify(r.isolated) === '[{"name":"Flat group","mode":"Normal"}]', r.isolated);
  check('analysis: lists modes Vitral 0.1 loses',
    JSON.stringify(r.lost) === '[{"name":"Tint","mode":"Overlay"},{"name":"Paint","mode":"Luminosity"}]', r.lost);
  check('analysis: lists approximated modes', JSON.stringify(r.approximate) === '[{"name":"Dark","mode":"Darken"}]', r.approximate);
  check('analysis: flags background blur', JSON.stringify(r.backgroundBlur) === '[{"name":"Glass","mode":""}]', r.backgroundBlur);
  check('analysis: no fill warning without a fill', r.filled === false, r);

  const filled = analyse(node({ type: 'FRAME', name: 'Card', fills: [solid(1, 1, 1)], children: [rect('Ink', 'MULTIPLY')] }));
  check('analysis: warns about a filled frame', filled.filled === true, filled);
  const isolatedRoot = analyse(node({ type: 'FRAME', name: 'Card', blendMode: 'NORMAL', children: [rect('Ink', 'MULTIPLY')] }));
  check('analysis: an exported frame set to Normal is isolated',
    isolatedRoot.reaching === 0 && isolatedRoot.isolated.length === 1, isolatedRoot);
  const plain = analyse(node({ type: 'GROUP', name: 'Logo', children: [rect('A', 'NORMAL'), rect('B', 'PASS_THROUGH')] }));
  check('analysis: plain artwork has nothing that reaches', plain.reaching === 0 && plain.isolated.length === 0, plain);
  const single = analyse(rect('Stamp', 'MULTIPLY', { fills: [solid(0.2, 0.4, 0.8)] }));
  check('analysis: a single blended layer reaches', single.reaching === 1 && !single.filled, single);
}

// --- the plugin and its UI in a browser -----------------------------------

const browser = await playwright.chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined });
const page = await browser.newPage({ acceptDownloads: true });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => {
  if (m.type() === 'error') errors.push(m.text());
});

const toB64 = (u8) => Buffer.from(u8).toString('base64');
const pack = (v) => (v instanceof Uint8Array ? { __u8: toB64(v) }
  : Array.isArray(v) ? v.map(pack)
    : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, pack(x)])) : v);
const unpack = (v) => (v && v.__u8 !== undefined ? new Uint8Array(Buffer.from(v.__u8, 'base64'))
  : Array.isArray(v) ? v.map(unpack)
    : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, unpack(x)])) : v);

let figma;
await page.exposeFunction('__toPlugin', async (msg) => {
  await figma.ui.onmessage(unpack(msg));
});

// Opens the UI as Figma does: an iframe whose address can't resolve relative
// URLs (Figma uses a data: URL; about:srcdoc behaves the same).
async function openPlugin(options) {
  figma = fakeFigma(options);
  figma.ui.postMessage = (msg) => {
    page.evaluate((m) => window.__toUI(m), pack(msg)).catch((e) => errors.push(e.message));
  };
  loadPlugin(figma);
  await page.setContent(`<!doctype html><body style="margin:0">
    <iframe id="ui" style="width:320px;height:600px;border:0"></iframe>
    <script>(() => {
      const fromB64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
      const toB64 = (u8) => { let s = ''; for (let i = 0; i < u8.length; i += 0x8000) s += String.fromCharCode(...u8.subarray(i, i + 0x8000)); return btoa(s); };
      const walk = (v, f) => Array.isArray(v) ? v.map((x) => walk(x, f)) : v && typeof v === 'object' && !(v instanceof Uint8Array) && v.__u8 === undefined
        ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, walk(x, f)])) : f(v);
      const ui = document.getElementById('ui');
      window.__toUI = (m) => ui.contentWindow.postMessage({ pluginMessage: walk(m, (v) => v && v.__u8 !== undefined ? fromB64(v.__u8) : v) }, '*');
      window.addEventListener('message', (e) => {
        if (e.source === ui.contentWindow && e.data && e.data.pluginMessage) {
          window.__toPlugin(walk(e.data.pluginMessage, (v) => v instanceof Uint8Array ? { __u8: toB64(v) } : v));
        }
      });
    })();</script></body>`);
  await page.evaluate((src) => { document.getElementById('ui').srcdoc = src; }, html);
  const frame = await (await page.waitForSelector('#ui')).contentFrame();
  await frame.waitForFunction(() => document.querySelector('#selection'));
  return frame;
}

const text = (frame, sel) => frame.$eval(sel, (el) => el.textContent);

async function importInto(ui, parent, file) {
  await ui.setInputFiles('#file', file);
  const deadline = Date.now() + 15000;
  while (!parent.children.length && Date.now() < deadline) await new Promise((ok) => setTimeout(ok, 50));
  return parent.children[0];
}

async function until(condition, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (!condition() && Date.now() < deadline) await new Promise((ok) => setTimeout(ok, 50));
  return condition();
}

// Reads the names and contents of a stored (uncompressed) ZIP archive.
function unzip(buf) {
  const files = {};
  let i = 0;
  while (buf.readUInt32LE(i) === 0x04034b50) {
    const size = buf.readUInt32LE(i + 18);
    const nameLength = buf.readUInt16LE(i + 26);
    const name = buf.subarray(i + 30, i + 30 + nameLength).toString('utf8');
    files[name] = buf.subarray(i + 30 + nameLength, i + 30 + nameLength + size);
    i += 30 + nameLength + size;
  }
  return files;
}

// Composites the placed layers over a synthetic backdrop with the browser's
// blend modes and returns the worst channel error against the reference
// compositor (WebAssembly `composite`).
async function compositeError(ui, group, bytes) {
  const layers = group.children.map((c) => ({
    mode: c.blendMode.toLowerCase(),
    png: toB64(figma.log.images[Number(c.fills[0].imageHash.split('-')[1])]),
  }));
  return ui.evaluate(async (args) => {
    const fromB64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
    const file = fromB64(args.file);
    const first = await createImageBitmap(new Blob([fromB64(args.layers[0].png)], { type: 'image/png' }));
    const w = first.width, h = first.height;
    const backdrop = new Uint8ClampedArray(w * h * 4);
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        backdrop.set([(x * 255) / (w - 1), (y * 255) / (h - 1), ((x + y) * 97) % 256, 255], (y * w + x) * 4);
      }
    }
    const reference = wasm_bindgen.composite(file, new Uint8Array(backdrop), w, h, 0, 0);
    const canvas = new OffscreenCanvas(w, h);
    const ctx = canvas.getContext('2d');
    ctx.putImageData(new ImageData(backdrop, w, h), 0, 0);
    for (const layer of args.layers) {
      const bitmap = await createImageBitmap(new Blob([fromB64(layer.png)], { type: 'image/png' }),
        { colorSpaceConversion: 'none', premultiplyAlpha: 'none' });
      ctx.globalCompositeOperation = layer.mode;
      ctx.drawImage(bitmap, 0, 0);
    }
    const got = ctx.getImageData(0, 0, w, h).data;
    let max = 0;
    for (let i = 0; i < got.length; i++) if (i % 4 !== 3) max = Math.max(max, Math.abs(got[i] - reference[i]));
    return max;
  }, { file: toB64(bytes), layers });
}

try {
  // Export: a frame at a fractional position, rotated in a rotated parent.
  const art = node({
    type: 'FRAME',
    name: 'Mixed',
    children: [node({ type: 'RECTANGLE', name: 'Ink', blendMode: 'MULTIPLY' })],
    absoluteRenderBounds: { x: 40.4, y: -10.6, width: 183.2, height: 183.5 },
    absoluteTransform: [[0.6, -0.8, 120.25], [0.8, 0.6, -3.5]],
    relativeTransform: [[1, 0, 0], [0, 1, 0]],
  });
  const ui = await openPlugin({ selection: [art] });
  await ui.waitForFunction(() => document.querySelector('#selection').textContent.includes('Mixed'));
  check('ui: shows the selection', (await text(ui, '#selection')).includes('184 × 184'), await text(ui, '#selection'));
  check('ui: export is enabled', await ui.$eval('#export', (b) => !b.disabled), null);
  check('ui: no “one file or several” choice for one layer', await ui.$eval('#together-row', (r) => r.hidden), null);
  check('ui: export and import share one panel',
    await ui.evaluate(() => !!document.querySelector('#export') && !!document.querySelector('#drop') && !document.querySelector('nav')), null);

  await ui.click('#export-scale button[data-value="2"]');
  const download = page.waitForEvent('download', { timeout: 20000 });
  await ui.click('#export');
  const file = await download;
  check('export: file name carries the scale', file.suggestedFilename() === 'Mixed@2x.vitral.png', file.suggestedFilename());
  const bytes = readFileSync(await file.path());

  const log = figma.log;
  const view = figma.viewport.bounds;
  check('export: three renders, over black, white and nothing',
    log.exports.map((e) => e.over).join() === 'black,white,transparent', log.exports.map((e) => e.over));
  check('export: keeps the document colour profile at the chosen scale',
    log.exports.every((e) => e.settings.format === 'PNG' && e.settings.colorProfile === 'DOCUMENT'
      && e.settings.constraint.type === 'SCALE' && e.settings.constraint.value === 2), log.exports[0].settings);
  check('export: the frame covers the render bounds on whole pixels',
    log.exports.every((e) => e.size.join() === '184,184'), log.exports.map((e) => e.size));
  check('export: the frame sits off screen, on whole pixels, so nothing flickers',
    log.exports.every((e) => e.at[0] >= view.x + view.width && Number.isInteger(e.at[0]) && Number.isInteger(e.at[1])),
    log.exports.map((e) => e.at));
  check('export: the copy keeps its place and rotation within the frame',
    JSON.stringify(log.exports[0].copyTransform) === JSON.stringify([[0.6, -0.8, 80.25], [0.8, 0.6, 7.5]]), log.exports[0].copyTransform);
  check('export: the copy keeps its blend mode', log.exports[0].copyBlend === 'PASS_THROUGH', log.exports[0].copyBlend);
  check('export: the temporary frame is removed and the original left alone',
    log.frames.every((f) => f.removed) && figma.currentPage.children.length === 1 && figma.currentPage.children[0] === art,
    figma.currentPage.children.map((c) => c.name));

  const out = mkdtempSync(join(tmpdir(), 'vitral-figma-'));
  const expected = join(out, 'expected.vitral.png');
  execFileSync(join(root, 'target/release/vitral'), ['encode', '--black', join(data, 'mixed-over-black.png'),
    '--white', join(data, 'mixed-over-white.png'), '--transparent', join(data, 'mixed-over-transparent.png'),
    '-o', expected]);
  const expectedBytes = readFileSync(expected);
  check('export: same file as `vitral encode`', Buffer.compare(bytes, expectedBytes) === 0, bytes.length);
  await ui.waitForFunction(() => document.querySelector('#result-notes').textContent.includes('Saved'));
  check('export: reports the file and its colour space',
    (await text(ui, '#result-notes')).includes('Mixed@2x.vitral.png (sRGB'), await text(ui, '#result-notes'));
  const previewSize = await ui.$eval('#preview-vitral canvas', (c) => {
    const r = c.getBoundingClientRect();
    const stage = c.closest('.stage').getBoundingClientRect();
    return { ok: r.width > 0 && r.height > 0 && r.width <= stage.width && r.height <= stage.height, w: r.width, h: r.height };
  });
  check('export: the preview fits its stage', previewSize.ok, previewSize);
  if (process.env.VITRAL_SCREENSHOTS) {
    await page.screenshot({ path: join(process.env.VITRAL_SCREENSHOTS, 'export.png'), clip: { x: 0, y: 0, width: 320, height: 600 } });
  }

  // Several layers: one file each, as a single ZIP download, or one file.
  const rect = (name, x, y) => node({
    type: 'RECTANGLE', name, blendMode: 'MULTIPLY',
    absoluteRenderBounds: { x, y, width: 100, height: 80 },
    absoluteTransform: [[1, 0, x], [0, 1, y]],
    relativeTransform: [[1, 0, x], [0, 1, y]],
  });
  const top = rect('Top', 50, 20);
  const bottom = rect('Bottom', 10, 0);
  node({ type: 'FRAME', name: 'Artboard', children: [bottom, top] });
  const multi = await openPlugin({ selection: [top, bottom] });
  await multi.waitForFunction(() => !document.querySelector('#together-row').hidden);
  const zipped = page.waitForEvent('download', { timeout: 20000 });
  await multi.click('#export');
  const archive = await zipped;
  check('export: several layers leave as one ZIP download', archive.suggestedFilename() === 'Vitral export.zip', archive.suggestedFilename());
  const entries = unzip(readFileSync(await archive.path()));
  check('export: the ZIP holds one file per layer, bottom layer first',
    Object.keys(entries).join() === 'Bottom.vitral.png,Top.vitral.png', Object.keys(entries));
  check('export: each file in the ZIP is a whole Vitral',
    Object.values(entries).every((b) => Buffer.compare(b, expectedBytes) === 0), Object.values(entries).map((b) => b.length));

  figma.log.exports.length = 0;
  await multi.click('#together button[data-value="together"]');
  const joined = page.waitForEvent('download', { timeout: 20000 });
  await multi.click('#export');
  const one = await joined;
  check('export: “One file” gives a single Vitral named after the layers’ parent', one.suggestedFilename() === 'Artboard.vitral.png', one.suggestedFilename());
  const togetherExports = figma.log.exports;
  check('export: “One file” renders all layers together, stacked as on the canvas',
    togetherExports.length === 3 && togetherExports[0].copies.join() === 'Bottom,Top', togetherExports.map((e) => e.copies));
  check('export: “One file” covers both layers', togetherExports[0].size.join() === '140,100', togetherExports[0].size);

  // Import the file just made into a frame with a fill.
  const board = node({ type: 'FRAME', name: 'Board', width: 400, height: 300, fills: [solid(0.9, 0.5, 0.2)], children: [] });
  const ui2 = await openPlugin({ selection: [board] });
  await ui2.waitForFunction(() => document.querySelector('#import-notes').textContent.includes('Board'));
  check('import: no background warning over a filled frame',
    !(await text(ui2, '#import-notes')).includes('no fill'), await text(ui2, '#import-notes'));
  const group = await importInto(ui2, board, { name: 'Mixed@2x.vitral.png', mimeType: 'image/png', buffer: bytes });
  check('import: places one group inside the selected frame', board.children.length === 1 && group && group.type === 'GROUP', board.children.length);
  check('import: two layers (screen → multiply) when nothing inverts',
    group && group.children.map((c) => c.blendMode).join() === 'SCREEN,MULTIPLY', group && group.children.map((c) => c.blendMode));
  check('import: the group is Pass through and named after the file',
    group && group.blendMode === 'PASS_THROUGH' && group.name === 'Mixed (Vitral)', group && [group.blendMode, group.name]);
  check('import: @2x halves the size and centres it',
    group && group.children.every((c) => c.width === 92 && c.height === 92) && group.x === 154 && group.y === 104,
    group && [group.children[0].width, group.x, group.y]);
  check('import: selects what it placed', figma.currentPage.selection[0] === group, null);
  check('import: the layers composite to the reference (max error ≤ 2)', await compositeError(ui2, group, bytes) <= 2, null);

  // A file that inverts needs the difference layer too.
  const ring = readFileSync(join(root, 'examples/invert-ring.vitral.png'));
  const board2 = node({ type: 'FRAME', name: 'Board', width: 400, height: 300, fills: [solid(0.2, 0.3, 0.9)], children: [] });
  const ui2b = await openPlugin({ selection: [board2] });
  const ringGroup = await importInto(ui2b, board2, { name: 'invert-ring.vitral.png', mimeType: 'image/png', buffer: ring });
  check('import: three layers, difference → screen → multiply, when the file inverts',
    ringGroup && ringGroup.children.map((c) => c.blendMode).join() === 'DIFFERENCE,SCREEN,MULTIPLY', ringGroup && ringGroup.children.map((c) => c.blendMode));
  const ringError = ringGroup && await compositeError(ui2b, ringGroup, ring);
  check('import: those layers composite to the reference (max error ≤ 2)', ringError <= 2, ringError);
  if (process.env.VITRAL_SCREENSHOTS) {
    await page.screenshot({ path: join(process.env.VITRAL_SCREENSHOTS, 'import.png'), clip: { x: 0, y: 0, width: 320, height: 600 } });
  }

  // Display P3 documents get Display P3 layers; the bare page gets a warning.
  const ui3 = await openPlugin({ profile: 'DISPLAY_P3' });
  await ui3.waitForFunction(() => document.querySelector('#import-notes').textContent.includes('Display P3'));
  check('import: warns that the bare page has no fill', (await text(ui3, '#import-notes')).includes('filled background'), await text(ui3, '#import-notes'));
  const before = figma.log.images.length;
  await ui3.setInputFiles('#file', { name: 'Mixed.vitral.png', mimeType: 'image/png', buffer: bytes });
  await until(() => figma.currentPage.children.length > 0);
  const p3 = figma.log.images.slice(before);
  check('import: layers are tagged Display P3 in a P3 document',
    p3.length === 2 && p3.every((png) => Buffer.from(png).includes('iCCP')), p3.length);
  const placed = figma.currentPage.children[0];
  check('import: on the page, at the viewport centre', placed && placed.x === 408 && placed.y === 308, placed && [placed.x, placed.y]);

  // A plain PNG is placed as one ordinary image.
  const ui4 = await openPlugin({});
  await ui4.setInputFiles('#file', join(data, 'mixed-over-transparent.png'));
  await until(() => figma.currentPage.children.length > 0);
  const single = figma.currentPage.children[0];
  check('import: a plain PNG becomes one normal image layer',
    single && single.type === 'RECTANGLE' && single.blendMode === 'NORMAL' && figma.log.images.length === 1, single && single.type);
  check('import: says it was not a Vitral', (await text(ui4, '#import-notes')).includes('not a Vitral file'), await text(ui4, '#import-notes'));

  // A .vitral.png dropped on the canvas while the plugin is open is rebuilt
  // where it lands; other files are left to Figma.
  const canvas = node({ type: 'FRAME', name: 'Canvas', width: 800, height: 600, fills: [solid(1, 1, 1)], children: [] });
  await openPlugin({ selection: [canvas] });
  const dropFile = (name, content) => ({ name, type: 'image/png', getBytesAsync: async () => new Uint8Array(content) });
  check('drop: other files are left to Figma',
    figma.handlers.drop({ node: canvas, x: 10, y: 10, files: [dropFile('photo.png', bytes)], items: [] }) === true, null);
  const handled = figma.handlers.drop({ node: canvas, x: 300, y: 200, files: [dropFile('Mixed@2x.vitral.png', bytes)], items: [] });
  check('drop: a .vitral.png is taken over from Figma', handled === false, handled);
  await until(() => canvas.children.length > 0);
  const dropped = canvas.children[0];
  check('drop: rebuilt as blended layers, centred where it landed',
    dropped && dropped.type === 'GROUP' && dropped.children.map((c) => c.blendMode).join() === 'SCREEN,MULTIPLY'
      && dropped.x === 254 && dropped.y === 154, dropped && [dropped.type, dropped.x, dropped.y]);

  // A Vitral that Figma placed as a flat image can be rebuilt in place.
  const holder = node({ type: 'FRAME', name: 'Holder', width: 500, height: 500, fills: [solid(1, 1, 1)], children: [] });
  const flat = node({
    type: 'RECTANGLE', name: 'Mixed.vitral.png', blendMode: 'PASS_THROUGH', width: 92, height: 92,
    relativeTransform: [[1, 0, 30], [0, 1, 40]],
    fills: [{ type: 'IMAGE', imageHash: 'flat', scaleMode: 'FILL', visible: true }],
  });
  const sibling = node({ type: 'RECTANGLE', name: 'Above', blendMode: 'NORMAL' });
  holder.appendChild(flat);
  holder.appendChild(sibling);
  const ui6 = await openPlugin({ selection: [flat] });
  figma.stored.flat = new Uint8Array(bytes);
  figma.ui.onmessage({ type: 'ready' });
  await ui6.waitForSelector('#candidates button', { timeout: 15000 });
  check('rebuild: offers to rebuild a flat Vitral image',
    (await text(ui6, '#candidates')).includes('“Mixed.vitral.png” is a Vitral'), await text(ui6, '#candidates'));
  await ui6.click('#candidates button');
  await until(() => flat.removed);
  const rebuilt = holder.children[0];
  check('rebuild: replaces the image in place, below what was above it',
    flat.removed && rebuilt && rebuilt.type === 'GROUP' && holder.children[1] === sibling, holder.children.map((c) => c.name));
  check('rebuild: the layers take the image’s size and position',
    rebuilt && rebuilt.children.every((c) => c.width === 92 && c.height === 92
      && JSON.stringify(c.relativeTransform) === JSON.stringify([[1, 0, 30], [0, 1, 40]])), rebuilt && rebuilt.children.map((c) => c.relativeTransform));
  check('rebuild: named after the image', rebuilt && rebuilt.name === 'Mixed (Vitral)', rebuilt && rebuilt.name);

  // Plain images are not offered.
  const photo = node({ type: 'RECTANGLE', name: 'Photo', fills: [{ type: 'IMAGE', imageHash: 'photo', visible: true }] });
  const ui7 = await openPlugin({ selection: [photo] });
  figma.stored.photo = new Uint8Array(readFileSync(join(data, 'mixed-over-white.png')));
  figma.ui.onmessage({ type: 'ready' });
  await new Promise((ok) => setTimeout(ok, 300));
  check('rebuild: plain images are not offered', (await ui7.$$('#candidates button')).length === 0, null);

  // Nothing selected: export stays disabled.
  const ui5 = await openPlugin({});
  await ui5.waitForFunction(() => document.querySelector('#selection').textContent.startsWith('Select'));
  check('ui: export is disabled with nothing selected', await ui5.$eval('#export', (b) => b.disabled), null);
} finally {
  check('no errors in the page', errors.length === 0, errors);
  await browser.close();
}

if (failed) {
  console.error(`${failed} check(s) failed`);
  process.exit(1);
}
console.log('all plugin checks passed');
