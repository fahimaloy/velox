# velox-renderer

Skia rendering backends, events and HMR for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-renderer.svg)](https://crates.io/crates/velox-renderer) [![Downloads](https://img.shields.io/crates/d/velox-renderer.svg)](https://crates.io/crates/velox-renderer) [![docs.rs](https://img.shields.io/docsrs/velox-renderer)](https://docs.rs/velox-renderer) [![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/velox-renderer.svg)](https://crates.io/crates/velox-renderer) [![CI](https://img.shields.io/github/actions/workflow/status/fahimaloy/velox/ci.yml?branch=main)](https://github.com/fahimaloy/velox/actions/workflows/ci.yml)

## Overview

`velox-renderer` owns everything between a styled `VNode` and pixels: backend selection, window and event loops, offscreen Skia rasterisation, hit testing and input editing, accessibility trees, viewport/DPI maths, and the HMR client. It depends on `velox-core`, `velox-dom` and `velox-style`, and every heavyweight dependency is behind a feature flag — with no features the crate compiles to a fast stub backend and pulls in no windowing or GPU stack at all. The feature set is the crate's real user-facing API.

## Features

- **Feature-flagged backends** — default stub backend (no optional dependencies), `skia`, `svg`, and `skia-native` (winit + skia-safe + EGL/GL)
- **Windowed render loop** — `run_window_vnode_skia` (and its HMR variant) opens a window and drives layout, paint and events
- **Immediate-mode paint** — Skia rasterisation of a styled `VNode` every frame
- **Offscreen rendering** — RGBA and PNG output at an explicit DPI scale
- **Event dispatch** — `click`/`input`/`change`, keyboard and mouse events, point-in-box hit testing, and semantic `EditAction` text editing
- **Accessibility** — `build_a11y_tree` walks the laid-out tree into roles, names and rects
- **Viewport/DPI maths** — a single rounding authority (`physical_from_logical`) plus the `Viewport` type
- **HMR client** — reconnecting client thread, `HmrMessage` protocol on port 31313

## Installation

```bash
cargo add velox-renderer                # stub backend, no features
cargo add velox-renderer --features skia-native   # native Skia window
```

| Feature | What it turns on |
|---|---|
| `default` | Empty — the stub backend, no optional dependencies, fast to compile. |
| `skia` | Selects the Skia backend path: `BACKEND = "skia"`, `SelectedRenderer = skia_backend::SkiaRenderer`, and a compile-only `skia_backend` module with no native dependencies (so the API surface builds without Skia). |
| `svg` | `dep:resvg 0.48` — rasterisation of SVG images (`<img src="*.svg">`). |
| `skia-native` | The real backend: `dep:skia-safe 0.91.1`, `dep:winit 0.28`, `dep:raw-window-handle 0.5`, `dep:egl 0.2`, `dep:glow 0.12`, `dep:softbuffer 0.3`, plus `skia` and `svg`. Unlocks the window loops, GPU surface, offscreen PNG/RGBA rendering, and `skia_backend::SkiaRenderer` with a live `surface`. |

The always-on dependencies are `velox-core`, `velox-dom`, `velox-style`, `log 0.4`, `bytemuck 1`, `serde 1`, `serde_json 1.0`.

## Quick start

With no features enabled the crate is a stub you can still build against:

```rust
use velox_dom::{h, text};
use velox_renderer::{backend_name, build_a11y_tree};

let vnode = h("button", (), vec![text("Save")]);
assert_eq!(backend_name(), "stub");

let tree = build_a11y_tree(&vnode, 800, 600);
println!("{}x{} root", tree.root.rect.w, tree.root.rect.h);
```

With `--features skia-native`, `run_window_vnode_skia` opens a window and drives the render/event loop — a complete program using it lives in [`examples/counter`](https://github.com/fahimaloy/velox/tree/main/examples/counter).

## Documentation

- API reference: [docs.rs/velox-renderer](https://docs.rs/velox-renderer)
- Book: [Renderer](https://fahimaloy.github.io/velox/features/renderer.html) — backends, features and rendering

## Workspace crates

Velox is a workspace of six published crates. This crate is marked **(this crate)**.

| Crate | What it provides | Documentation |
|:---|:---|:---|
| [velox-core](https://docs.rs/velox-core) | Reactive primitives: signals, effects, lifecycle hooks | [docs.rs](https://docs.rs/velox-core) |
| [velox-dom](https://docs.rs/velox-dom) | `VNode` types, block + flex layout, positioning, overflow | [docs.rs](https://docs.rs/velox-dom) |
| [velox-style](https://docs.rs/velox-style) | CSS parsing, selectors, cascade; styles applied to `VNode` | [docs.rs](https://docs.rs/velox-style) |
| **[velox-renderer](https://docs.rs/velox-renderer)** · this crate | Rendering backends, event dispatch, hit testing | [docs.rs](https://docs.rs/velox-renderer) |
| [velox-sfc](https://docs.rs/velox-sfc) | Single-file component compiler (pest grammar) → Rust | [docs.rs](https://docs.rs/velox-sfc) |
| [veloxc](https://docs.rs/veloxc) | The CLI and dev server | [docs.rs](https://docs.rs/veloxc) |

## Status

Published on [crates.io](https://crates.io/crates/velox-renderer) — the badge above tracks the latest release.

Working today: Skia immediate-mode paint, event hit-testing, the `skia-native` windowed backend, offscreen RGBA/PNG rendering, and accessibility trees. Notes:

- `default` compiles to a stub backend with no native dependencies — opt in with `skia-native` when you're ready. Expect a heavy first build: the Skia stack is large.
- The render loop is immediate-mode: no retained tree, no previous frame to compare, no patch applier. See [`docs/internal/RECONCILER.md`](https://github.com/fahimaloy/velox/blob/main/docs/internal/RECONCILER.md).
- Falls back to headless rendering with `VELOX_HEADLESS=1` or when no compositor is available.

## License

Licensed under **MIT OR Apache-2.0**.

Copyright © FAHIM AHMED <fahimaloy@tutamail.com>
