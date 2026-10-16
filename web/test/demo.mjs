// Demo page test: opening your own file (button and drag and drop), plain and
// invalid files, large files, and sending the encoder's result to the stages.
//
//   cargo build --release -p vitral-cli && ./web/build.sh
//   node web/test/demo.mjs
//
// Same requirements and VITRAL_BROWSERS setting as run.mjs.

import { createServer } from 'node:http';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
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
const engines = (process.env.VITRAL_BROWSERS || 'chromium').split(',').map((s) => s.trim());

const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.png': 'image/png' };
const server = createServer(async (req, res) => {
  try {
    const path = join(root, decodeURIComponent(new URL(req.url, 'http://x').pathname));
    if (!path.startsWith(root)) throw new Error('outside root');
    const body = await readFile(path.endsWith('/') ? join(path, 'index.html') : path);
    res.writeHead(200, { 'content-type': types[extname(path) || '.html'] ?? 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
}).listen(0, '127.0.0.1');
await new Promise((ok) => server.once('listening', ok));
const base = `http://127.0.0.1:${server.address().port}`;

// A file larger than the stage: 640 × 400 px.
const out = await mkdtemp(join(tmpdir(), 'vitral-demo-'));
const wide = join(out, 'wide.vitral.png');
execFileSync(join(root, 'target/release/vitral'), ['from-image', join(root, 'examples/backdrop.png'),
  '--mode', 'multiply', '-o', wide], { stdio: 'ignore' });

let failed = 0;
for (const engine of engines) {
  const options = engine === 'chromium' ? { executablePath: process.env.CHROMIUM_PATH || undefined } : {};
  const browser = await playwright[engine].launch(options);
  const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.route(/fonts\.(googleapis|gstatic)\.com/, (route) => route.abort());
  await page.goto(`${base}/web/`);

  const check = (name, ok, detail) => {
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${engine.padEnd(8)} ${name}${ok ? '' : `: ${JSON.stringify(detail)}`}`);
    if (!ok) failed++;
  };
  const vitralState = (stage) => page.$eval(`#stage-${stage} img`, (img) => img.dataset.vitralState);
  const settled = (state) => page.waitForFunction((state) =>
    document.querySelector('#stage-vitral img')?.dataset.vitralState === state, state, { timeout: 15000 });
  const noteStarting = (text) => page.waitForFunction((text) =>
    document.querySelector('#sample-note').textContent.startsWith(text), text, { timeout: 15000 });
  const note = () => page.$eval('#sample-note', (n) => ({ text: n.textContent, error: n.classList.contains('error') }));
  const pressed = () => page.$$eval('#samples button[aria-pressed="true"]', (bs) => bs.map((b) => b.dataset.id).join());
  const placement = () => page.evaluate(() => {
    const s = document.querySelector('#stage-vitral').getBoundingClientRect();
    const o = document.querySelector('#stage-vitral .obj').getBoundingClientRect();
    return { width: o.width / s.width, inside: o.left >= s.left - 1 && o.right <= s.right + 1
      && o.top >= s.top - 1 && o.bottom <= s.bottom + 1 };
  });

  try {
    await settled('ok');

    await page.setInputFiles('#own-input', join(root, 'examples/glass-badge.vitral.png'));
    await noteStarting('glass-badge.vitral.png, 360 × 260 px.');
    await settled('ok');
    check('own Vitral file blends', (await pressed()) === 'own' && (await vitralState('png')) === 'off',
      { pressed: await pressed(), note: await note() });

    await page.setInputFiles('#own-input', join(root, 'examples/sources/stained-glass-over-black.png'));
    await settled('not-vitral');
    check('plain PNG is explained', (await note()).text.includes('has no Vitral data'), await note());

    await page.setInputFiles('#own-input', { name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') });
    await noteStarting('Could not open notes.txt');
    check('invalid file shows an error', (await note()).error, await note());

    await page.click('#samples button[data-id="film"]');
    await settled('ok');
    await page.click('#samples button[data-id="own"]');
    await settled('not-vitral');
    check('own file stays selectable', (await note()).text.startsWith('stained-glass-over-black.png'), await note());

    await page.setInputFiles('#own-input', wide);
    await noteStarting('wide.vitral.png');
    await settled('ok');
    const fit = await placement();
    check('large file fits the stage', fit.inside && Math.abs(fit.width - 0.8) < 0.01, fit);

    const dropped = [...await readFile(join(root, 'crates/vitral/tests/data/figma/mixed-over-white.png'))];
    await page.evaluate((bytes) => {
      const data = new DataTransfer();
      data.items.add(new File([new Uint8Array(bytes)], 'dropped.png', { type: 'image/png' }));
      const target = document.querySelector('#stage-png');
      for (const type of ['dragover', 'drop']) {
        target.dispatchEvent(new DragEvent(type, { dataTransfer: data, bubbles: true, cancelable: true }));
      }
    }, dropped);
    await noteStarting('dropped.png, 184 × 184 px.');
    check('dropped file opens', true);

    await page.waitForSelector('#try-above:not([hidden])', { timeout: 15000 });
    await page.click('#try-above');
    await noteStarting('lens.vitral.png, 640 × 640 px.');
    await settled('ok');
    check('encoder result opens in the stages', true);

    // Display P3 exports give a Display P3 file, not one converted to sRGB.
    for (const which of ['black', 'white', 'transparent']) {
      await page.setInputFiles(`#${which}-input`, join(root, `examples/sources/p3-neon-over-${which}.png`));
    }
    await page.waitForFunction(() => document.querySelector('#encode-status').textContent.includes('Display P3'),
      null, { timeout: 15000 });
    check('Display P3 exports stay Display P3', true);
  } catch (e) {
    check('demo', false, e.message.split('\n')[0]);
  }
  check('no page errors', errors.length === 0, errors);
  await browser.close();
}

server.close();
await rm(out, { recursive: true });
process.exit(failed ? 1 : 0);
