#!/usr/bin/env bash
# Runs the real, built Ebb.app unattended and checks a full cycle:
# launch → work → break screen → break ends → back to work, with no crash.
# Smoke-test mode makes one "minute" last one second and pretends someone is typing,
# so a 50-minute cycle takes 50 seconds. Screenshots land in build/smoke/.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=build/smoke
rm -rf "$OUT" && mkdir -p "$OUT"
LOG="$OUT/ebb.log"

defaults delete app.ebb.Ebb >/dev/null 2>&1 || true   # start from default settings
EBB_SMOKE_TEST=1 build/Ebb.app/Contents/MacOS/Ebb >"$LOG" 2>&1 &
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
shot() { screencapture -x "$OUT/$1.png" 2>/dev/null || echo "(no screenshot for $1)"; }

wait_for "EBB_EVENT launched" 20
sleep 3; shot 1-launched-with-settings
wait_for "EBB_EVENT warning" 10
wait_for "EBB_EVENT breakStarted" 70
sleep 3; shot 2-break-screen
wait_for "EBB_EVENT breakEnded(completed: true)" 30
sleep 3; shot 3-back-to-work
alive

echo "PASS: full work → break → work cycle, no crash"
grep -F "EBB_EVENT" "$LOG"
