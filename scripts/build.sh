#!/usr/bin/env bash
# Full production build: binary + .deb/.rpm/.AppImage bundles.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
npx tauri build
