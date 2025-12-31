#!/usr/bin/env bash
# Helper to run headless WGPU smoke tests inside the Docker image
set -euo pipefail

# Build the image (if not already built)
docker build -t velox-wgpu -f docker/wgpu/Dockerfile ..

# Run tests inside container
docker run --rm -e VK_ICD_FILENAMES=/opt/swiftshader/etc/vulkan/icd.d/10_swiftshader_icd.json velox-wgpu \
  bash -lc "cargo test -p velox-renderer --features wgpu -- --ignored --nocapture"
