# Rendering: how velox paints, and why

This file records two decisions that are easy to "fix" by accident, plus the measurements that
produced them. Both were confirmed against source on 2026-09-28.

---

## 1. The rendering target is a CPU raster surface. This is deliberate.

**Decision: stay on the CPU raster path.** The plan flagged this as needing a ruling before any
paint performance work is tuned against it, because a later reversal invalidates the tuning.

The surface is `sk::surfaces::raster_n32_premul` (`velox-renderer/src/skia_surface.rs:34`). There is
no GPU context:

- `_gpu_ctx: None` — `velox-renderer/src/skia_surface.rs:41`
- `_gl_ctx: None` — `velox-renderer/src/skia_surface.rs:43`

**Do not "upgrade" this to a GPU context.** The pixel tests in `velox-renderer/tests/` compare
rendered bytes. Determinism is what makes them possible, and a GPU path forfeits it. If a GPU
backend is ever added it must be a separate feature-gated backend that runs the pixel tests in a
tolerance-based mode, and it must set Skia's `GrDirectContext` resource-cache limit explicitly —
Skia defaults that to 256 MB and can occupy roughly twice that when full. Velox currently sets no
limit, so enabling a GPU context today would be a resource-exhaustion risk, not an optimization.

Cost of the decision: CPU rasterization is slower for large paints, and `raster_n32_premul` has no
subpixel text antialiasing, so velox text will never match a browser pixel-for-pixel. That is
accepted.

### The GL path is dead code

The only assignments of `Some` to `_gpu_ctx` / `_gl_ctx` in the workspace are
`velox-renderer/src/skia_surface.rs:199`, `:201`, `:214`, `:216` — all inside
`create_window_surface_from_handle` (`:173`), which is reachable only from
`SkiaRenderer::with_window` (`velox-renderer/src/lib.rs:1031-1043`). **`with_window` has zero
callers.**

Three sites call a guarded `gl_ctx.make_current()`:

| Site | Function | Called from |
|---|---|---|
| `velox-renderer/src/skia_surface.rs:60` | `canvas()` (`:48`) | `velox-renderer/src/skia_render.rs:1575` — **once per frame** |
| `velox-renderer/src/skia_surface.rs:92` | `present()` (`:89`) | — |
| `velox-renderer/src/skia_surface.rs:132` | `resize` (`:124`) | `velox-renderer/src/lib.rs:1781`, `:2489` |

All three are `if let Some(gl_ctx) = &self._gl_ctx { let _ = gl_ctx.make_current(); }`, so they
are no-ops while `_gl_ctx` is `None`. The expensive branch they guard is
`velox-renderer/src/skia_gl.rs:46-47` (`egl::make_current`). **`canvas()` is on the per-frame
paint path, so enabling the GPU path turns this into a live per-frame `eglMakeCurrent` with no
other change.** `canvas()` also carries an `if false { }` block at
`velox-renderer/src/skia_surface.rs:50-56` — a marker that this path was started and abandoned.

---

## 2. The raster surface and the pixel buffer are already reused. Do not "fix" them.

A remediation plan proposed hoisting the raster surface into loop state and reallocating only on
resize, on the premise that "paint is still 1.6 ms of a 3.7 ms frame" and therefore there must be
a per-frame allocation to remove. **There is not.** Both buffers are already long-lived.

**The surface is allocated once per window.** There are exactly two `new_raster` calls in the
process, both at window setup before the event loop:

- `velox-renderer/src/lib.rs:1377` — normal loop
- `velox-renderer/src/lib.rs:2015` — HMR loop

The live `RedrawRequested` arm is `velox-renderer/src/lib.rs:1776-1857`. Line `:1796`
(`if let Some(s) = &mut renderer.surface {`) mutably borrows the one long-lived surface every frame,
and nothing between `:1796` and `:1855` creates one. The only post-setup reallocation is the `else`
path of `resize`, `velox-renderer/src/skia_surface.rs:145-152`.

**The staging buffer is allocated once, and it is velox's own.**

- `velox-renderer/src/presenter.rs:214` — `rgba: Vec<u8>`, field of `SoftbufferPresenter` (`:204-219`)
- `velox-renderer/src/presenter.rs:283` — the one and only allocation, inside `new` (`:228`)
- `velox-renderer/src/presenter.rs:320` — the one and only reallocation, inside `resize` (`:292`),
  which early-returns at `:298` when the dimensions are unchanged

`present()` (`velox-renderer/src/presenter.rs:338`) allocates no pixel buffer. Note that
`self.surface.buffer_mut()` at `:380` is **softbuffer's / the OS's** buffer, not velox's — the OS
hands out a fresh mapping per frame and that is not velox's to cache.

