# Renderer

`velox-renderer` is the crate that paints. It takes the cascaded VNode tree your `<template>` and `<style>` blocks produce, lays it out, draws it with Skia, and presents the pixels to a native window through softbuffer — all inside a winit event loop. Everything heavy is feature-gated: with no features enabled the crate compiles to a fast stub, and you opt in to the real backend when you are ready.

![The renderer painting the layout showcase example — spacing, flex row and column, overflow scrolling, inherited text alignment, and wrapping widths, all resolved from a `.vx` component](../assets/showcase-window.png)

---

## The rendering pipeline

Every frame runs the same pipeline — first frame, resize, or repaint alike:

| Stage | Where | What happens |
|:---|:---|:---|
| 1. View function | your `make_view` closure | Builds the VNode tree + stylesheet from the current logical size |
| 2. Hover tagging | `with_hover_ids` | Tags every element with a `data-hover-id` |
| 3. Cascade | `style_vnode_with_hover` | Applies UA < author < inline (see [Styling](styling.md)) |
| 4. Input attributes | `inject_input_caret_attrs` | Text inputs get caret/focus attributes — unfocused defaults on frame one |
| 5. Layout | `velox_dom::layout::compute_layout` | Flexbox/table layout against logical `w×h` |
| 6. Scroll + targets | `apply_scroll_offsets`, `recompute_targets` | Applies scroll offsets; recomputes click/hover/input hit targets |
| 7. Paint | `skia_render::skia_impl::render_frame` | Skia draws the tree into the raster surface |
| 8. Present | `SoftbufferPresenter::present` | Copies the finished pixels to the window |

The first frame renders **before** the event loop starts, so the window has content even on platforms where an early `request_redraw()` does not produce a `RedrawRequested` (certain Wayland/X11 compositors).

### Entry points

```rust
// Paint a window whose contents are produced by a view closure.
run_window_vnode_skia(
    title: &str,
    make_view: F,   // FnMut(u32, u32) -> (VNode, Stylesheet)
    on_event: G,    // FnMut(&str, Option<&str>)
    get_title: H,   // FnMut() -> String
) -> Result<(), String>

// HMR variant — same contract plus a receiver for HmrMessage values.
run_window_vnode_skia_with_hmr(
    title: &str,
    make_view: F,
    on_event: G,
    get_title: H,
    hmr_rx: Arc<Mutex<Receiver<HmrMessage>>>,
) -> Result<(), String>
```

Both live behind `#[cfg(feature = "skia-native")]`. The event loop parks on `ControlFlow::Wait` — a UI application should not burn a core polling, and `Wait` is what keeps idle CPU at zero.

### The `make_view` contract

`make_view` is `FnMut(w: u32, h: u32) -> (VNode, Stylesheet)`, where `(w, h)` are **logical viewport dimensions** — `Viewport::from_i32(physical, scale).logical_size()`. The renderer calls it once on startup and again on every `RedrawRequested`, resize, and scale-factor change with the *current* logical `w,h`.

Templates must not ignore the dimensions (`|_w, _h|` defeats reflow). The canonical responsive pattern is a viewport-filling root — `width: 100%; min-height: 100vh` on `.app` — which reflows visibly on every window resize:

```html
<style scoped>
.app { width: 100%; min-height: 100vh; }
</style>
```

Root normalization is the safety net: `velox_dom::layout::root_is_viewport_filling` ensures the *first* VNode always fills the logical viewport even without explicit sizing. A root sized `100%`, `100vw`, `100dvw`, `100vh`, `100dvh`, or `min-height: 100%`/`100vh`/`100dvh` counts as viewport-filling.

---

## Viewport handling

The Skia surface, the softbuffer presenter, and the window loop share one `Viewport` type — the single source of truth for physical size, logical size, and scale:

```rust
pub struct Viewport {
    pub physical: PhysicalSize, // window inner size, clamped to >= 1
    pub logical: LogicalSize,   // (physical / scale).round().max(1)
    pub scale: f32,             // device pixel ratio; invalid values fall back to 1.0
}
```

