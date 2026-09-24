# Task 15 report — Wire the UA stylesheet into every production render path

## Status

DONE_WITH_CONCERNS

The production renderer now applies the existing UA < author < inline cascade on all listed production paths. The three rasterizer entry points and the example proof harnesses apply the cascade once, and the new regression test pins UA defaults plus author/inline precedence.

## Implementation

### Renderer styling entry point

`velox-renderer/src/lib.rs` now exposes one renderer-side entry point:

```rust
pub fn style_vnode_with_hover<F>(vnode: &VNode, author: &Stylesheet, is_hovered: &F) -> VNode
where
    F: Fn(&str, &velox_dom::Props) -> bool,
```

It delegates to `velox_style::apply_with_cascade_with_hover`, which composes the existing UA stylesheet beneath the author stylesheet and preserves inline precedence. The entry point is public so the required `velox-renderer/tests/ua_cascade_render.rs` integration test can exercise the same entry point directly; the brief's example helper was described as private, but Rust integration tests cannot import private library items.

All ten production calls formerly using `apply_styles_with_hover` now use `crate::style_vnode_with_hover`, preserving each existing hover predicate:

- Skia normal window initial frame (`velox-renderer/src/lib.rs:928`).
- Skia normal window mouse click handler (`velox-renderer/src/lib.rs:1068`).
- Skia normal window redraw path (`velox-renderer/src/lib.rs:1226`).
- Skia HMR initial frame (`velox-renderer/src/lib.rs:1484`).
- Skia HMR target refresh (`velox-renderer/src/lib.rs:1547`).
- Skia HMR mouse click handler (`velox-renderer/src/lib.rs:1678`).
- Skia HMR redraw path (`velox-renderer/src/lib.rs:1785`).
- WGPU `recompute_from_vnode` (`velox-renderer/src/lib.rs:2332`).
- WGPU redraw reconciliation (`velox-renderer/src/lib.rs:2649`).
- WGPU text/frame path (`velox-renderer/src/lib.rs:2797`).

### Rasterizer helpers

`velox-renderer/src/skia_render.rs` now imports and uses `velox_style::apply_with_cascade` in:

- `render_vnode_to_raster_png` (`velox-renderer/src/skia_render.rs:603`).
- `render_vnode_to_rgba` (`velox-renderer/src/skia_render.rs:840`).
- `render_vnode_to_raster_png_with_scale` (`velox-renderer/src/skia_render.rs:877`). The scaled helper was corrected as well because all example proof harnesses use it and it previously omitted author styling entirely.

Each helper styles a freshly supplied VNode once, then lays out and paints that already-cascaded tree. `render_frame` retains its `sheet` parameter for API compatibility. Its documentation now explicitly records that paint reads the already-cascaded inline style attributes and performs no paint-time sheet lookup or second style application. This prevents a path that silently drops the UA layer.

### Example proof harnesses

Removed the author-only pre-application from all three proof harnesses:

- `examples/counter/tests/render_proof.rs`
- `examples/todo/tests/render_proof.rs`
- `examples/showcase/tests/render_proof.rs`

Each now passes the raw generated VNode to both rasterizer helpers, so the renderer owns the complete cascade exactly once. Existing assertions and `target/velox-render-proof/` output paths were retained.

### Regression test

Added `velox-renderer/tests/ua_cascade_render.rs` with two pure-layout tests:

1. An author-empty tree containing `h1` and `p` receives UA margins, giving the h1 a non-zero offset and creating sibling spacing.
2. An author `.title { margin: 3px 0; }` rule produces `y = 3`, while an inline `margin: 5px 0` on the same element produces `y = 5`.

The test uses the renderer entry point and does not require Skia.

## Validation

The following checks passed:

- `cargo fmt`
- `cargo fmt --check`
- `CARGO_INCREMENTAL=0 cargo build --workspace`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-style`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-dom`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-sfc`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-cli`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-renderer --features skia-native`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --workspace`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo build -p velox-example-counter -p velox-example-todo -p velox-example-showcase`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-example-counter --test render_proof` (2 passed)
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-example-todo --test render_proof` (3 passed)
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-example-showcase --test render_proof` (2 passed)
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo check -p velox-renderer --features wgpu`

The proof suites regenerated these files under `target/velox-render-proof/`: counter large/small, todo large/small/toggled, and showcase large/small.

The full workspace and native Skia sweeps had no test failures. Existing warnings remain for `SoftbufferPresenter::is_degraded`, and the WGPU check reports existing unused/dead-code warnings. An initial parallel native linker attempt failed with `collect2: fatal error: ld terminated with signal 7 [Bus error], core dumped`; rerunning the required native suite with `CARGO_BUILD_JOBS=1` passed.

## Shifted expectations

No existing test expectation shifted. The UA layer produced no failures in the full workspace, native renderer, or example proof suites, so no expectations were changed or suppressed. The new expectations are isolated to the new regression test and reflect the correct UA/author/inline values.

## Concerns

- The shared renderer styling entry point is public rather than private solely so the required integration test can import and exercise it. This is additive API surface; the brief explicitly required an integration test file but also suggested a private helper.
- No UA rule content was changed, and no concern was found requiring a change to `velox-style/src/ua.rs`.
