// Captures the comparisons shown at the top of the README from the demo page
// itself, so the README shows what the site shows: any PNG viewer on the
// left, Vitral on the right, same file over the same backdrop.
//
//   cargo build --release -p vitral-cli && ./web/build.sh
//   node tools/readme_renders.mjs
//
// Writes examples/renders/site-*.png. Requires Playwright (a global install
// is fine); CHROMIUM_PATH overrides the Chromium executable.

import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { join, extname, resolve, dirname } from 'node:path';
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

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// [output name, sample on the page or a file to open, backdrop]
const shots = [
  ['capsule', 'web/samples/capsule.vitral.png', 'pattern'],
  ['stained-glass', 'examples/stained-glass.vitral.png', 'studio'],
  ['highlighter', 'highlighter', 'type'],
  ['sunglasses', 'sunglasses', 'poster'],
  ['neon', 'neon', 'dark'],
];

const types = {
  '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.png': 'image/png',
  '.jpg': 'image/jpeg', '.svg': 'image/svg+xml', '.css': 'text/css',
};
const server = createServer(async (req, res) => {
  try {
    const path = join(root, decodeURIComponent(new URL(req.url, 'http://x').pathname));
    if (!path.startsWith(root)) throw new Error('outside root');
    const body = await readFile(path);
    res.writeHead(200, { 'content-type': types[extname(path)] ?? 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
}).listen(0, '127.0.0.1');
await new Promise((ok) => server.once('listening', ok));

const browser = await playwright.chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined });
const page = await browser.newPage({
  viewport: { width: 1000, height: 800 }, deviceScaleFactor: 2, colorScheme: 'light', reducedMotion: 'reduce',
});
page.on('pageerror', (e) => console.error('page error:', e.message));
await page.goto(`http://127.0.0.1:${server.address().port}/web/index.html`);

const settled = () => page.waitForFunction(() =>
  document.querySelector('#stage-vitral img')?.dataset.vitralState === 'ok', null, { timeout: 15000 });

for (const [name, sample, backdrop] of shots) {
  if (sample.includes('/')) await page.setInputFiles('#own-input', join(root, sample));
  else await page.click(`#samples button[data-id="${sample}"]`);
  await page.click(`#backdrops button[data-id="${backdrop}"]`);
  await settled();
  await page.waitForLoadState('networkidle');
  await page.evaluate(() => document.fonts.ready);
  const out = join(root, `examples/renders/site-${name}.png`);
  await page.locator('.stages').screenshot({ path: out });
  console.log(out);
}

await browser.close();
server.close();