| Rule | Detail |
|:---|:---|
| Physical clamp | Width/height clamped to a minimum of `1` — a zero-sized window never reaches Skia |
| Logical derivation | `(physical / scale).round().max(1)` per axis |
| Invalid scale | Non-finite or `<= 0.0` scale falls back to `1.0` |
| Single rounding | `physical = (logical × scale).round()` — one canonical forward mapping, so fractional scales never produce 0.25px subpixel edges |
| Pixel-grid snap | `snap_logical_to_physical_grid(value, scale)` aligns coordinates to device pixels after `canvas.scale(scale)` |
| Default | `800 × 600` at scale `1.0` |

At scale `1.5`, an `800 × 600` physical window reports a `533 × 400` logical viewport. Resize hooks fire only after a coalesced physical resize has committed *and* the logical size actually changed — a resize drag produces many `Resized` events, and only the last one per frame materializes. Scale-factor changes recompute *logical* dimensions from *physical* and deliberately do **not** reallocate the pixel buffer.

---

## Backends and feature flags

The crate builds nothing by default — `default = []`:

| Feature | Pulls in | What you get |
|:---|:---|:---|
| *(none)* | `velox-core`, `velox-dom`, `velox-style` only | A stub renderer; compiles fast, no window |
| `skia` | *(nothing — an API-surface switch)* | The Skia API surface compiles; no native backend yet |
| `skia-native` | `skia` + `skia-safe 0.91.1` (`gl`, `egl`), `winit 0.28`, `softbuffer 0.3`, `raw-window-handle 0.5`, `egl 0.2`, `glow 0.12`, `svg` | The real backend: windowed rendering with Skia + softbuffer |
| `svg` | `resvg 0.48` | SVG rasterisation for `<img src="logo.svg">` |

> Note: `svg` is split out of `skia-native` so a build that never paints SVG can skip resvg entirely; `skia-native` implies it to preserve current behaviour. resvg is built with `default-features = false` and only `raster-images` kept — the dropped font features (fontdb, rustybuzz, ttf-parser) exist to typeset SVG `<text>`, by far the largest build cost, spent on the one thing a logo never has. An SVG that embeds a PNG still resolves.

