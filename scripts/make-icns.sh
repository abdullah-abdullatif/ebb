#!/usr/bin/env bash
# Turn a 1024px PNG into a macOS .icns (needs macOS: sips + iconutil).
set -euo pipefail
SRC=${1:-Resources/AppIcon.png}
OUT=${2:-build/AppIcon.icns}
SET=$(mktemp -d)/AppIcon.iconset
mkdir -p "$SET" "$(dirname "$OUT")"
for size in 16 32 128 256 512; do
  sips -z $size $size "$SRC" --out "$SET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size*2)) $((size*2)) "$SRC" --out "$SET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$SET" -o "$OUT"
echo "→ $OUT"
