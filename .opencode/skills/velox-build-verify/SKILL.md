---
name: velox-build-verify
description: Use when building, testing, linting, or verifying the velox Rust workspace after any code change. Knows the gate commands, the current baseline, and which tests are failing/ignored.
---

# Velox Build & Verify

Run these in order after any code change. Stop at the first failure and report it.

## 1. Format gate (fastest failure)

```bash
cargo fmt --all -- --check
```

Expected: no output. **The 11-diff baseline is GONE — Phase 0.2 shipped rustfmt, and the current baseline is 0 diffs.** A non-zero result is now your change.

If diffs: `cargo fmt --all` to apply, then re-run `--check`.

## 2. Clippy gate

```bash
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

**The 88-warning baseline is GONE — the current baseline is 0 warnings.** Phase 0.2 fixed the real lints; Task 1.6 (`6c42126`) resolved the 8 `Arc` not `Send`/`Sync` ones as justified `#[allow]`s (root cause: `Ref<T>(Rc<Signal<T>>)` is `!Send + !Sync` and renderer callbacks need only `'static`). Any warning now is yours.

Prefer real fixes over `#[allow]`. Any remaining `#[allow]` must carry a comment explaining why — and the comment's stated reason must be **true**.

## 3. Full test suite

```bash
cargo test --workspace --no-fail-fast
```

**Use `--no-fail-fast`.** Without it cargo stops at the first failing test binary and the remaining crates are never reported, so a total is impossible to reconcile.

Baseline at `de1e12f`: **1120 passed, 0 failed, 33 ignored** (workspace), and `velox-cli` alone sums to 57. **The old "949 passed, 1 failed" baseline is GONE.** Phase 0.1 shipped tests-only (`02a2e77`, `c2a3078`) and the suite is fully green.

**There is no longer a pre-existing failure to ignore.** If a test is red, it is your change or a genuine regression — report it, do not classify it as baseline.

## 4. Crate-specific (faster, when you know what you touched)

```bash
# Compiler + codegen
cargo test -p velox-sfc

# Layout + style
cargo test -p velox-dom
cargo test -p velox-style

# Reactivity
cargo test -p velox-core

# Renderer (no GPU feature — fast path)
cargo test -p velox-renderer

# CLI
cargo test -p velox-cli
```

## 5. Frame cost benchmark (the numbers that matter)

```bash
cargo test -p velox-renderer --release --features skia-native --test frame_cost_bench -- --ignored --nocapture
```

Reports per-frame cost breakdown: stylesheet parse, cascade, layout, paint+readback, full frame, fps ceiling. **The plan's table below is stale on the layout column** (superseded by `7de08a1b`, Task 2.1c, −72.0%):

| todos | stylesheet | cascade | layout | paint+readback | full frame | fps ceiling |
|-------|-----------|---------|--------|---------------|------------|-------------|
| 0     | 1.6 µs    | 46 µs   | 293 µs | 1574 µs       | 1913 µs    | 523         |
| 10    | 1.6 µs    | 110 µs  | ~~1947 µs~~ → **551.3 µs** | 1651 µs       | 3708 µs    | 270         |

**The Phase 2 gate is `layout_us` ≤ 400 µs @ 10 todos × 3 runs, and it currently FAILS at 551.3 µs median** (runs: 543.3 / 556.7 / 551.3; spread 2.4%). Do not restate the gate to make it pass. The plan's original `1947 → 200 µs` target was arithmetically impossible: it assumed the style re-parse cost went to ZERO, but a cache replaces a ~100–150 ns parse with a ~32 ns lookup, so 15272 lookups ≈ 489 µs is a hard floor. The remaining levers are recorded in `velox-phase`.

**Layout depth scaling is `2^depth`, not "three descents".** `at()` has 7 call sites with 2 descents per content-basis level. The plan's "18 nested levels = 11,978 ms" has **not been re-measured since 2.1a reordered flex** — re-measure before citing it. Note that a layout cache **cannot** fix the depth problem: the probe and target descents share node + `ContainingBlock` but differ in `avail_w`/`avail_h`, so they are distinct cache entries. Removing the probe descent needs an intrinsic-sizing subsystem (`max_content_size`) that does not exist yet.

## 6. Scoped style merging — was red, now green, and the fix is a trap

```
cargo test -p velox-cli --test scoped_style_merging
```

**This test used to fail and was "fixed" tests-only in `02a2e77`/`c2a3078`. It is now green. Do NOT reintroduce the "fix".**

The original Task 0.1 diagnosis — that child component styles were hoisted with the *root's* scope hash — was **wrong**; child hashing already worked (`feee06b`). What actually happened is that a parent-owned `.btn-add` had legitimately **moved into `Todos.vx`**, and velox has **no `:deep()`/scope inheritance**, so a child-scoped rule cannot style a parent-owned element. Reverting that move restores a real styling bug.

## 7. Coverage gate (Phase 5)

```bash
cargo llvm-cov --workspace --fail-under-lines 70
```

Not yet configured — Phase 5 gate. Uncovered public surface to target: `velox-core/src/ergonomics.rs` (504 lines), `velox-renderer/src/{presenter,event_binding,text,viewport,hmr}.rs`, `velox-style/src/{visual_effects,fonts}.rs`, `velox-cli/src/commands/{add,lint,init}.rs`.

## 8. Release build (when you need to confirm it compiles optimized)

```bash
cargo build --workspace --release
```

## What NOT to do

- Do NOT claim a change is verified without running the relevant gate commands above.
- Do NOT run frame_cost_bench without `--release --features skia-native` — the numbers are meaningless in debug.
- Do NOT trust `cargo test -p velox-renderer` alone for rendering changes — run the pixel-proof tests too: `cargo test -p velox-renderer --test todo_item_pixels --test caret_pixels --test presenter_pixel_bytes`.
- Do NOT run the 16 ignored Skia tests without the `skia-native` feature — they're `#[ignore = "requires skia-native feature and GPU hardware"]`.
