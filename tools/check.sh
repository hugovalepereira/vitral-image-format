#!/usr/bin/env bash
# Runs the same checks as CI's rust job, stopping at the first failure.
# Uses the CI toolchain (1.99.0) when installed, otherwise the default one.
set -euo pipefail
cd "$(dirname "$0")/.."
tc=""
if rustup toolchain list 2>/dev/null | grep -q '^1.99.0'; then tc="+1.99.0"; fi
cargo $tc fmt --all --check
cargo $tc clippy --release --all-targets -- -D warnings
cargo $tc test --release
cargo $tc build -p vitral-wasm --profile wasm --target wasm32-unknown-unknown
echo "all checks passed"
