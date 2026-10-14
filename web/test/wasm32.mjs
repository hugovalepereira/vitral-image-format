// WebAssembly is 32-bit: checks that forged chunk lengths cannot crash the
// decoder there (64-bit Rust tests cannot reach the wrap-around).
//
//   ./web/build.sh && node web/test/wasm32.mjs

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../..', import.meta.url));
const { initSync, decodePlanes } = await import(new URL('../pkg/vitral_wasm.js', import.meta.url));
initSync({ module: readFileSync(`${root}/web/pkg/vitral_wasm_bg.wasm`) });

const file = readFileSync(`${root}/examples/invert-ring.vitral.png`);
const status = (bytes) => {
  const planes = decodePlanes(bytes);
  const s = planes.status;
  planes.free();
  return s;
};

if (status(file) !== 'ok') throw new Error('valid file did not decode');

// Insert, before IEND, chunk headers whose length makes `end + 4` wrap past
// 2^32, plus a few other extreme lengths.
const iend = file.length - 12;
const dataStart = iend + 8;
for (const length of [0xfffffffe - dataStart, 0xffffffff - dataStart - 3, 0xffffffff, 0x7fffffff]) {
  const forged = Buffer.concat([
    file.subarray(0, iend),
    Buffer.from([length >>> 24, (length >>> 16) & 255, (length >>> 8) & 255, length & 255]),
    Buffer.from('zzZz'),
    file.subarray(iend),
  ]);
  const s = status(forged); // throws (RuntimeError: unreachable) if the decoder panics
  console.log(`ok   forged length 0x${(length >>> 0).toString(16)}: ${s}`);
}
