#!/usr/bin/env bash
# Stops any running instance and launches the current release binary.
# Run scripts/build-quick.sh first if you've changed code.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="src-tauri/target/release/deskstat"
if [ ! -x "$BIN" ]; then
  echo "no release binary at $BIN — run scripts/build-quick.sh or scripts/build.sh first" >&2
  exit 1
fi

pkill -f "$BIN" 2>/dev/null || true
sleep 1

nohup "./$BIN" > /tmp/deskstat.log 2>&1 &
disown
sleep 1
echo "started, pid $(pgrep -f "$BIN" | head -1), log at /tmp/deskstat.log"
