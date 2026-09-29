---
name: velox-phase
description: Use when working on a specific phase of the velox remediation plan. Knows all 6 phases, their tasks, gate commands, and the current status of each. Directs work to the right task with the right verification command.
---

# Velox Remediation Plan — Phase Reference

All current work is driven by `docs/plans/2026-09-28-velox-remediation-plan.md`. Read the relevant task in that plan before starting — it specifies the failing test to write, files to modify (with line numbers), the fix, the verification command, and the commit message.

---

## Phase 0 — Restore a green baseline (do first)

**Status: In progress.** Task 0.1 failing, 0.2 not done, 0.3 not done.

### Task 0.1 — Fix the failing scoped-style test

```bash
cargo test -p velox-cli --test scoped_style_merging
```

Fails at `velox-cli/tests/scoped_style_merging.rs:81`: "TodoInput's .btn-add must be scoped in the merged sheet."

**Fix:** Read `velox-sfc/src/codegen.rs` around `scope_css` and the child-style merge. The child component's own scope hash is not being applied when its rules are hoisted into the root stylesheet. Fix so each child's rules keep the CHILD's hash, not the root's.

Commit: `fix(sfc): scope a child component's style rules with the child's own hash`

### Task 0.2 — Apply rustfmt + clippy clean

```bash
cargo fmt --all
cargo fmt --all -- --check          # must be silent
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

11 fmt diffs. 88 clippy warnings (see `velox-build-verify` for breakdown).

Commit: `style: rustfmt + clippy clean across workspace`

### Task 0.3 — Reclaim disk

```bash
du -sh target test-app/tmp test-app tmp
rm -rf test-app/target tmp/velox_test_init_app/target
cargo-sweep --time 7 2>/dev/null || cargo clean --release
```

Expected: `target` ≈ 45 GB, `test-app` ≈ 3.3 GB, `tmp` ≈ 2.0 GB. Target ~50 GB reclaim.

---

## Phase 1 — Correctness: reactivity crashes, leaks, silent failures

**Status: Not started.** All three bugs have failing tests ready to write.

### Task 1.1 — Fix the re-entrant effect panic

Panic at `velox-core/src/signal.rs:71`: "RefCell already borrowed". Triggered by an effect that writes a signal it also reads.

**Status: SHIPPED, but the plan's diagnosis was WRONG.** There is no double-borrow: `IS_FLUSHING: Cell<bool>` already guarded re-entrancy and was checked *before* enqueue. The real defect was a **panic inside an effect body** leaving `IS_FLUSHING` stuck `true` — which silently kills all reactivity for the rest of the process. The shipped fix is not "take the body out of the `RefCell`": it is `FlushResetGuard` + `EffectScopeGuard` (`5fa0429`), which restore `IS_FLUSHING` **and** `CURRENT_EFFECT` on unwind. Do not reintroduce the take-and-restore design.

Known gap: the unwind guards must cover **both** `flush_queue` and `effect()`'s initial run, and the `STOPPED_EFFECTS` prune predicate (`weak.strong_count() > 0`) still has **no behavioural test** — an unconditional `stopped.clear()` would pass the suite today. Add one.

Tests: `velox-core/tests/signal_reentrancy_tests.rs` (two tests: self-invalidating effect, two-effect cycle).

### Task 1.2 — Monotonic effect id (not heap address)

**Status: SHIPPED** (`925fb69`). `ptr_id` (`signal.rs:29`, `as_ptr() as usize`) **no longer exists** — a modern grep for it returns nothing in `src/`. It used to make new effects inherit dead effects' stopped flags because tombstones in `STOPPED_EFFECTS` were never pruned; 50k stop cycles → fresh effect silently suppressed.

`AtomicU64` monotonic id on `EffectInner` replaced it. **Prune predicate is `weak.strong_count() > 0`** — NOT "still in `EFFECT_STORAGE`", because stop/`Drop` remove from storage in the same operation that records the tombstone. Prune at a threshold (1024), not on every set.

Tests to write: `velox-core/tests/signal_effect_id_tests.rs`.

Commit: `fix(core): identify effects by monotonic id, not heap address`

### Task 1.3 — Fix the `computed()` leak

`EFFECT_STORAGE` holds strong `Rc` — dropped computed still fires.

**Fix:** `Weak` in storage, `EffectHandle` as sole strong owner.

Tests to write: `velox-core/tests/signal_computed_drop_tests.rs`.

Commit: `fix(core): computed() releases its effect when the signal is dropped`

### Task 1.4 — Remove per-read subscriber sweep

`signal.rs:161` runs `subs.retain()` on every `get()` — O(n) on the hottest path.

**Fix:** Prune only inside `set()` and `enqueue_effect`, never on read. Single `iter().any()` for duplicate check in `get()`.

Benchmark to write: `velox-core/benches/signal_read.rs` (criterion).

Commit: `perf(core): stop sweeping subscribers on every signal read`

### Task 1.5 — Recursion depth guard

No depth limit in `velox-sfc` parser or `velox-dom` layout. Deeply nested input → stack overflow or 12s hang.

**Fix:** `MAX_TEMPLATE_DEPTH = 256` in `template_parse.rs`. Thread `depth: usize` through `at()` in layout, cap at reasonable limit.

Tests to write: `velox-sfc/tests/depth_limit_tests.rs`, `velox-dom/tests/layout_depth_limit.rs`.

Commit: `fix(sfc,dom): bound template and layout recursion depth`

### Task 1.6 — Investigate 8 Arc not-Send/Sync clippy warnings

Read each warning location. If `Arc<something-not-Send>`, that's a soundness smell. If graphics context handles, `Sync` is the bug (per rust-skia's own guidance — N10). Either fix or add comment-justified `#[allow]`.

