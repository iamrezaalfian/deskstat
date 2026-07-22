#!/usr/bin/env bash
# Fast rebuild for iteration: compiles the real embedded-frontend binary
# but skips the slow .deb/.rpm/.AppImage bundling step.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
npx tauri build --no-bundle
