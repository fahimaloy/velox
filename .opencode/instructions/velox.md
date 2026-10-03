# Velox — Project Instructions for OpenCode Agents

> **Read this before any work on the velox workspace.** These instructions encode the project's structure, conventions, gate commands, and the remediation plan that drives all current work.

---

## What Velox Is

A Rust workspace (edition 2024, rustc 1.91.1) implementing a Vue SFC-syntax GUI framework:

```
.vx SFC file → velox-sfc (pest parser + codegen) → generated Rust in OUT_DIR/
                                                      → velox-renderer (Skia + softbuffer)
 velox-core  — signals, effects, lifecycle, v-model
 velox-sfc  — .vx parsing, template AST, codegen, component resolution
 velox-dom  — VNode, CSS cascade, flex+block layout engine
 velox-style — CSS parser, selectors, UA stylesheet (ua.css)
 velox-renderer — Skia rendering, events, HMR protocol, winit event loop
 veloxc  — velox init/build/lint/dev CLI
```

Three examples: `counter`, `todo`, `showcase`. The `velox` binary is already installed at `~/.cargo/bin/velox`.

Current branch: `fix/2A-flex-complete` — 93 commits ahead of origin/main. DO NOT merge to main without CI.

---

## The Remediation Plan Is the Source of Truth

All current work is driven by **`docs/plans/2026-09-28-velox-remediation-plan.md`** (1455 lines, 6 phases).

Before starting any task, read the relevant phase + task from that plan. The plan specifies:
- Exact failing test to write first (RED)
- Files to modify (with line numbers)
- The fix
- The verification command
- The commit message

**Do not improvise fixes.** The plan was produced by a research pass that verified every claim against velox source at the cited lines. Deviating from it without re-reading the cited source is how defects survive.

---

## Workspace Layout

```
velox/
├── velox-core/src/     — signal.rs (reactivity), lifecycle.rs, watch.rs, vmodel.rs
├── velox-sfc/src/      — sfc.rs, template_parse.rs (pest grammar.pest),
│                         template_codegen.rs, codegen.rs, component_resolver.rs
├── velox-dom/src/      — lib.rs (VNode, Props), layout.rs (compute_layout),
│                         style.rs (cascade, 133 properties), diff.rs (dead code)
├── velox-style/src/    — lib.rs (Stylesheet, cascade), ua.css, visual_effects.rs
├── velox-renderer/src/ — lib.rs (winit loop, HMR), skia_render.rs, presenter.rs,
│                         events.rs, hmr.rs, skia_surface.rs
├── veloxc/src/      — commands/dev.rs (watcher, HMR loop), commands/init.rs
└── examples/           — counter, todo, showcase
```

---

## Gate Commands (Run These to Verify Anything)

```bash
# Full suite — must be green before claiming anything done
cargo test --workspace --all-targets

# Release-frame benchmarks (the numbers that matter)
cargo test -p velox-renderer --release --features skia-native --test frame_cost_bench -- --ignored --nocapture

# Clippy gate
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings

# Format gate
cargo fmt --all -- --check

# Specific crate (faster, when you know what you touched)
cargo test -p velox-sfc
cargo test -p velox-dom
cargo test -p velox-core
cargo test -p veloxc
cargo test -p velox-renderer

# Scoped style merging test (Task 0.1 — currently failing)
cargo test -p veloxc --test scoped_style_merging

# Coverage (Phase 5 gate)
cargo llvm-cov --workspace --fail-under-lines 70
```

**Current baseline (from the remediation plan, verified this session):**
- 949 passed, 1 failed, 33 ignored
- `cargo fmt --check`: 11 diffs
- Layout: exponential 2^depth (18 levels = 12 seconds)
- Per-keystroke at 200 todos: 16.2 ms (62 fps) — layout is 92% of that cost
- `grep -rn 'memo|cache|dirty|invalidate' velox-dom/src/layout.rs` → nothing. No memoization exists.

---

## Before You Edit Anything