---

## Phase 2 — Performance: fast and resource-frugal

**Status: In progress. The gate below was RE-BASELINED and the original number was
never right — do not chase it.**

Gate is now `layout_us` ≤ **400 µs @ 10 todos × 3 runs**, measured via
`cargo test -p velox-renderer --release --features skia-native --test frame_cost_bench -- --ignored --nocapture`.
The plan's `1947 → 200 µs` is superseded: it assumed 84.4% of layout time was style
re-parsing and that removing it took cost to **zero**. A cache replaces a ~100-150 ns
parse with a ~32 ns lookup, so 15272 lookups ≈ 489 µs is a **hard floor**. The
re-baseline is the *reachable* target, not the ideal one.

**Current measurement: 551.3 µs median — the gate still fails.** Shipped:
2.1a (`ab152c8`), 2.1b (`91f0abd`+`5db49e6`), 2.1c (`7de08a1b`, −72.0%).
2.4 is **WITHDRAWN** (surface and `rgba` target are already reused; the real cost is
bandwidth). 2.2 is **DEFERRED, not cancelled**, and 2.3 needs **re-scoping** — its
real per-frame cost is the cascade's `ua.rules.clone()` + `author.rules.clone()`, not
the 1.6 µs parse the plan measured.

Known remaining levers, in order of cost-to-payoff:
1. `max_content_size` / intrinsic-sizing subsystem — the actual fix for `2^depth`,
   and a **multi-week** task.
2. `style_lookup_str` still returns `Option<String>` (one heap alloc, ~17 calls per
   descent, 19.9–20.4% of layout time) even though the cache's value is already
   `Rc<str>`. Left alone deliberately: ~36 call sites, and the gate fails either way.

**A cache cannot solve the depth problem.** Probe and target share node +
`ContainingBlock` but differ in `avail_w`/`avail_h`, so any key including those
collides. If a lane proposes a layout cache as the `2^depth` fix, it has not read
the two descents apart.

### Task 2.1 — Split flex sizing roles (kills 2^depth)

