#!/usr/bin/env bash
set -euo pipefail

echo "Installing system deps (Debian/Ubuntu)"
sudo apt-get update
sudo apt-get install -y libegl1-mesa-dev libgl1-mesa-dev libx11-dev libx11-xcb-dev libglu1-mesa-dev libxext-dev

echo "Building with skia-native feature"
cargo build -p velox-renderer --features "skia-native"

echo "Running Skia ignored tests (may require headless EGL and skia-native libs)"
cargo test -p velox-renderer --features "skia-native" -- --ignored --nocapture