### 1. Run gitnexus impact analysis

The project's `AGENTS.md` mandates this. Run it via the gitnexus MCP or CLI:

```
impact({target: "symbolName", direction: "upstream"})
```

Or CLI:
```bash
# run from the repository root
node .gitnexus/run.cjs impact "symbolName" --direction upstream --repo .
```

Report callers, risk level, and execution flows. **MUST do this before editing any symbol.**

### 2. Check the remediation plan

Is the symbol you're about to edit covered by a task in the plan? If yes, follow the plan's steps exactly — don't improvise. If no, read the plan's Risk Register (end of doc) to see if your change touches a high-risk area.

### 3. `risk: UNKNOWN` is not low

GitNexus returns `risk: UNKNOWN` when it cannot resolve callers. This is **not** evidence the symbol is safe — it means the graph walk couldn't answer. Confirm with a text `grep -rn` before treating the symbol as safe to change or delete.

---

## Key Architectural Facts

### Reactivity (`velox-core/src/signal.rs`)
- `Signal<T>` — `Rc<RefCell<SignalInner<T>>>`, single-threaded
- `effect()` — pushes to `EFFECT_QUEUE` thread-local, flushed synchronously
- Three known bugs (all in the plan, Phase 1):
  1. **Re-entrant panic** (`signal.rs:71`): `flush_queue` holds `borrow_mut()` across the effect body; a `set()` inside the body re-enters and double-borrows. Fix: take body out of RefCell before calling.
  2. **Effect ID reuse** — **FIXED** (`925fb69`). It was `ptr_id` = `as_ptr() as usize`, and tombstones in `STOPPED_EFFECTS` were never pruned, so recycled addresses made new effects inherit dead effects' stopped flags (50k stop cycles → fresh effect silently suppressed). Replaced by a monotonic `AtomicU64` id on `EffectInner`; prune predicate is `weak.strong_count() > 0`, at a threshold of 1024. `ptr_id` no longer exists in `src/`.
  3. **Computed leak** (`signal.rs:23`): `EFFECT_STORAGE` holds strong ` Rc` — dropped computed still fires. Fix: `Weak` in storage, `EffectHandle` as sole strong owner.
- Reader path is O(n) in subscriber count (`signal.rs:161` — `subs.retain()` on every `get()`).

### SFC compiler (`velox-sfc/`)
- `grammar.pest` → pest 2.8 parser
- `parse_sfc()` → `SfcAst`
- `template_codegen.rs` → `VNode` tree
- `codegen.rs` → full Rust module emitted to `OUT_DIR/`
- `scope_css` / `scope_selector_list` — child component styles are hoisted with the **child's own** scope hash. **Task 0.1's diagnosis was WRONG**: this already worked (it landed in `feee06b`); the failing test was because a parent-owned `.btn-add` had *moved into* `Todos.vx`. **Do not revert that move** — velox has no `:deep()`/scope inheritance, so a child-scoped rule cannot style a parent-owned element; reverting restores a real styling bug. The 0.1 fix (`02a2e77`, `c2a3078`) is tests-only.