`layout.rs:3760-3850`. Three `at()` calls per content-basis flex item, each re-descending the subtree. Spec (css-flexbox-1 §9.2) defines three distinct numbers: flex base size (unclamped, the only subtree descent), hypothetical main size (clamp), target main size (flex resolve + clamp).

**Fix:** Three-stage form — measure content basis once, then pure arithmetic for clamping and flex resolution.

Test to write: `velox-dom/tests/flex_probe_memo.rs`.

**CRITICAL:** Task 4.8 (min-size + auto minimum size) must follow immediately after 2.1 — the spec's base-size vs hypothetical-size ordering is shared, and implementing them in the wrong order is a subtle correctness bug.

Commit: `perf(dom): split flex base/hypothetical/target sizing — one subtree descent per item`

### Task 2.2 — Layout invalidation (largest architectural change)

No memoization exists. Every frame recomputes everything. Layout is 92% of per-keystroke cost at 10+ items.

**Fix:** Stable node identity (u64 counter on VNode), `LayoutCache` keyed on `(node_id, avail_w, avail_h, containing_block)`, style hash invalidation, wire previous frame through render loop.

**Highest-risk task in the plan.** A stale-cache bug is silent and visual. Gate: all golden + pixel tests stay green plus two new targeted tests. Must not start until 2.1 lands.

Commit: `perf(dom,renderer): cache layout per node and invalidate on change`

### Task 2.3 — Parse stylesheet once

`make_view()` returns `(VNode, Stylesheet)` and is called every frame. Stylesheet parse is only 1.6 µs but it's a per-frame allocation.

**Fix:** Split into `make_vnode()` and `build_stylesheet()` returning `Arc<Stylesheet>` computed once.

Commit: `perf(renderer): parse the stylesheet once, not per frame`

### Task 2.4 — Reuse paint buffer

Check whether the live loop already reuses its raster surface (read `RedrawRequested` arm in `lib.rs:1776`). If not, hoist surface into loop state, reallocate only on resize.

**NOTE:** `frame_cost_bench.rs` builds a fresh surface per call — its paint number overstates the real loop. Verify first.

Commit: `perf(renderer): reuse the raster surface across frames`

### Task 2.5 — Reduce per-node allocation

`Props` is `HashMap<String, String>` — two heap allocations per attribute plus a hashmap node. `VNode::Element` stores `tag: String`.

**Fix:** Intern tags as `&'static str` via lazy interner. Replace HashMap with sorted `Vec<(Arc<str>, Arc<str>)>` using small-vector for ≤8 attributes.

Commit: `perf(dom): intern tags and use small-vector props`

---

## Phase 3 — Hot reload

**Status: Partially shipped — 3.4 only.** `8ccdaa1` (`fix(cli): unblock the dev
build, reap every child, coalesce saves`) is in the tree and its compile-error
recovery test passes. Original framing below, for the tasks that are still open.

Current "HMR" = full `cargo build` + process restart on every edit. Window closes and reopens.

**3.3 is BLOCKED on 2.2** — state-preserving rerender needs stable node identity.
**Note the identity ruling, which constrains how 2.2 must be built:** add **no field
to `VNode`**. The objection was not cost (14 sites need edits, not 151) but design —
it would duplicate the existing `"key"` attribute, and `VNode` is pure data with no
per-node state. The cache key must be **content-addressed**
`(content_hash, avail_w, avail_h, ContainingBlock)`, threaded as a `u64` *alongside*
`compute_layout`. **NEVER key on `source_index`** — it is the child ordinal, so a
sibling insert silently shifts it and every later sibling reads the wrong entry.
Do not reuse `data-hover-id` either; it is a per-frame ordinal. Cross-frame state
uses the existing structural `path` (`preserve_input_state`).

### Task 3.1 — Replace polling with notify

400ms `read_dir` poll + 150ms blocking `sleep` debounce. Replace with `notify = "6"` `RecommendedWatcher`. Non-blocking drain-and-collapse debounce. **Must verify `target/` is excluded by test** (cargo writes thousands of files there — inotify exhaustion risk, N9). Surface `Err` instead of silent stop.

