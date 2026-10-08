#!/usr/bin/env bash
# Build Ebb.app with only the Xcode Command Line Tools — no Xcode project needed.
#   scripts/build-app.sh                  # this Mac's architecture → build/Ebb.app
#   UNIVERSAL=1 scripts/build-app.sh      # Apple Silicon + Intel (needs full Xcode)
#   VERSION=1.2.0 scripts/build-app.sh    # stamp the version shown in Finder
#   SIGN_IDENTITY="Developer ID Application: You" scripts/build-app.sh
set -euo pipefail
cd "$(dirname "$0")/.."

CONFIG=${CONFIG:-release}
ARCH_FLAGS=()
if [[ "${UNIVERSAL:-0}" == 1 ]]; then ARCH_FLAGS=(--arch arm64 --arch x86_64); fi

# ${arr[@]+...} keeps macOS's bash 3.2 happy with `set -u` and an empty array.
swift build -c "$CONFIG" ${ARCH_FLAGS[@]+"${ARCH_FLAGS[@]}"}
BIN="$(swift build -c "$CONFIG" ${ARCH_FLAGS[@]+"${ARCH_FLAGS[@]}"} --show-bin-path)/Ebb"

APP=build/Ebb.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Ebb"
cp Resources/Info.plist "$APP/Contents/Info.plist"
if [[ -n "${VERSION:-}" ]]; then
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist"
fi
scripts/make-icns.sh Resources/AppIcon.png "$APP/Contents/Resources/AppIcon.icns"

# Ad-hoc signature by default; pass a real identity to distribute without the Gatekeeper prompt.
codesign --force --sign "${SIGN_IDENTITY:--}" --timestamp=none "$APP"
echo "Built $APP ($(lipo -archs "$APP/Contents/MacOS/Ebb"))"