### Layout (`velox-dom/src/layout.rs`)
- `compute_layout(node, viewport_w, viewport_h)` — `i32` viewport (subpixel support planned, Task 4.4)
- Flex: content-basis items re-descend the subtree. `at()` has **7 call sites with 2 descents per content-basis level** ⇒ `2^depth`, *not* the `3^depth` the plan claimed. The probe's `LayoutNode` **is** the output, so only the probe descent could be removed — and that needs an intrinsic-sizing subsystem that does not exist (4.8b / `max_content_size`, multi-week).
- `at(depth)` recursion — no depth guard exists (Task 1.5). **NOTE: the parser half of Task 1.5 is already done** — depth was capped at `velox-sfc/src/template_parse.rs` (that parser is iterative, so it never had a stack risk). Only the layout half is open.
- `INLINE_BY_DEFAULT_TAGS` (`layout.rs:2042-2046`) — flat list; must be extended per-tag with justification (Task 4.3). `velox-style/tests/cascade.rs` already fails if `ua.css` and this list diverge, so **adding a tag means editing both in the same commit.**
- `min_width`/`min_height` are **live and enforced** (`2180585`), reached through the string-key clamp `table_for(style)` → `len_full_in(&t, "min-width", &ctx) -> Option<i32>`. The plan's "never enforced" claim came from `grep -c 'min_width'`, which greps a Rust identifier the code does not use — the cascade stores lengths under **string keys**. **A grep in the wrong name form is not evidence of absence.**
- `diff.rs` is still unwired, but that is now a **recorded decision** (`docs/RECONCILER.md`, `538d2eb`): `:key` is formally a non-goal until 2.2 lands, and the competing reconciler `reconcile_keyed_children` plus `run_window_vnode` were **deleted** along with the test that asserted their wrong behaviour.