Also: classify change kind (`StyleOnly`, `TemplateOnly`, `Script`) — prerequisite for 3.2. A `<style>` edit must not trigger `cargo build`.

Test to write: `velox-cli/tests/watcher_tests.rs`.

Commit: `perf(cli): watch with notify instead of a 400ms polling scan`

### Task 3.2 — Style-only hot update

`<style>` edit → live stylesheet swap, no rebuild, no restart, state preserved. Emit per-component style hashes from `velox-sfc/src/codegen.rs`. New `HmrMessage::StyleUpdate { hash, css }` in `velox-renderer/src/hmr.rs`.

Test to write: `velox-renderer/tests/hmr_style_update.rs`.

Commit: `feat(hmr): apply style-only changes live, with no rebuild`

### Task 3.3 — Template-only rerender preserving state

`<template>` edit → rerender preserving component state and caret position. Depends on Task 2.2's node identity. `HmrMessage::Rerender { template_hash }`.

**Sequence after 2.2.**

### Task 3.4 — Fix full-reload fallback

- Kill the 2s stall: `send_hmr_reload` then `kill()` immediately, no wait.
- Guarantee no orphans on every exit path.
- Coalesce rapid saves into one rebuild.
- Recover from compile errors — keep watcher alive, print error, retry on next save.

Commit: `fix(cli): no 2s reload stall, no orphaned children, coalesced rebuilds`

### Task 3.5 — Enable CI on feature branches

`.github/workflows/ci.yml` triggers only on `[main, master, alpha]`. Work is on `fix/2A-flex-complete`, 93 commits ahead of origin — none of it runs CI. Change to `on: [push, pull_request]` for all branches. Add frame-budget gate (Task 2.7 equivalent).

---

## Phase 4 — HTML/CSS default-behaviour parity

**Status: In progress — re-derive the sub-task state from `git log` before trusting
this list.** Shipped: 4.2a (`50b0565`, the Tier-1 `ua.css` pass), 4.6 steps 1–2
(`538d2eb`), 4.7a (`de1e12f`), 4.8a (`2180585`). In flight: 4.2b (B/D/E).
**Never started: 4.1, 4.3, 4.4, 4.5, 4.7b, 4.8b, 4.9.**

The recurring lesson of this phase: **the plan's premises here were mostly wrong, and
each correction came from reading the source rather than the plan.** 4.1 cannot be
written as specified (`computed_for_tag` does not exist), 4.4's `min_width` claim was
a snake_case-grep artifact (see Task 4.8), and 4.7's prescribed carrier was a type
with no production callers.

### Task 4.6 — Decide `:key` and unify reconcilers

**Decision: MADE (`538d2eb`).** `:key` is formally declared a **non-goal**, and the
tradeoff is recorded in `docs/RECONCILER.md`. The competing-reconcilers defect is
closed: `reconcile_keyed_children` (the one documented as *wrong*) and its
`run_window_vnode` entry point are **deleted**; `velox-renderer/tests/reconcile_keyed_tests.rs`,
which asserted the stale behaviour as intended, is **deleted with it**.

Why not wire `diff::diff`: it needs stable node identity, which is Task 2.2, which
is deferred. Shipping a half-wired reconciler would recreate the original defect —
a `diff` module that looks live and is not. `velox-dom/src/diff.rs` remains
unwired and that is now a *recorded* state, not an accident.

Remaining: re-open only after 2.2 lands. Until then `diff.rs` is intentionally dead —
a reconcile pass will read it as a bug.

### Task 4.7 — Never silently ignore a parsed declaration (N1) — **SHIPPED as 4.7a (`de1e12f`)**

**The prescribed fix below was WRONG. Do not reinstate it.** It said to add
`unimplemented: Vec<&'static str>` to `ComputedStyle`. `ComputedStyle` has **zero
production callers** — the live path carries declarations in the merged style
**string** and the renderer never builds a `ComputedStyle`. A field on it is dead
on arrival: the plan and this skill both certified the field without noticing the
type is unreadable in the product.

