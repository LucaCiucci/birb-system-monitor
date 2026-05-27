#!/usr/bin/env bash
set -euo pipefail

echo "==> Building AppImage with cargo-appimage..."
cargo appimage
echo "==> Done!"
