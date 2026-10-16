// Writes figma/ui.html from figma/ui.src.html and the wasm-bindgen output in
// target/figma-pkg (built by figma/build.sh).
//
// With --check it writes nothing and fails if the committed ui.html was not
// built from the current UI source and bindings. The WebAssembly bytes are
// left out of that comparison, since they can vary with the build machine;
// the browser tests run on the freshly built module.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = join(root, 'target/figma-pkg');
const out = join(root, 'figma/ui.html');

const source = readFileSync(join(root, 'figma/ui.src.html'), 'utf8');
// The bindings look for the .wasm file next to their <script src>. Inline,
// with Figma's UI loaded from a data: or about:srcdoc address, that lookup
// throws before anything runs; the module bytes are passed in instead.
const SCRIPT_SRC = /\n *if \(typeof document !== 'undefined' && document\.currentScript !== null\) \{\n *script_src = new URL\(document\.currentScript\.src, location\.href\)\.toString\(\);\n *\}\n/;
const generated = readFileSync(join(pkg, 'vitral_wasm.js'), 'utf8');
if (!SCRIPT_SRC.test(generated)) throw new Error('wasm-bindgen output changed: update SCRIPT_SRC');
const bindings = generated.replace(SCRIPT_SRC, '\n');
const wasm = readFileSync(join(pkg, 'vitral_wasm_bg.wasm')).toString('base64');
if (/<\/script/i.test(bindings)) throw new Error('bindings contain </script>');

const build = (blob) => {
  const html = source
    .replace('/*VITRAL_WASM_BINDINGS*/', () => bindings)
    .replace('__VITRAL_WASM_BASE64__', () => blob);
  if (html.includes('/*VITRAL_WASM_BINDINGS*/') || html.includes('__VITRAL_WASM_BASE64__')) {
    throw new Error('placeholder left in ui.html');
  }
  return html;
};

if (process.argv.includes('--check')) {
  let committed;
  try {
    committed = readFileSync(out, 'utf8');
  } catch {
    committed = '';
  }
  const blob = /const WASM_BASE64 = '([A-Za-z0-9+/=]*)';/.exec(committed);
  if (!blob || committed !== build(blob[1])) {
    console.error('figma/ui.html is out of date: run ./figma/build.sh and commit it');
    process.exit(1);
  }
  console.log('figma/ui.html is up to date');
} else {
  const html = build(wasm);
  writeFileSync(out, html);
  console.log(`figma/ui.html: ${Math.round(html.length / 1024)} KB`);
}