**What shipped:** `velox-dom/src/style.rs` `pub const PARSED_BUT_UNRENDERED: &[(&str, &str)]`
sits immediately above `set_property`; `velox-cli/src/commands/lint.rs` reads it and
warns per name. Build-time table, no `ComputedStyle` change.

**The list is 10, not 3.** N1 named `transition`/`transform`/`box-shadow`; the audit
that followed found 7 more parsed-but-unrendered properties, including
`visibility` (so `visibility: hidden` hides nothing) and `border-style`/
`border-color` (read only via the `border` shorthand). **N1's "three" was itself
an undercount** — a per-property audit is required, do not trust a remembered count.

Rule: **invalid/unknown declarations are silently ignored (per MDN cascade spec).
Parsed-but-unrendered declarations are a bug** — they must be reported.

Remaining: 4.7b = actually implement the properties (remove a name from the table
when it gains a reader; that removal is the definition of done).

### Task 4.8 — Implement `min-width` and flex automatic minimum size (NEW — N2/N3)

**N2 in the original plan was FALSE — `min-width` is not inert.** The plan's
evidence was `grep -c 'min_width' velox-dom/src/layout.rs` → 0, which greps a Rust
*identifier*. The cascade stores lengths under **string keys**, so layout reads them
as `"min-width"` and the snake_case grep returns zero while the property is fully
live. `min-height` was likewise called "read only by a viewport heuristic" — it is
read by the same string-key path.

**A grep that looks for a name the code does not use is not evidence of absence.**
This is the 20th falsified premise in this programme, and the cheapest one to
catch: re-run the grep against the *actual key form* before believing a count of 0.

**Shipped as 4.8a (`2180585`)**, on the "Reuse" branch: the style-parse cache
already exposes a parsed clamp — `table_for(style)` → `len_full_in(&t, "min-width", &ctx) -> Option<i32>`
— so it was a key in an existing memo, not a 39th call site. Three caveats for
whoever picks this up again: `min` and `max` are **two separate keys**, not one
entry; `auto` → `None` must mean **no constraint, never 0**; and the box-sizing and
border helpers do **not** consult min/max today, so clamp ordering changes the
arithmetic.

**4.8b — the §9.2 base/hypothetical/target split — is still open** and is a design
task, not a mechanical one. `layout.rs` calls `at()` at 7 sites with **2 descents per
content-basis level**, so the blowup is `2^depth`, not the plan's `3^depth`. A layout
cache cannot help: the probe and the target share node + `ContainingBlock` but differ
in `avail_w`/`avail_h`, so a cache keyed on those would collide. See the Phase 2 gate
note below.

Tests: `velox-dom/tests/min_size_tests.rs`.

---

## Phase 5 — Rendering hardening, security, toolchain

**Status: Partially shipped — re-derive from `git log`.** Shipped: 5.4 in part
(`100f224`, `5707411`, `538d2eb`), 5.6 fully (`47af18d` pins the duplicated event-loop
arms, then `5bda43d` extracts them), 5.8 **Step 2** (`39b406d`).
**Never started: 5.1, 5.2, 5.3, 5.5, 5.7.**

5.8 is **decided — Option A, stay on the CPU raster surface**, and the user has not
revisited it. 949 tests assert EXACT BYTES; a `GrDirectContext` would break them, and
its GPU resource cache defaults to 256 MB with no limit configured anywhere. Any lane
that "optimizes" `new_raster` into a GPU context will break 949 tests — the
deliberation is commented at the `raster_n32_premul` call site in
`velox-renderer/src/skia_surface.rs` (`39b406d`). Read that comment before touching paint.

### Task 5.1 — Re-enable 16 ignored Skia tests

`#[ignore = "requires skia-native feature and GPU hardware"]`. `skia-native.yml` already runs them. Confirm green. Add pixel-determinism gate.

### Task 5.2 — Un-ignore remaining tests or justify each

