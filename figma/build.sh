#!/usr/bin/env sh
# Builds the Figma plugin's ui.html: ui.src.html with the WebAssembly module
# and its bindings inlined, because Figma loads a plugin's UI as one HTML
# string. Same requirements as web/build.sh.
#
#   ./figma/build.sh           rebuild ui.html
#   ./figma/build.sh --check   fail if ui.html is out of date
set -eu
cd "$(dirname "$0")/.."
cargo build -p vitral-wasm --profile wasm --target wasm32-unknown-unknown
wasm-bindgen --target no-modules --no-typescript --out-dir target/figma-pkg \
  target/wasm32-unknown-unknown/wasm/vitral_wasm.wasm
node figma/inline.mjs "$@"
