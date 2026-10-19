#!/usr/bin/env bash
# ScreenCaptureKit precisa do Xcode completo (não só Command Line Tools).
set -euo pipefail
if [[ -d /Applications/Xcode.app/Contents/Developer ]]; then
  export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
fi
cd "$(dirname "$0")/.."
npm run tauri dev
