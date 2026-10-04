Skia-native CI and local run instructions
======================================

This document describes how to run the headless Skia smoke tests (the ones guarded behind the
`skia-native` feature) in GitHub Actions or locally.

GitHub Actions
--------------
The workflow at `.github/workflows/skia-native.yml` installs the Mesa/EGL dev packages on
`ubuntu-latest` and runs the ignored Skia tests.

Local reproduction
------------------
Run `./ci/run-skia-tests.sh` (Debian/Ubuntu) to install the required dependencies and run the
ignored Skia tests.

Notes
-----
- The smoke tests attempt to create an EGL context and a `skia_safe::gpu::DirectContext`; they
  require Mesa/EGL dev packages on the test runner.
- If `skia-native` tests fail due to missing native binaries, consider using `skia-safe` prebuilt
  binaries, or use a custom Docker image with Skia libs installed.
