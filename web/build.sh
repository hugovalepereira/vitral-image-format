#!/usr/bin/env sh
# Builds the WebAssembly module used by vitral.js into web/pkg.
# Requires: rustup target add wasm32-unknown-unknown
#           cargo install wasm-bindgen-cli --version 0.2.129
set -eu
cd "$(dirname "$0")/.."
cargo build -p vitral-wasm --profile wasm --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  target/wasm32-unknown-unknown/wasm/vitral_wasm.wasm
ls -l web/pkg