`frame_cost_bench.rs:187` (`#[ignore = "measurement tool"]`) is legitimate. The 3 in `velox-sfc/tests/integration_compile.rs` (`#[ignore = "slow"]`) should run in CI.

### Task 5.3 — Coverage gate

`cargo llvm-cov --workspace --fail-under-lines 70` in CI.

### Task 5.4 — Sweep dead code

**Partially shipped.** Done: the wgpu backend (`100f224` + `5707411`) and the dead
reconciler (`538d2eb`).

Two corrections to the original text, both of which were premise errors:

- **`skia_gl.rs` is LIVE and must be KEPT.** It is gated on `skia-native`, the live
  feature, and is reachable through live test helpers. Only its *GPU* class is
  dormant, and for the original reason: no production frame path ever creates a
  `DirectContext` (consistent with 5.8 Option A). Deleting it would break the
  `skia-native` build.
- The wgpu backend was never **compiled at all** — `velox-renderer/Cargo.toml`
  `default = []`, and the only in-workspace consumer requests `["skia-native"]`
  alone. A prior audit (`task-5.5.2-audit.md`) concluded "dormant, safe to delete"
  for the right reason but stated the wrong one: it said `WgpuRenderer` "IS the
  selected renderer", when that `SelectedRenderer` alias is itself inside
  `#[cfg(feature = "wgpu")]`. **Re-check whether a "dead" module is *uncompiled*
  before sizing the risk** — uncompiled is a stronger and cheaper guarantee.

**Still open:** `presenter.rs:289` `is_degraded`, and the nine `#[allow(dead_code)]`
in `layout.rs`/`text_wrap.rs` (each gets a justifying comment or is removed).
Deleting 18 merged branches (`git branch --merged HEAD`) still needs explicit user
confirmation, including for `main`.

**Out of bounds for 5.4:** changing a live `ControlFlow` to `Wait` is 5.6/5.8
territory, never this task.

### Task 5.5 — Security review

1. `cargo audit` on `Cargo.lock` — never run.
2. Audit 18 `unsafe` blocks against three rust-skia bug classes (ownership, `Send` without `Sync`, `zeroed().assume_init()` for C++ types).
3. Path traversal in `init.rs`/`add.rs`.
4. Command injection at `Command::new("cargo")` sites.

### Task 5.8 — Decide rendering target (NEEDS USER CALL)

Option A: Stay CPU (recommended) — deterministic pixel tests, no device-lost, no 256 MB cache, works headless. Option B: Add GPU as opt-in — faster large paints, loses deterministic tests, needs cache limits + device-lost handling.

**Record decision in `docs/RENDERING.md` before Phase 2 measurement gates are trusted.**

---

## Phase 6 — Property-based testing and fuzzing

**Status: Not started.** Do after Phase 1+2 so proptest types model the fixed `VNode` and `ComputedStyle`.

### Task 6.1-6.4 — proptest strategies + properties

`proptest` for: layout never panics, layout is idempotent, cascade is order-independent, compiler never panics on arbitrary input.

### Task 6.5 — cargo-fuzz targets

`fuzz_sfc` (parse + codegen), `fuzz_css`. Nightly CI job, bounded corpus, `-max_total_time=300`.

### Task 6.6 — CI wiring

Commit `tests/proptest-regressions/` to version control — converts fuzz findings into permanent regression tests.

---

## Quick phase selection

- **Starting fresh today?** Phase 0, Task 0.1 (1 failing test, small fix, immediate green).
- **Want the biggest correctness impact?** Phase 1, Task 1.1 (re-entrancy panic — crashes running apps).
- **Want the biggest performance impact?** Phase 2, Task 2.1 (kills 2^depth — 12s → <50ms at depth 18).
- **Want the most visible user-facing fix?** Phase 4, Task 4.8 (`min-width` no-op) or Task 4.7 (silent-dropped properties).
- **Need a user decision first?** Phase 5, Task 5.8 (CPU vs GPU rendering target).
