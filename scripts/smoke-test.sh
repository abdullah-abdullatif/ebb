#!/usr/bin/env bash
# Runs the real, built Ebb unattended and checks a full cycle on any OS:
# launch → eye break → heads-up → break screen → break ends → back to work, with no crash.
# Smoke-test mode makes one "minute" last one second and pretends someone is typing,
# so a 50-minute cycle takes 50 seconds. Screenshots (macOS) land in build/smoke/.
#
#   scripts/smoke-test.sh path/to/ebb-binary
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${1:-src-tauri/target/release/ebb}"
OUT=build/smoke
rm -rf "$OUT" && mkdir -p "$OUT/data"
LOG="$OUT/ebb.log"

# Fresh settings and history, kept away from the real ones.
EBB_SMOKE_TEST=1 EBB_DATA_DIR="$PWD/$OUT/data" "$BIN" >"$LOG" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true' EXIT

alive() { kill -0 "$PID" 2>/dev/null || { echo "FAIL: Ebb exited unexpectedly"; cat "$LOG"; exit 1; }; }
wait_for() { # <text> <timeout seconds>
  for ((i = 0; i < $2; i++)); do
    grep -qF "$1" "$LOG" && { echo "ok: $1"; return 0; }
    alive; sleep 1
  done
  echo "FAIL: timed out waiting for '$1'"; cat "$LOG"; exit 1
}
shot() { [[ "$(uname)" == Darwin ]] && screencapture -x "$OUT/$1.png" 2>/dev/null || true; }

wait_for 'EBB_EVENT "launched"' 60
sleep 3; shot 1-launched-with-settings
wait_for "EBB_EVENT EyeBreakStarted" 40
sleep 2; shot 2-eye-break
wait_for "EBB_EVENT BreakStarted" 70
sleep 3; shot 3-break-screen
wait_for "EBB_EVENT BreakEnded { completed: true }" 30
sleep 3; shot 4-back-to-work
alive
test -s "$OUT/data/stats.json" || sleep 31
test -s "$OUT/data/stats.json" || { echo "FAIL: stats.json was never written"; exit 1; }

echo "PASS: full work → break → work cycle, no crash"
grep -F "EBB_EVENT" "$LOG"