> Note: the platform crates build anywhere — `skia-safe`/`egl`/`glow` need no build script or C toolchain, and `softbuffer` supports Windows natively — so `skia-native` compiles on every target. GPU *acceleration* stays unix-gated; see [From paint to pixels](#from-paint-to-pixels-the-presenter).

Every scaffolded project enables the real backend: `veloxc init` writes `velox-renderer = { version = "0.1.2", features = ["skia-native"] }` into the generated `Cargo.toml`.

---

## From paint to pixels: the presenter

The rendering target is a **CPU raster surface** — `sk::surfaces::raster_n32_premul`, created once per window by `SkiaSurface::new_raster`. There is no GPU context, and that is deliberate: the pixel tests in `velox-renderer/tests/` compare rendered bytes, and determinism is what makes them possible. A GPU path would forfeit determinism and has no resource-cache limit set — enabling one today would be a resource-exhaustion risk, not an optimization. The accepted cost: CPU rasterization is slower for large paints, and `raster_n32_premul` has no subpixel text antialiasing, so Velox text will not match a browser pixel-for-pixel.

`SoftbufferPresenter::present` moves the finished frame to the window:

1. `read_pixels` copies the whole surface into velox's own staging buffer, requested in the surface's native `N32` order. On little-endian targets `N32` is `BGRA_8888` — exactly softbuffer's word order — so no swizzle pass exists.
2. `buffer_mut` maps softbuffer's buffer. The OS hands out a fresh mapping per frame; it is not velox's to cache.
3. One bulk `copy_from_slice` moves the staging bytes in — no per-pixel loop.
4. `buffer.present()` blits. On a broken compositor connection (`EPIPE`) the presenter degrades to a no-op: rendering still runs offscreen and the error surfaces once per session on stderr instead of crashing the event loop.

If the compositor is unavailable at all (CI, containers, SSH), headless mode kicks in: the window and event loop are still created, presentation is skipped, and rendering runs offscreen. Enable with `VELOX_HEADLESS=1` or let it auto-detect via `presenter::is_compositor_available()`.

---

## Performance notes

The buffers are already reused — do not "fix" them:

- The raster surface is allocated **once per window** at window setup, in both the normal loop (`velox-renderer/src/lib.rs:1907`) and the HMR loop (`velox-renderer/src/lib.rs:2569`). The only post-setup reallocation is the `else` path of `resize`.
- The staging buffer (`rgba: Vec<u8>`, `velox-renderer/src/presenter.rs:294`) is allocated once, inside `SoftbufferPresenter::new` (`velox-renderer/src/presenter.rs:308`), and only reallocated in `resize` when the dimensions actually changed.

What remains on the per-frame path is **bandwidth, not allocation**: three full-surface passes plus a clear — `canvas.clear`, `read_pixels`, and `copy_from_slice` (a ~1.92 MB memcpy at 800×600). Shrinking that is a damage-limited-repaint problem; a "reuse the buffer" change cannot address it.

> Note: `render_vnode_to_rgba` allocates a fresh surface *and* buffer on every call — it is the benchmark path and has zero production callers. The frame-cost bench header says so itself: the bench **overstates** total frame cost. Treat `layout_us` and `cascade_us` from the bench as evidence; the derived `paint_us` describes a path the product never runs.

Known per-frame allocations still on the live path, all in `velox-renderer/src/skia_render.rs`:

| Site | Cost |
|:---|:---|
| `ImageCache::load` + its per-frame recreation | A disk read and a full PNG/JPEG decode **per frame, per image** |
| `AdvanceKey` construction | A `String` allocation on every text-advance lookup — hit or miss |
| `RenderPaints::new`, `default_family()`, `child_family.to_string()` | Five heap-backed paints + small strings per frame / per element node |

The font cache, by contrast, is already handled: it lives in a thread-local and is taken-and-returned per frame, so the fontconfig scan behind `load_default_typeface` runs once per thread rather than once per frame.

---

## HMR channel

The dev server and a running app talk over a small TCP protocol, implemented in `velox-renderer/src/hmr.rs`:

- `velox_renderer::DEFAULT_HMR_PORT` is `31313` (`velox-renderer/src/hmr.rs:23`). The dev server listens on `127.0.0.1:31313`; apps connect to it.
- `hmr_config() -> Option<u16>` returns `Some(port)` when the process runs under the dev server (`VELOX_HMR=1`), reading the port from `VELOX_HMR_PORT` (defaulting to `31313`); otherwise `None`.
- `run_hmr_client(port, tx)` spawns a background thread that connects to the dev server, forwards `HmrMessage` values through a channel, retries every 500 ms when the connection is lost, and on `FullReload` **exits the process with code 0** — the dev server sees the child exit and restarts it with a fresh build.

Messages are newline-delimited JSON:

| Message | Meaning |
|:---|:---|
| `FullReload` | Rebuild and re-mount. The app exits with code 0; the dev server restarts it. |
| `HotReload { module_path }` | Re-execute a specific module. **Future use** — currently treated as a `FullReload`. |
| `KeepWindow` | Keep-alive no-op; lets the dev server probe the connection without restarting. |

The dev-side half of the story — watch, classify, rebuild, relaunch — is on [Dev Workflow & HMR](hmr-dev-workflow.md).

---

## See also

- [Template Syntax](template-syntax.md) — the VNode tree this renderer paints.
- [Styling](styling.md) — the cascade that runs in stage 3.
- [Dev Workflow & HMR](hmr-dev-workflow.md) — what triggers a rebuild while you edit.
