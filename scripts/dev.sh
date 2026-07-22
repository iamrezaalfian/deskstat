#!/usr/bin/env bash
# Runs the app in dev mode (vite + cargo watch, hot reload).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
npx tauri dev