### Style (`velox-style/`, `velox-dom/src/style.rs`)
- **The parsed-but-never-rendered list is 10, not 3** (N1's "three" was an undercount). `velox-dom/src/style.rs` `pub const PARSED_BUT_UNRENDERED: &[(&str, &str)]` is the single source of truth, sitting immediately above `set_property`; `velox lint` reads it and warns per name (`de1e12f`). It includes `visibility` (**so `visibility: hidden` hides nothing**) and `border-style`/`border-color` (read only through the `border` shorthand). Removing a name from that table is the definition of done for implementing it.
- **`ComputedStyle` has ZERO production callers.** The live path carries declarations in the merged style **string**; the renderer never builds a `ComputedStyle`. Do not design a fix that stores state on `ComputedStyle` — it is dead on arrival. (4.7a's original brief did exactly that; `velox-phase` records the correction.)
- **`font-family: monospace` is DEAD.** `GenericFamily` (`velox-style/src/fonts.rs`) has zero consumers outside its own parser and tests; it is re-exported at `velox-style/src/lib.rs` and never read by the renderer.
- **`font-size` in `em`/`rem`/`%` is a silent no-op at BOTH ends** — `velox-renderer/src/skia_render.rs` `parse_px_value` and `velox-dom/src/layout.rs` `inline_font_size` both strip only `px`. But `Length::Em` **is** resolved for the box model, so `em` *margins and paddings* are live. Also live-but-broken: `font-weight: bolder` renders non-bold (4.2b commit B), `text-decoration: line-through` is never painted, and `font-style` had **no `set_property` arm at all** despite being listed in `velox-style/src/lib.rs` `INHERITABLE` — dropped twice (4.2b commit D).
- `parse_border_shorthand` **fabricates** `BorderStyle::Solid` + `Color::BLACK` when no style is given, so plain `border: 1px` paints a solid black border. CSS's initial `border-style` is `none` (4.2b commit E).
- `ua.css` Tier 1 **shipped** (`50b0565`): `h3`-`h6`, `pre`, `blockquote`, `hr`, `fieldset`, `figure`, `dl`/`dt`/`dd`, `address`, `sub`/`sup`, `mark`, `big`/`small`, `s`/`del`/`strike`, italic and bold and monospace families, `center`. **Still missing: tables** (no table layout algorithm exists) and form-control `box-sizing`/`textarea { white-space: pre-wrap }`.
- **Invariant that catches silently-dead declarations:** a declaration may not be added to `ua.css` unless it has a `set_property` arm **and** is consumed by either `parse_text_style` (`velox-renderer/src/skia_render.rs`, exactly nine keys) or `compute_layout`. This one rule would have caught most of the false premises in this programme.
- Deliberate deviation — do **not** "fix": `meter` and `progress` are `inline-block` in Blink but `inline` in velox, argued at `layout.rs:2029-2033` and mirrored in `ua.css`. `button`/`input`/`select`/`textarea` are `block`, not `inline-block`. Both are documented choices, not oversights.

### Renderer (`velox-renderer/`)
- **CPU raster path only.** `skia_surface.rs` creates `sk::surfaces::raster_n32_premul` with `_gpu_ctx: None`. No `DirectContext`. No resource cache limits configured (latent risk if GPU is added — Skia defaults to 256 MB).
- `frame_cost_bench.rs` builds a fresh raster surface per call — it **overstates** paint. Task 2.4 is **WITHDRAWN**: the live loop already reuses the surface and the `rgba` target. Do not re-file it. The bench's 1.6 ms "paint+readback" is CPU rasterization plus a full-surface readback, so it is not comparable to a GPU path.
- `ControlFlow::Wait` is correct (winit docs: "ideal for non-game applications"). Do NOT switch to `Poll` — that burns CPU continuously.
- `CaretBlinkTicker` (530ms, focus-gated, correct `Drop`) is the pattern for time-based work.
- HMR: the **reload path** is fixed (`8ccdaa1` — no 2 s grace stall, every child reaped on every exit path, rapid saves coalesced into one rebuild, compile errors recovered on the next save). The **watcher is still a 400 ms polling scan** with a blocking `sleep` debounce; replacing it with `notify` is Task 3.1, **never started**. So: do not re-fix 3.4, and do not assume a filesystem watcher exists.
- There is **no style-only or template-only fast path yet** (3.2/3.3). Every edit still does a full `cargo build` + relaunch. 3.3 is blocked on Task 2.2's node identity.

### CLI dev server (`veloxc/src/commands/dev.rs`)
- `changed_file()`: 400ms `read_dir` poll + `metadata().modified()`.
- `sleep(150ms)` blocking debounce at line ~467 — stalls command handling.
- Every change → full `cargo build` (lines ~350, ~381).
- `send_hmr_reload` → wait up to 2s → `child.kill()` → `spawn_app_hmr`. The 2s stall is the dominant reload cost (Task 3.4).

---

## .vx File Syntax

```vue
<script setup>
  let count = ref!(0);
  fn on_increment() { count.set(*count + 1); }
</script>

<template>
  <button @click={on_increment}>Count: {count}</button>
</template>

<style>
  button { padding: 8px 16px; background: #4a90d9; color: white; border-radius: 4px; }
</style>
```

Directives: `v-if`, `v-else-if`, `v-else`, `v-for`, `@event={handler}`, `:attr={expr}`, `{{ expr }}`.
Reactive primitives: `ref!()`, `signal!()`, `define_emits!()`, `on_mounted!()`.

---

## Coding Conventions

- Reactive primitives: `ref!()`, `signal!()`, `define_emits!()`, `on_mounted!()`
- Template interpolation: `{variable}` (not `{{ }}` in the actual syntax — CLAUDE.md shows `{{ }}` but the `.vx` example uses `{...}`)
- Event handlers: `@click={handler}`
- Attribute binding: `:src={expr}`
- Test files live in `crate/tests/` as standalone `.rs` files with `#[test]` functions (not inline `#[cfg(test)]` modules — except `velox-sfc` which has both)
- `#[cfg(test)]` inline modules are used heavily in `velox-sfc/src/` — count them when assessing unwrap/panic counts

---

## Commit Conventions

Follow the remediation plan's commit messages exactly — they're structured for the changelog:

```
fix(core): a re-entrant effect write no longer double-borrows its RefCell
fix(core): identify effects by monotonic id, not heap address
fix(core): computed() releases its effect when the signal is dropped
perf(core): stop sweeping subscribers on every signal read
perf(dom): split flex base/hypothetical/target sizing — one subtree descent per item
perf(dom,renderer): cache layout per node and invalidate on change
perf(renderer): parse the stylesheet once, not per frame
perf(renderer): reuse the raster surface across frames
perf(dom): intern tags and use small-vector props
perf(cli): watch with notify instead of a 400ms polling scan
feat(hmr): apply style-only changes live, with no rebuild
fix(cli): no 2s reload stall, no orphaned children, coalesced rebuilds
fix(sfc): scope a child component's style rules with the child's own hash
style: rustfmt + clippy clean across workspace
```

---

## What NOT to Do

- **Do not** switch `ControlFlow::Wait` to `Poll` — it burns CPU continuously. See plan N8.
- **Do not** "fix" `meter`/`progress` from `inline` to `block` — they're deliberately `inline` (velox has no `inline-block` layout). See plan N6.
- **Do not** delete `diff.rs` — it's complete and correct, just not wired in yet. It's the basis for Task 4.6.
- **Do not** `#[allow]` the 8 `Arc` not-Send/Sync clippy warnings without reading them first. If they're graphics context handles, `Sync` is the bug. See plan Task 1.6/N10.
- **Do not** add a GPU context without setting `DirectContext` resource limits — Skia defaults to 256 MB and can double that. See plan N4/Task 5.8.
- **Do not** silently ignore a parsed-but-unrendered CSS property. Add it to an `unimplemented` list and warn. See plan N1/Task 4.7.
- **Do not** commit without running `detect_changes` (gitnexus MCP) or `node .gitnexus/run.cjs detect-changes --scope all --repo .` first.

---

## Test Infrastructure

- **949 tests** across the workspace. Run with `cargo test --workspace --all-targets`.
- **Criterion benchmarks** in `velox-renderer/benches/` and `velox-renderer/tests/frame_cost_bench.rs`. Run with `--ignored` flag (they're marked ignored because they need `skia-native` feature).
- **Pixel-proof tests** (`todo_item_pixels.rs`, `caret_pixels.rs`, `presenter_pixel_bytes.rs`) — compare rendered pixels against golden values. These depend on the CPU raster path being deterministic.
- **Layout golden tests** (`layout_golden.rs`) — assert computed layout matches reference values.
- **proptest** not yet added (Phase 6 of the plan).
- **`cargo llvm-cov`** not yet configured (Phase 5 gate).

---

## Hermes Agent Integration

Hermes has gitnexus skills loaded (exploring, impact-analysis, debugging, refactoring, CLI, guide). When working through Hermes:

- Use `impact()` before any edit — it's faster than the CLI for symbol-level queries.
- Use `query({search_query: "concept"})` for architecture questions (e.g., "how does the flex sizing work?").
- Use `context({name: "symbolName"})` for a named symbol's full signature and usages.
- The gitnexus skills live at `.claude/skills/gitnexus-*`. They're also available to Hermes via the AGENTS.md-cli block in CLAUDE.md.
- `risk: UNKNOWN` from impact → confirm with text grep before treating as safe.

---

## Phase Status (as of this writing)

| Phase | Status |
|-------|--------|
| 0 — Green baseline | In progress. Task 0.1 (scoped style test) failing. Task 0.2 (fmt+clippy) not done. Task 0.3 (disk reclaim) not done. |
| 1 — Reactivity correctness | Not started. 3 bugs documented with failing tests ready to write. |
| 2 — Performance | Not started. 2^depth layout, no memoization, per-keystroke input lag all confirmed. |
| 3 — Hot reload | Not started. Current "HMR" is full process restart. |
| 4 — HTML/CSS parity | Not started. 3 parsed-but-dropped properties, `min-width` no-op, `ua.css` incomplete. |
| 5 — Rendering hardening | Not started. 16 ignored Skia tests, dead code, no coverage gate, no security audit. |
| 6 — Property-based testing | Not started. No proptest, no fuzzing. |

---

## Quick Reference: What to Do First

If you're picking up this workspace fresh:

1. **Task 0.1** — fix the scoped style merging test (`veloxc/tests/scoped_style_merging.rs:81`). Read the test, find the merge logic in `velox-sfc/src/codegen.rs`, fix so child rules keep the child's hash.
2. **Task 0.2** — `cargo fmt --all` then fix the 88 clippy warnings.
3. Then pick any Phase 1 task — they're correctness bugs that crash or silently break apps.