### What actually costs: bandwidth, not allocation

The remaining per-frame paint cost is **three full-surface passes plus a clear**:

1. `velox-renderer/src/skia_render.rs:1576` — `canvas.clear(sk::Color::TRANSPARENT)`
2. `velox-renderer/src/presenter.rs:376` — `read_pixels` copies the whole surface into the reused
   `rgba`
3. `velox-renderer/src/presenter.rs:406-407` — `dst.copy_from_slice(&self.rgba[..pixel_count * 4])`,
   a 1.92 MB memcpy at 800×600

Reducing this is a **damage-limited repaint** problem. It is not an allocation problem, and a
"reuse the buffer" change cannot address it.

### Two invalidation mechanisms, not one

A plan comment described resize and scale-factor changes as sharing "a coalescing path via
`ResizeState`". They do not. They are two separate mechanisms, both correct:

- **Physical size** — `ResizeState` (`velox-renderer/src/lib.rs:92-95`) coalesces, then
  `renderer.resize` reallocates. `velox-renderer/src/skia_surface.rs:145`
- **Scale factor** — handled separately at `velox-renderer/src/lib.rs:1531-1558`
  (`ScaleFactorChanged`; HMR twin `:2250`), which calls `set_scale_factor` (`:1551-1553` →
  `velox-renderer/src/skia_surface.rs:112` → `velox-renderer/src/viewport.rs:136-145`).
  This recomputes *logical* dimensions from *physical* and correctly does **not** reallocate the
  pixel buffer.

`DpiChanged` is not handled at all — `grep DpiChanged velox-renderer/src/lib.rs` returns nothing.

---

## 3. The frame-cost benchmark measures a path the product never runs

`velox-renderer/src/skia_render.rs:975-996` `render_vnode_to_rgba` allocates a raster surface at
`:981` **and** `let mut rgba = vec![0u8; (width * height * 4) as usize];` at `:991` on every call.
**It has zero `src` callers** — the only references in `velox-renderer/src` are its definition, a
doc comment, and a `pub use` re-export at `velox-renderer/src/lib.rs:674`.

The benchmark's own header says this correctly
(`velox-renderer/tests/frame_cost_bench.rs:27-29`): the bench "builds a fresh raster surface per
call, which the real loop does not; it therefore OVERSTATES total frame cost."

**Consequence for anyone reading the numbers.** `layout_us` and `cascade_us` are measured
directly and are valid. `frame_us` and the derived `paint_us` describe the dead path, and
`paint_us` is worse than useless as a gate: it is
`(frame_us - cascade_us - layout_us).max(0.0)` (`:188`), a *residual*, so it absorbs the error of
all three phases it subtracts — and it already contains the very allocation the real loop avoids.
Treat `layout_us` as evidence; treat `paint_us` as a label on a number nobody should quote.

---

## Known per-frame allocations not yet addressed

Recorded here so the next person does not rediscover them. All are in
`velox-renderer/src/skia_render.rs`.

- **`:672-680` `ImageCache::load`** — `std::fs::read(src)` + `sk::Data::new_copy` +
  `sk::Image::from_encoded` + `src.to_string()`, i.e. a disk read and a full PNG/JPEG decode **per
  frame, per image**. The cache is itself recreated at `:1592` (`let mut images = ImageCache::new();`)
  every frame. The comment at `:1046-1054` shows the author was already optimizing exactly this
  class of repeated work for text.
- **`:1260` `text: text.to_string()`** — the `AdvanceKey` is constructed at `:1257-1261`, *before* the
  `self.advances.get(&key)` hit check at `:1271`. Every lookup therefore allocates a `String`,
  whether it hits or misses. At the 57–150 calls/frame the `:1046-1054` doc cites, that is an
  allocation on the hottest text path in the renderer.
- Lesser: `:1604` `RenderPaints::new()` (5 heap-backed `sk::Paint`, `:629-635`); `:1603`
  `default_family()` returns a `String`; `:1636` `child_family.to_string()` runs once per
  **Element node** per frame, inside `render_with_layout`'s `VNode::Element` arm.
- The text advance cache itself is already handled correctly and is *not* in this list: the
  `FontCache` lives in a thread-local (`velox-renderer/src/skia_render.rs:1384-1389`) and is
  taken-and-returned per frame by the guard at `:1450`, so the fontconfig scan behind
  `load_default_typeface` runs once per thread rather than once per frame (rationale at `:1583-1589`).
