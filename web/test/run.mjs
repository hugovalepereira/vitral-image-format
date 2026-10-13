// Browser conformance test: renders every sample with the polyfill and
// compares the screenshot with the Rust reference compositor.
//
//   cargo build --release -p vitral-cli && ./web/build.sh
//   node web/test/run.mjs [--keep]
//
// Requires Playwright (a global install is fine). Engines are chosen with
// VITRAL_BROWSERS (comma-separated: chromium, firefox, webkit; default
// chromium). CHROMIUM_PATH overrides the Chromium executable.

import { createServer } from 'node:http';
import { readFile, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
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

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const cli = join(root, 'target/release/vitral');
const TOLERANCE = 2; // RNF2: the blend-mode stack is within 2 levels of the reference.
const engines = (process.env.VITRAL_BROWSERS || 'chromium').split(',').map((s) => s.trim());
const samples = {
  'stained-glass': [300, 360],
  'glass-badge': [360, 260],
  'light-leak': [420, 280],
  'invert-ring': [260, 260],
};

const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.png': 'image/png' };
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
const base = `http://127.0.0.1:${server.address().port}`;

const out = await mkdtemp(join(tmpdir(), 'vitral-test-'));
let failed = 0;

for (const engine of engines) {
  const options = engine === 'chromium' ? { executablePath: process.env.CHROMIUM_PATH || undefined } : {};
  const browser = await playwright[engine].launch(options);
  const page = await browser.newPage({ viewport: { width: 640, height: 400 }, deviceScaleFactor: 1 });
  page.on('pageerror', (e) => console.error(`${engine} page error:`, e.message));

  for (const [name, [w, h]] of Object.entries(samples)) {
    const label = `${engine.padEnd(8)} ${name}`;
    const x = Math.trunc((640 - w) / 2);
    const y = Math.trunc((400 - h) / 2);
    await page.goto(`${base}/web/test/compare.html?sample=${name}&x=${x}&y=${y}`);
    const state = await page
      .waitForFunction(() => {
        const s = document.querySelector('img')?.dataset.vitralState;
        return s && s !== 'loading' ? s : null;
      }, null, { timeout: 15000 })
      .then((handle) => handle.jsonValue())
      .catch((e) => `timeout (${e.message.split('\n')[0]})`);
    if (state !== 'ok') {
      console.log(`FAIL ${label}: polyfill state "${state}"`);
      failed++;
      continue;
    }
    // Let the canvases paint before taking the screenshot.
    await page.evaluate(() => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok))));
    const shot = join(out, `${engine}-${name}-browser.png`);
    const ref = join(out, `${name}-reference.png`);
    await writeFile(shot, await page.screenshot({ clip: { x: 0, y: 0, width: 640, height: 400 } }));
    execFileSync(cli, ['render', join(root, `examples/${name}.vitral.png`), '--backdrop',
      join(root, 'examples/backdrop.png'), `--x=${x}`, `--y=${y}`, '-o', ref], { stdio: 'ignore' });
    try {
      const report = execFileSync(cli, ['diff', shot, ref, '--tolerance', String(TOLERANCE)], { stdio: 'pipe' });
      console.log(`ok   ${label}: ${report.toString().trim()}`);
    } catch (e) {
      console.log(`FAIL ${label}: ${e.stdout}${e.stderr}`.trim());
      failed++;
    }
  }
  await browser.close();
}

server.close();
if (process.argv.includes('--keep')) console.log(`screenshots kept in ${out}`);
else await rm(out, { recursive: true });
process.exit(failed ? 1 : 0);
