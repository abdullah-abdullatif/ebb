#!/usr/bin/env bash
# Build Ebb.app with only the Xcode Command Line Tools — no Xcode project needed.
#   scripts/build-app.sh            # release build → build/Ebb.app
#   SIGN_IDENTITY="Developer ID Application: You" scripts/build-app.sh
set -euo pipefail
cd "$(dirname "$0")/.."

CONFIG=${CONFIG:-release}
swift build -c "$CONFIG"
BIN="$(swift build -c "$CONFIG" --show-bin-path)/Ebb"

APP=build/Ebb.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Ebb"
cp Resources/Info.plist "$APP/Contents/Info.plist"
scripts/make-icns.sh Resources/AppIcon.png "$APP/Contents/Resources/AppIcon.icns"

# Ad-hoc signature by default; pass a real identity to distribute.
codesign --force --sign "${SIGN_IDENTITY:--}" --timestamp=none "$APP"
echo "Built $APP"
