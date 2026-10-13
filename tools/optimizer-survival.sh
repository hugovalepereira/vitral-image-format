#!/usr/bin/env bash
# Runs each sample through common PNG optimisers and re-encoders and reports
# whether the Vitral data survives. Tools that are not installed are skipped.
#
#   cargo build --release -p vitral-cli
#   tools/optimizer-survival.sh [sample.vitral.png ...]
#
# Optional: SHARP_DIR=<dir with node_modules/sharp> to include sharp (libvips).
set -u
cd "$(dirname "$0")/.."
VITRAL=${VITRAL:-target/release/vitral}
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT
samples=("$@")
[ ${#samples[@]} -eq 0 ] && samples=(examples/*.vitral.png)

# name|command (IN and OUT are replaced by paths)
tools=(
  "optipng (default)|optipng -quiet -out OUT IN"
  "optipng -strip all|optipng -quiet -strip all -out OUT IN"
  "oxipng (default)|oxipng -q --out OUT IN"
  "oxipng --strip safe|oxipng -q --strip safe --out OUT IN"
  "oxipng --strip all|oxipng -q --strip all --out OUT IN"
  "pngcrush (default)|pngcrush -q IN OUT"
  "pngcrush -rem allb|pngcrush -q -rem allb IN OUT"
  "zopflipng (default)|zopflipng -y IN OUT"
  "advpng -z|cp IN OUT && advpng -q -z -3 OUT"
  "pngquant (lossy)|pngquant --force --output OUT IN"
  "ImageMagick convert|convert IN OUT"
  "Pillow save|python3 -c 'import sys; from PIL import Image; Image.open(sys.argv[1]).save(sys.argv[2])' IN OUT"
  "ffmpeg|ffmpeg -loglevel error -y -i IN OUT"
)
if [ -n "${SHARP_DIR:-}" ]; then
  tools+=("sharp (libvips)|NODE_PATH=$SHARP_DIR/node_modules node -e \"require('sharp')(process.argv[1]).png().toFile(process.argv[2])\" IN OUT")
fi

echo "| Tool | $(for s in "${samples[@]}"; do printf '%s | ' "$(basename "$s" .vitral.png)"; done)"
echo "|---|$(for s in "${samples[@]}"; do printf -- '---|'; done)"
for entry in "${tools[@]}"; do
  name=${entry%%|*}
  cmd=${entry#*|}
  bin=${cmd%% *}
  [ "$bin" = "cp" ] && bin=advpng
  [ "${bin#NODE_PATH}" != "$bin" ] && bin=node
  if ! command -v "$bin" >/dev/null; then continue; fi
  row="| $name |"
  for s in "${samples[@]}"; do
    out="$OUT/$(basename "$s")"
    rm -f "$out"
    c=${cmd//IN/$s}
    c=${c//OUT/$out}
    if ! eval "$c" >/dev/null 2>&1 || [ ! -s "$out" ]; then
      row+=" failed to run |"
      continue
    fi
    status=$("$VITRAL" inspect "$out" 2>/dev/null | sed -n 's/^status: *//p')
    size=$(( ( $(stat -c %s "$out") * 100 ) / $(stat -c %s "$s") ))
    case "$status" in
      "Vitral data OK") verdict="✅ kept" ;;
      "plain PNG"*) verdict="⚪ chunk removed" ;;
      "main image was modified"*) verdict="🟡 pixels changed, detected" ;;
      *) verdict="❓ $status" ;;
    esac
    row+=" $verdict ($size%) |"
  done
  echo "$row"
done
