# Rendering Pipeline

How Velox turns state into pixels, and why the pipeline looks the way it does. This page records the *rationale* and the *edge mechanisms* behind the live render loop in `velox-renderer`.

> For the stage-by-stage walkthrough of an actual frame, the presenter contract, buffer reuse, per-frame allocations, and the font cache, see [Renderer](../features/renderer.md). That page is the canonical reference; this one only covers what it does not.

## The frame, end to end

Every frame follows the same path:

1. **View** — the renderer calls `make_view(w, h)` with the current *logical* viewport dimensions (see the [Viewport Contract](viewport.md)).
2. **Style & layout** — the `VNode` tree is styled and laid out against the viewport (`compute_layout`).
3. **Paint** — Skia draws the laid-out tree into a raster surface.
4. **Present** — the surface's pixels are copied into a staging buffer and handed to `softbuffer`, which presents them to the OS window.

## The rendering target is a CPU raster surface — deliberately

The surface is `sk::surfaces::raster_n32_premul`, created by `SkiaSurface::new_raster` (`velox-renderer/src/skia_surface.rs:65`). There is no GPU context on the live path.

This is a considered choice, not a limitation to work around:

- **Determinism.** The pixel tests in `velox-renderer/tests/` compare rendered bytes. A CPU raster path is deterministic; a GPU path forfeits that. Any future GPU backend must be a separate feature-gated backend that runs the pixel tests in a tolerance-based mode.
- **Resource safety.** A GPU context requires setting Skia's `GrDirectContext` resource-cache limit explicitly — Skia defaults it to 256 MB and can occupy roughly twice that when full. Enabling a GPU context without that limit is a resource-exhaustion risk, not an optimization.

The accepted cost: CPU rasterization is slower for large paints, and `raster_n32_premul` has no subpixel text antialiasing, so Velox text will never match a browser pixel-for-pixel.

The surface is allocated **once per window** — in the normal loop at `velox-renderer/src/lib.rs:1907` and the HMR loop at `velox-renderer/src/lib.rs:2569`. The only post-setup reallocation is the `else` path of `resize` (`velox-renderer/src/skia_surface.rs:175`).

## Two invalidation mechanisms, not one

Resize and scale-factor changes are handled by two separate mechanisms, both correct:

- **Physical size** — `ResizeState` (`velox-renderer/src/lib.rs:91-95`) coalesces the burst of `Resized` events a drag produces and keeps the committed logical baseline. Only the last event of a frame materializes, and `renderer.resize` then reallocates the surface (`velox-renderer/src/skia_surface.rs:175`).
- **Scale factor** — handled separately by the shared `window_scale_factor_changed` helper (`velox-renderer/src/lib.rs:1452-1480`), wired into both loops at `velox-renderer/src/lib.rs:2071` and `velox-renderer/src/lib.rs:2808`. It rescales the logical cursor position by `old / new` so the pointer does not jump, queues the new physical size, and calls `set_scale_factor` (`velox-renderer/src/skia_surface.rs:141` → `velox-renderer/src/viewport.rs:136`). It deliberately does **not** reallocate the pixel buffer: logical dimensions are recomputed from physical, and the actual recreate waits for the next `RedrawRequested`, so a scale change costs one layout, not one layout plus a surface rebuild.

## Reading the frame-cost benchmark

`velox-renderer/tests/frame_cost_bench.rs` measures `layout_us` and `cascade_us` directly; those numbers are valid, and its header (`velox-renderer/tests/frame_cost_bench.rs:1-24`) explains the tree it mirrors (nested flex containers, the case that makes layout expensive).

Its `frame_us` — and the derived `paint_us` residual — builds a fresh raster surface per call via `render_vnode_to_rgba`, a path the real loop does not run and that has zero production callers. The bench therefore **overstates** total frame cost. Treat `layout_us` and `cascade_us` as evidence; treat `paint_us` as a label on a number you should not quote.

## See also

- [Renderer](../features/renderer.md) — the frame pipeline table, presenter, and buffer lifecycle.
- [Viewport Contract](viewport.md) — logical vs physical size and scale.
