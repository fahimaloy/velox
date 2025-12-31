Headless WGPU CI and local run instructions

This document describes how to run the WGPU smoke tests (the ones guarded behind the `wgpu` feature) either in GitHub Actions or locally inside Docker using Google's SwiftShader software Vulkan ICD.

GitHub Actions
---------------
We include a workflow at `.github/workflows/wgpu-smoke.yml` which:
- Installs Vulkan loader and dev packages
- Downloads SwiftShader and registers its ICD
- Sets `VK_ICD_FILENAMES` to the SwiftShader ICD
- Runs `cargo test -p velox-renderer --features wgpu -- --ignored` to run the ignored smoke tests

Local Docker (recommended for reproducible runs)
------------------------------------------------
A provided Dockerfile `docker/wgpu/Dockerfile` builds an image with Rust toolchain and SwiftShader installed. Use the following commands to run the smoke tests locally:

```bash
# build the docker image
docker build -t velox-wgpu -f docker/wgpu/Dockerfile .

# run tests (this will run the ignored smoke tests inside container)
docker run --rm -e VK_ICD_FILENAMES=/opt/swiftshader/etc/vulkan/icd.d/10_swiftshader_icd.json velox-wgpu \
  bash -lc "cargo test -p velox-renderer --features wgpu -- --ignored --nocapture"
```

If you prefer to run locally on your host machine, install Vulkan drivers or SwiftShader and set `VK_ICD_FILENAMES` accordingly before running the cargo command above.

Notes
-----
- SwiftShader provides a software Vulkan ICD that works in CI where GPU drivers may not be present. It is slower but sufficient for smoke testing adapter/device creation.
- If the smoke tests still fail in CI, consider running inside a runner with Docker + hardware support, or adjust the workflow to use a headless X server / ANGLE.
