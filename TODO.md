# velox debugging TODO (no functional fixes yet)

## Phase 1 — Confirm backend + rendering codepath
- [ ] Inspect `velox-renderer/Cargo.toml` and `velox-cli` dev/build logic to confirm which renderer features are enabled (skia-native vs stub).
- [ ] Inspect `velox-cli/src/commands/dev.rs` to see what cargo features are used for `test_app`.

## Phase 2 — Validate what VNode + styles are produced at runtime
- [ ] Locate `apply_styles_with_hover` in `velox-style` and confirm what it outputs into `VNode` (e.g., does it generate `props.attrs["style"]` inline?).
- [ ] Inspect whether `<style scoped>` is transformed into selectors with a scope id and whether that id is attached to element props.
- [ ] Add temporary debug logging (or env-gated) to dump the root VNode and any generated `style`/computed style attributes after `apply_styles_with_hover`.

## Phase 3 — Trace layout sizing causing children to disappear
- [ ] Inspect `velox-dom/src/layout.rs` for percentage-height handling (`height: 100%`) and root container sizing.
- [ ] Enable hit-rect overlay via `VELOX_DEBUG_HIT_RECTS=1` to see whether children have non-zero layout.

## Phase 4 — Align renderer expectations with style pipeline
- [ ] Review `velox-renderer/src/skia_render.rs` to see exactly what style fields it consumes (it appears to parse only `props.attrs["style"]`).
- [ ] Verify whether the stylesheet pipeline is intended to generate inline style strings, or whether renderer should consume computed style objects directly.

## Phase 5 — Produce final root-cause report + fix plan
- [ ] Based on evidence from runtime dumps + layout checks, identify the single root cause(s).
- [ ] Produce a comprehensive fix plan (implementation steps), but do not apply code changes yet.

