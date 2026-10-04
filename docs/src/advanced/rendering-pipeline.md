# Rendering Pipeline

How Velox turns state into pixels, and why the pipeline looks the way it does. This page describes the live render loop in `velox-renderer` and the measurements behind its design.

## The frame, end to end

Every frame follows the same path:

1. **View** — the renderer calls `make_view(w, h)` with the current *logical* viewport dimensions (see the [Viewport Contract](viewport.md)).
2. **Style & layout** — the `VNode` tree is styled and laid out against the viewport (`compute_layout`).
3. **Paint** — Skia draws the laid-out tree into a raster surface.
4. **Present** — the surface's pixels are copied into a staging buffer and handed to `softbuffer`, which presents them to the OS window.

## The rendering target is a CPU raster surface — deliberately

The surface is `sk::surfaces::raster_n32_premul` (`velox-renderer/src/skia_surface.rs:34`). There is no GPU context on the live path.

This is a considered choice, not a limitation to work around:

- **Determinism.** The pixel tests in `velox-renderer/tests/` compare rendered bytes. A CPU raster path is deterministic; a GPU path forfeits that. Any future GPU backend must be a separate feature-gated backend that runs the pixel tests in a tolerance-based mode.
- **Resource safety.** A GPU context requires setting Skia's `GrDirectContext` resource-cache limit explicitly — Skia defaults it to 256 MB and can occupy roughly twice that when full. Enabling a GPU context without that limit is a resource-exhaustion risk, not an optimization.

The accepted cost: CPU rasterization is slower for large paints, and `raster_n32_premul` has no subpixel text antialiasing, so Velox text will never match a browser pixel-for-pixel.

## Buffers are reused, not reallocated

A frame might look like it allocates; it doesn't. Both long-lived buffers are allocated once:

- **The raster surface** is allocated once per window, before the event loop starts (`velox-renderer/src/lib.rs:1377` for the normal loop, `:2015` for the HMR loop). The live `RedrawRequested` arm mutably borrows that one surface every frame. The only post-setup reallocation is the resize path (`velox-renderer/src/skia_surface.rs:145-152`).
- **The staging buffer** is Velox's own `rgba: Vec<u8>`, a field of `SoftbufferPresenter` (`velox-renderer/src/presenter.rs:214`). It is allocated once in `new` (`:283`) and reallocated only in `resize` (`:292`), which early-returns when the dimensions are unchanged. `present()` (`:338`) allocates no pixel buffer — the buffer it writes into (`surface.buffer_mut()`, `:380`) belongs to softbuffer/the OS, which hands out a fresh mapping per frame.

## What actually costs: bandwidth, not allocation

The remaining per-frame paint cost is **three full-surface passes plus a clear**:

1. `canvas.clear(sk::Color::TRANSPARENT)` — `velox-renderer/src/skia_render.rs:1576`
2. `read_pixels` copies the whole surface into the reused staging buffer — `velox-renderer/src/presenter.rs:376`
3. `copy_from_slice` moves the pixels into softbuffer's buffer — a 1.92 MB memcpy at 800×600 — `velox-renderer/src/presenter.rs:406-407`

Reducing this is a **damage-limited repaint** problem (only redraw what changed). It is not an allocation problem, and no amount of buffer reuse addresses it.

## Two invalidation mechanisms, not one

Resize and scale-factor changes are handled by two separate mechanisms, both correct:

- **Physical size** — `ResizeState` (`velox-renderer/src/lib.rs:92-95`) coalesces resize events, then `renderer.resize` reallocates the surface (`velox-renderer/src/skia_surface.rs:145`).
- **Scale factor** — handled separately at `velox-renderer/src/lib.rs:1531-1558` (`ScaleFactorChanged`; HMR twin `:2250`), which calls `set_scale_factor` (`:1551-1553` → `velox-renderer/src/skia_surface.rs:112` → `velox-renderer/src/viewport.rs:136-145`). This recomputes *logical* dimensions from *physical* and correctly does **not** reallocate the pixel buffer.

## Reading the frame-cost benchmark

`velox-renderer/tests/frame_cost_bench.rs` measures `layout_us` and `cascade_us` directly; those numbers are valid. Its `frame_us` (and the derived `paint_us` residual) builds a fresh raster surface per call — a path the real loop does not run — so the bench **overstates** total frame cost, as its own header notes (`:27-29`). Treat `layout_us` as evidence; treat `paint_us` as a label on a number you shouldn't quote.

## Known future work

Recorded so nobody rediscovers these from scratch. The remaining per-frame allocations live in `velox-renderer/src/skia_render.rs`:

- **Image cache** (`:672-680`, recreated at `:1592` every frame) — a disk read and full PNG/JPEG decode per frame, per image.
- **Text advance lookups** (`:1260`) — the `AdvanceKey` allocates a `String` before the cache hit check, on the hottest text path (57–150 calls/frame).
- **Lesser:** `RenderPaints::new()` (`:1604`, five heap-backed paints), `default_family()` returning a `String` (`:1603`), and a per-element `child_family.to_string()` (`:1636`).

The font cache, by contrast, is already handled correctly: it lives in a thread-local (`velox-renderer/src/skia_render.rs:1384-1389`), so the fontconfig scan behind `load_default_typeface` runs once per thread rather than once per frame.
