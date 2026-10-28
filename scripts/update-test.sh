#!/usr/bin/env bash
# Proves auto-update works end to end: runs an old build of Ebb (made by the update-test
# workflow, labelled 0.0.1) against the real GitHub release, and checks that it finds the
# update, downloads it, verifies its signature, installs it and restarts.
# The caller then checks that the new version is what's installed.
#
#   scripts/update-test.sh path/to/old-ebb-binary-or-AppImage
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="$1"
OUT=build/update-test
rm -rf "$OUT" && mkdir -p "$OUT/data"
LOG="$OUT/ebb.log"

EBB_UPDATE_TEST=1 EBB_DATA_DIR="$PWD/$OUT/data" "$BIN" >"$LOG" 2>&1 &
PID=$!

wait_for() { # <text> <timeout seconds>
  for ((i = 0; i < $2; i++)); do
    grep -qF "$1" "$LOG" && { echo "ok: $1"; return 0; }
    sleep 1
  done
  echo "FAIL: timed out waiting for '$1'"; cat "$LOG"; exit 1
}

wait_for "EBB_UPDATE Checking" 60
wait_for "EBB_UPDATE Downloading" 60
wait_for "EBB_UPDATE Ready" 300
wait_for "EBB_UPDATE installing" 30
# Windows hands over to the installer and exits; macOS and Linux print this, then restart.
[[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* ]] || wait_for "EBB_UPDATE installed, restarting" 120
sleep 15
grep -F "EBB_UPDATE" "$LOG"
if grep -q "EBB_UPDATE Error" "$LOG"; then echo "FAIL: updater reported an error"; exit 1; fi
kill "$PID" 2>/dev/null || true
echo "PASS: the old build found, verified and installed the update"
