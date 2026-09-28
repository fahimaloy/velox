# Velox Framework — Remediation & Hardening Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Make velox fast, stable, resource-frugal, and behaviourally faithful to HTML/CSS/Vue defaults — fixing the exponential layout blowup, the reactivity panics and leaks, the missing hot reload, and the HTML-default parity gaps.

**Architecture:** Six sequential phases. Phase 0 restores a green build. Phase 1 fixes correctness bugs that crash or silently break the reactivity system. Phase 2 attacks layout cost, which dominates every frame and every keystroke. Phase 3 adds real hot reload. Phase 4 closes HTML/CSS parity. Phase 5 hardens rendering and the toolchain. Phase 6 adds property-based testing and fuzzing. Each phase ends with a measurable gate.

**Research basis:** this plan was revised after a deep research pass over the W3C flexbox spec (ED 8 May 2026), Blink's `html.css`, Vite's HMR docs, Vue's reactivity internals, the winit event-loop docs, Impeller/Flutter's architecture, and rust-skia's own ownership guidance. Findings that changed this plan are in **§ New findings from research** below and the full notes are gitignored at `.research/2026-09-28-deep-research.md`.

**Tech Stack:** Rust edition 2024, Cargo workspace (velox-core, velox-sfc, velox-dom, velox-renderer, velox-style, velox-cli), pest 2.8, Skia (skia-safe 0.91), winit 0.28, softbuffer 0.3, criterion.

---

## Critical corrections to the prior audit

These supersede findings from the first report. Each was re-verified in this pass.

| Prior claim | Correction | How verified |
|---|---|---|
| "velox-sfc has 102 `.unwrap()` and 14 panics reachable from user input" | **Only 3 production `.unwrap()` exist** (`expr.rs:121,537,544`). The other ~99 are inside `#[cfg(test)]` inline test modules. Production `panic!`/`unreachable!` across the whole workspace: 8 total. | Split each file at its `#[cfg(test)]` line and counted unwraps in the head. |
| "`expr.rs:121` can panic on a non-char-boundary slice" | **Not reachable.** `pos` is only ever advanced by `c.len_utf8()` (`expr.rs:122`); no other assignment exists. Confirmed a mid-char slice *would* panic, but the lexer cannot produce one. | Enumerated every `self.pos =` site; ran a boundary probe. |
| "Two competing reconcilers, `diff.rs` unused" | **Confirmed and still true** — the most significant dead-code finding. | `grep` for `diff` across all `src/`. |
| "102 unwraps vs velox-core's zero" asymmetry | The asymmetry is an artifact of test placement, not error-handling discipline. velox-core has no inline test modules, which is why it showed zero. | Same file-split method. |

The compiler is in considerably better shape than the first report implied. Effort should go to layout, reactivity, and HMR.

---

## Measured baseline (all numbers from this session, release builds unless noted)

**Per-frame cost** (`cargo test -p velox-renderer --release --features skia-native --test frame_cost_bench -- --ignored --nocapture`):

| todos | stylesheet parse | cascade | layout | paint+readback | full frame | fps ceiling |
|---|---|---|---|---|---|---|
| 0 | 1.6 µs | 46 µs | 293 µs | 1574 µs | 1913 µs | 523 |
| 10 | 1.6 µs | 110 µs | 1947 µs | 1651 µs | 3708 µs | 270 |

**Layout is exponential in nesting depth** (probe against real `compute_layout`, `frame_cost_bench.rs`'s exact stylesheet):

| depth | layout ms | ratio |
|---|---|---|
| 8 | 11.6 | 2.03 |
| 12 | 185.5 | 2.00 |
| 16 | 2956.3 | 1.99 |
| 18 | 11978.6 | 1.97 |

Clean **2^depth**. Eighteen nested elements = 12 seconds.

**Flex is 15–24× costlier per node than block** (64 items, 258 vnodes):

| shape | ns/vnode |
|---|---|
| plain block, no flex | 2,315 |
| flex, all widths declared | 35,715 |
| flex `flex:1` (content basis) | 40,266 |
| flex + wrapping text | 55,102 |

**Per-keystroke cost** (cascade + layout, one character typed):

| todos in list | frame ms | typing fps |
|---|---|---|
| 0 | 0.057 | 17,666 |
| 10 | 0.862 | 1,160 |
| 50 | 4.014 | 249 |
| 100 | 7.631 | 131 |
| 200 | 16.254 | **62** |

Layout is **92%** of per-keystroke cost at list sizes ≥10. This is the reported input lag, reproduced and explained.

**No memoization or invalidation exists** in `velox-dom`: `grep -rn 'memo|cache|dirty|invalidate' velox-dom/src/layout.rs` returns nothing.

**Test suite:** 949 passed, 1 failed, 33 ignored. **`cargo fmt --check`: 11 diffs.**

---

## New findings from research (2026-09-28 pass)

Full notes: `.research/2026-09-28-deep-research.md` (gitignored). These are **verified against velox source at the cited lines** unless marked as spec-derived.

### N1 — Three CSS properties are parsed and then silently dropped (HIGH)

`velox-dom/src/style.rs` accepts **133 properties**. Three are fully parsed into `ComputedStyle` fields and **never read by any renderer code path**:

| Property | Parsed at | Read by renderer? | Verified |
|---|---|---|---|
| `transition` | `style.rs:1587` → `transitions: Vec<Transition>` (`:1268`) | **NO** | `grep -rn 'transition' velox-renderer/src` → only an unrelated comment at `events.rs:711` |
| `transform` | `style.rs:1575` | **NO** | `grep -rn 'transform' velox-renderer/src` → one unrelated comment at `presenter.rs:365` |
| `box-shadow` | `style.rs:1582` | **NO** | `grep -rn 'box_shadow\|shadow' velox-renderer/src` → **zero** |

This is the "silent lie" failure mode, and it directly violates the requirement that "all styles must behave the same as the CSS." An author writing `transition: opacity 0.2s ease` gets no error, no warning, and no animation.

**Also unparsed entirely** (no `set_property` arm at all): `content`, `cursor`, `user-select`, `float`, `clear`, `aspect-ratio`, `writing-mode`. Of these `cursor` and `user-select` are UI-visible and were explicitly requested.

**Spec basis (MDN cascade reference):** invalid/unknown declarations are *filtered out* — silently ignored. Velox should match that for genuinely unknown properties, but must **not** silently ignore properties it *parses*. See new Task 4.7.

### N2 — `min-width` is parsed but never enforced in layout (HIGH)

```
$ grep -c 'min_width' velox-dom/src/layout.rs
0
```

`min_width` exists in `ComputedStyle` (`style.rs:1223`), is parsed at `style.rs:1329`, and **`velox-dom/src/layout.rs` never references it.** `min_height` appears only in a viewport-filling heuristic (`layout.rs:2989-3089`), not as a general clamp.

**Consequence:** `min-width: 200px` is a **no-op** in velox today. This is a far more common authoring pattern than `transition`, and it is a straightforward author-facing bug. New Task 4.8.

### N3 — The flexbox "automatic minimum size" rule is entirely absent

Per css-flexbox-1 §4.5, a flex item's `min-width: auto` resolves to its **content-based minimum size** (min-content, clamped by specified min/max). Velox has no such rule. This explains why the flex work repeatedly needed `min-width: 0` workarounds, and likely **contributed to the 2^depth blowup**: without a correct minimum-size rule, the probe cycle must re-descend the subtree to recompute content sizes. Pairs with N2 in Task 4.8.

### N4 — Velox renders on a CPU raster surface; there is no GPU context

`velox-renderer/src/skia_surface.rs`:
```rust
let surface = sk::surfaces::raster_n32_premul((w, h))   // CPU raster
_gpu_ctx: Option<sk::gpu::DirectContext>,   // always None
_gl_ctx:  Option<SkiaGlContext>,            // always None in new_raster
```

`canvas()` also calls `gl_ctx.make_current()` on **every accessor call** — a no-op while `_gl_ctx` is `None`, but a per-frame GL context switch if that path is ever enabled.

Consequences:
- The benchmark's 1.6 ms "paint+readback" is **CPU rasterization plus a full-surface readback**. Paint optimizations are being measured against a software path.
- No `DirectContext` means the **Skia GPU resource cache is never configured.** Per Skia's own docs it defaults to **256 MB** and when full "can actually occupy twice as much." `grep 'resource_cache|set_resource_limit|purge|memory_budget'` → **zero hits**. This is a latent resource-exhaustion risk the instant a GPU context is added.
- `raster_n32_premul` means no subpixel text antialiasing — expect text to differ visually from browsers.

**Decision (research recommendation, needs your sign-off):** keep the CPU raster path as the *deterministic reference* the pixel tests depend on, and treat any GPU path as a separate opt-in backend with explicit cache limits. This follows Impeller's stated priority — "predictable performance… The engine controls caching and caches explicitly" — and it protects the 949-test suite. See Tasks 2.4 and 5.8.

### N5 — `ua.css` covers 11 tags; Blink's `html.css` covers far more (HIGH)

Velox's `ua.css` has **9 rules / 11 tags**: `html, body, button, p, ul, ol, u, var, wbr`. Elements with **zero** UA rules in velox that are near-universal in real markup:

| Element | Browser default (Blink/WHATWG) | velox today |
|---|---|---|
| `h1` | ✅ **already present** — `font-size:2em; margin:0.67em 0; font-weight:bold; display:block` | correct |
| `h2` | ✅ **already present** — `font-size:1.5em; margin:0.83em 0; font-weight:bold` | correct |
| `h3`–`h6` | `1.17em / 1em / 0.83em / 0.67em`; margins `1em / 1.33em / 1.67em / 2.33em`; `font-weight:bold` | **none — parent size, normal weight** |
| `pre` | `font-family:monospace; white-space:pre; margin-block:1em` | none — **preformatted text silently wraps** |
| `blockquote` | `margin-block:1em; margin-inline:40px` | none |
| `hr` | `border-style:inset; border-width:1px; margin-block:0.5em; margin-inline:auto` | none |
| `fieldset` | `border:groove 2px; padding-inline:0.75em; margin-inline:2px` | none |
| `table`/`thead`/`tbody`/`tr`/`td`/`th`/`caption` | `display:table; border-spacing:2px; border-collapse:separate`; `td` padding `1px`; `th` bold+center | none — **no table layout at all** |
| `dl`/`dt`/`dd` | `dl` `margin-block:1em`; `dd` `margin-inline-start:40px` | none (display values happen to be right) |
| `sub`/`sup` | `vertical-align:sub/super; font-size:smaller` | no UA rule |
| `mark` | `background-color:Mark; color:MarkText` | none |
| form controls | `box-sizing:border-box` (WHATWG §15.3.10); `button` padding `1px 6px`; `textarea` `white-space:pre-wrap` | no box-sizing rule for controls |

Also: Blink gives `:focus-visible` an `outline: auto 1px` and `::selection` a highlight — velox has neither.

**This substantially expands Task 4.2.** `pre` and `h3`–`h6` are the highest-value: `pre` silently destroys whitespace formatting, and `h3`–`h6` render at the parent's size in normal weight in a document where `h1` and `h2` are correct.

### N6 — `meter` and `progress` are `inline`, and that is already a documented decision (INFO, corrected)

Comparing `INLINE_BY_DEFAULT_TAGS` (`layout.rs:2042-2046`) against Blink, `meter` and `progress` are `inline-block` in browsers and `inline` in velox. **On re-reading the source this is deliberate and already argued**, at `layout.rs:2029-2033`:

> `progress` and `meter` are `inline-block` in browsers, not `inline`. Velox
> has no `inline-block` layout, and `display: inline` is a far closer
> approximation than `block`, so they are claimed as `inline`. Do not
> "correct" them back to block.

The same reasoning is mirrored in `ua.css:15-18`. The other 27 entries (`img`, `a`, `span`, `code`, `label`, `output`, `data`, `time`, …) match Blink exactly.

**No action needed** — but Task 4.3 must carry this justification into the per-tag table verbatim, or a future maintainer will "fix" it back to `block` and make it wrong.

### N7 — Design debt: `default_display_for_tag` is a flat list

`layout.rs:2061-2067` is a single `contains()` over one hardcoded array. The spec-accurate mechanism is Blink's *element default style* tables (css-display-3 §2.7, HTML §15.3) — per-tag and multi-valued. Note the existing comment at `layout.rs:2048-2060` is genuinely high quality and argues the `inline-block` deviation correctly; the fix is to extend that rigor to a per-tag table with a justification per entry, not to keep appending to a flat list.

### N8 — winit's `ControlFlow::Wait` is correct — document it to stop a future "fix" (INFO)

From the winit docs: `ControlFlow::Wait` is "ideal for non-game applications that only update in response to user input, and **uses significantly less power/CPU time** than `ControlFlow::Poll`." Velox is right. It *looks* like a bug ("event loop not ticking") and a future maintainer may "fix" it by switching to `Poll`, which would burn CPU continuously and directly contradict the resource-frugal requirement. Add a comment at the call site.

For animation, the correct tool is `ControlFlow::WaitUntil` scheduling `AboutToWait`. The existing `CaretBlinkTicker` (530 ms, focus-gated, correct `Drop` at `lib.rs:293-301`) is the pattern to copy — a focus-gated `WaitUntil` blink costs **zero** CPU when nothing is focused.

### N9 — `notify` introduces inotify exhaustion as a new failure mode (MEDIUM)

From notify's documented "Known Problems": inotify exhaustion produces **"No space left on device"**, mitigated by raising `fs.inotify.max_user_instances` / `max_user_watches`. Velox's current `read_dir` poll has no such limit, so **moving to `notify` introduces a failure mode velox does not handle today.** A silently dead watcher is worse than a slow-but-working poll. Two requirements for Task 3.1: surface `Err` instead of stopping silently, and verify by test that `target/` (where `cargo build` writes thousands of files) is excluded.

### N10 — rust-skia's own FFI lessons apply to velox's `unsafe` audit (MEDIUM)

From a recent rust-skia commit message (its own `AGENTS.md`):
- The dangerous FFI bug class is **ownership, not arithmetic**: wrapping a *borrowed* `SkCanvas::recorder()` pointer in an owning handle caused a **double-free**.
- `Send` **without** `Sync` is correct for a graphics context: "Sync would let two Contexts race on it."
- `zeroed().assume_init()` is **unsound for C++ types** — "a zeroed value of an arbitrary C++ type need not be a valid Rust value."

Add these three classes explicitly to Task 5.5.2. Note velox is on the CPU path with no live `DirectContext`, so the highest-risk class is currently dormant — an argument for the N4 CPU recommendation.

### N11 — Vite's HMR boundary model (INFO — sharpens Phase 3)

From Vite's HMR docs: a self-accepting module is an **HMR boundary**; "importers up the chain from the boundary module will not be notified"; **CSS updates are special-cased to simply swap the stylesheet** with no reload; `hot.data` persists across successive instances of a module (mutate, don't reassign); `hot.dispose`/`hot.prune` clean up side effects; `hot.invalidate` escalates when a boundary cannot handle an update.

Velox's watcher has **no notion of what kind of thing changed**. Classifying style-only / template-only / script is the key architectural change, and a `<style>`-block edit must not trigger `cargo build`. See Tasks 3.2/3.3.

### N12 — Vue's per-flush dedupe set (MEDIUM — refines Task 1.2/1.4)

From Vue's reactivity internals: triggered jobs go into a queue scheduled on the **microtask** queue, and "if another dependency of the same component changes within the same synchronous execution section, **no second job gets added**." The formulation is a `ALREADY_QUEUED: HashSet<EffectId>` **cleared once at the end of the flush** — distinct from velox's current `QUEUED` set, which is consulted at run time. It composes cleanly with the monotonic-id fix.

Also: Vue sets `activeEffect` around the body from a single slot, not a queue that can re-enter. This is the "why" behind Task 1.1's "take the body out of the RefCell" approach. **Do not** import `flush: 'sync'` — Vue's own docs call it a performance trap.

### N13 — The spec's base-size vs hypothetical-size split sharpens Task 2.1 (INFO)

css-flexbox-1 §9.2 is precise about the distinction velox conflates:
- **Flex base size**: "the item's min and max main size properties **are ignored (no clamping occurs)**"
- **Hypothetical main size**: "the item's flex base size **clamped** according to its min and max main size properties"
- **Target main size**: "**Clamp each non-frozen item's target main size** by its min and max main size properties"

Velox calls `at()` three times per content-basis flex item (`layout.rs:3793` probe, `:3815` clamped target, `:3839` non-content arm), conflating these three roles and re-descending each time. Task 2.1 should be re-specified around this decomposition — see the revised task below.

Note also that browsers hit this exact problem: the WPT suite includes `nested-flex-image-loading-invalidates-intrinsic-sizes.html`. Browsers solve it with **invalidation** (Task 2.2) as well as memoization. Both tasks are needed; 2.1 just makes the common case cheap.

---

## Phase 0 — Restore a green baseline (do first; nothing else is verifiable until this is done)

### Task 0.1: Fix the failing scoped-style test

**Objective:** Make `velox-cli --test scoped_style_merging` pass.

**Files:**
- Modify: `velox-cli/tests/scoped_style_merging.rs:81`
- Inspect: `velox-sfc/src/codegen.rs` (`scope_css` / `scope_selector_list`)

**Step 1: Read the failure**

```
thread 'layer_a_child_rules_are_scoped_in_root_stylesheet' panicked at
velox-cli/tests/scoped_style_merging.rs:81:5:
TodoInput's .btn-add must be scoped in the merged sheet.
```

The merged sheet shows `App`'s rules with `[data-v-81f51acc]` and `Todos`' with `[data-v-f5e31faa]`, but `TodoInput`'s `.btn-add` rule is missing or carries the wrong hash.

**Step 2: Run to confirm the failure**

```bash
cargo test -p velox-cli --test scoped_style_merging
```
Expected: `layer_a_child_rules_are_scoped_in_root_stylesheet ... FAILED`

**Step 3: Locate the merge logic**

Read `velox-sfc/src/codegen.rs` around `scope_css` and the child-style merge. The uncommitted work in `velox-cli/templates/project/src/components/` is mid-flight; the child component's own scope hash is not being applied when its rules are hoisted into the root stylesheet.

**Step 4: Fix so each child's rules keep the CHILD's hash, not the root's**

**Step 5: Verify**

```bash
cargo test -p velox-cli --test scoped_style_merging
```
Expected: `2 passed; 0 failed`

**Step 6: Commit**

```bash
git add velox-cli velox-sfc
git commit -m "fix(sfc): scope a child component's style rules with the child's own hash"
```

### Task 0.2: Apply rustfmt and get clippy clean

**Objective:** `cargo fmt --check` passes; clippy warnings drop to zero for the CI gate.

**Files:** the 11 files reported by `cargo fmt --check`: `velox-renderer/src/skia_render.rs`, `velox-renderer/tests/frame_cost_bench.rs`, `velox-renderer/tests/presenter_pixel_bytes.rs`, `velox-dom/tests/flex_content_basis_probe_passes.rs`

**Step 1: Apply formatting**

```bash
cargo fmt --all
cargo fmt --all -- --check
```
Expected: no output

**Step 2: Fix the 88 clippy warnings**

Run and address each category:
```bash
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```
Highest-count lints: 14 collapsible `if`, 8 `Arc` not `Send`/`Sync` (**investigate — see Task 1.6**), 6 doc list indentation, 5 needless deref, 3 elidable lifetimes, 2 complex types, 2 `Range::contains`, 1 match-for-equality. Prefer real fixes over `#[allow]`. Any remaining `#[allow]` must carry a comment explaining why.

**Step 3: Verify and commit**

```bash
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
git commit -am "style: rustfmt + clippy clean across workspace"
```

### Task 0.3: Reclaim disk

**Objective:** Free ~50 GB.

**Step 1: Measure**

```bash
du -sh target test-app/tmp test-app tmp
```
Expected: `target` ≈ 45 GB, `test-app` ≈ 3.3 GB, `tmp` ≈ 2.0 GB

**Step 2: Remove the two nested, gitignored, fully re-derivable target dirs**

```bash
rm -rf test-app/target tmp/velox_test_init_app/target
```

**Step 3: Prune the root target without a full clean** (keeps incremental builds)

```bash
cargo-sweep --time 7 2>/dev/null || cargo clean --release
df -h .
```

**Step 4: Prevent recurrence** — add to `.gitignore` and note in `docs/` that nested `target/` dirs must not be committed.

---

## Phase 1 — Correctness: reactivity crashes, leaks, and silent failures

These are P0. The first crashes a running app; the second makes the UI silently stop responding.

### Task 1.1: Fix the re-entrant effect panic (`signal.rs:71`)

**Objective:** An effect that writes a signal it also reads must not panic.

**Root cause:** `flush_queue` holds `eff.borrow_mut()` across the effect body (`signal.rs:71`). A `set()` inside that body calls `flush_queue` again, which re-borrows the same `RefCell`.

**Reproduced (compiled probe against real `signal.rs`):**
```
PANIC: panicked at src/signal.rs:71:13: RefCell already borrowed
```
Triggered by both a self-invalidating effect and a two-effect cycle.

**Files:**
- Modify: `velox-core/src/signal.rs:45-76` (`flush_queue`)
- Test: `velox-core/tests/signal_reentrancy_tests.rs` (create)

**Step 1: Write the failing test**

```rust
// velox-core/tests/signal_reentrancy_tests.rs
use std::cell::RefCell;
use std::rc::Rc;
use velox_core::signal::{effect, Signal};

#[test]
fn effect_writing_a_signal_it_reads_does_not_panic() {
    let a = Rc::new(Signal::new(0i32));
    let runs = Rc::new(RefCell::new(0usize));
    {
        let r = runs.clone();
        let a2 = a.clone();
        let _h = effect(move || {
            let v = a2.get();
            *r.borrow_mut() += 1;
            if v < 3 {
                a2.set(v + 1);
            }
        });
    }
    assert_eq!(*runs.borrow(), 4, "0,1,2,3 then stop");
}

#[test]
fn two_effects_writing_each_other_do_not_panic() {
    let a = Rc::new(Signal::new(0i32));
    let b = Rc::new(Signal::new(0i32));
    let n = Rc::new(RefCell::new(0usize));
    {
        let r = n.clone();
        let (a2, b2) = (a.clone(), b.clone());
        let _h1 = effect(move || {
            let v = a2.get();
            *r.borrow_mut() += 1;
            if v < 5 { b2.set(v + 1); }
        });
    }
    {
        let r = n.clone();
        let (a2, b2) = (a.clone(), b.clone());
        let _h2 = effect(move || {
            let v = b2.get();
            *r.borrow_mut() += 1;
            if v < 5 { a2.set(v + 1); }
        });
    }
    assert!(*n.borrow() > 0);
}
```

**Step 2: Verify it fails**

```bash
cargo test -p velox-core --test signal_reentrancy_tests
```
Expected: FAIL — `panicked at src/signal.rs:71: RefCell already borrowed`

**Step 3: Fix with a re-entrancy guard**

Take the closure out of the `RefCell` before calling it, so a nested flush never re-borrows:

```rust
// velox-core/src/signal.rs
// Change the effect representation so the body is not held borrowed while running.
type Effect = Rc<RefCell<Option<Box<dyn FnMut()>>>>;

fn flush_queue() {
    if IS_FLUSHING.with(|f| f.replace(true)) { return; }
    loop {
        let Some(eff) = EFFECT_QUEUE.with(|q| q.borrow_mut().pop_front()) else { break };
        let id = ptr_id(&eff);
        QUEUED.with(|s| { s.borrow_mut().remove(&id); });
        if STOPPED_EFFECTS.with(|s| s.borrow().contains(&id)) { continue; }

        CURRENT_EFFECT.with(|c| *c.borrow_mut() = Some(eff.clone()));
        // Take the body OUT of the RefCell, run it, put it back. A nested
        // flush_queue() from inside the body can no longer double-borrow.
        let body = eff.borrow_mut().take();
        if let Some(mut f) = body {
            f();
            *eff.borrow_mut() = Some(f);
        }
        CURRENT_EFFECT.with(|c| *c.borrow_mut() = None);
    }
    IS_FLUSHING.with(|f| f.set(false));
}
```

Apply the same take-and-restore in `effect()`'s initial run (`signal.rs:248-253`).

**Step 4: Verify**

```bash
cargo test -p velox-core
```
Expected: all pass, including the two new tests

**Step 5: Commit**

```bash
git add velox-core
git commit -m "fix(core): a re-entrant effect write no longer double-borrows its RefCell"
```

### Task 1.2: Replace pointer identity with a monotonic effect id

**Objective:** A newly created effect must never inherit a dead effect's stopped flag.

**Root cause:** `ptr_id` (`signal.rs:29`) is `eff.as_ptr() as usize`. `STOPPED_EFFECTS` and `QUEUED` key on it and entries are **never removed from `STOPPED_EFFECTS`**. A new effect allocated at a recycled address is treated as stopped.

**Reproduced:**
```
50k stop() cycles completed
fresh effect initial runs = 1
fresh effect total runs after set = 1 (SUPPRESSED)   <-- should be 2
```
The app renders, state updates, and the UI silently stops responding.

**Files:**
- Modify: `velox-core/src/signal.rs:8-31` (effect type, `ptr_id`, storage)
- Test: `velox-core/tests/signal_effect_id_tests.rs` (create)

**Step 1: Write the failing test**

```rust
// velox-core/tests/signal_effect_id_tests.rs
use std::cell::RefCell;
use std::rc::Rc;
use velox_core::signal::{effect, Signal};

#[test]
fn a_fresh_effect_after_many_stops_still_runs() {
    let s = Rc::new(Signal::new(0i32));
    for _ in 0..50_000 {
        let sg = s.clone();
        let h = effect(move || { let _ = sg.get(); });
        h.stop();
    }
    let n = Rc::new(RefCell::new(0usize));
    {
        let r = n.clone();
        let sg = s.clone();
        let _h = effect(move || { let _ = sg.get(); *r.borrow_mut() += 1; });
    }
    assert_eq!(*n.borrow(), 1, "initial run");
    s.set(1);
    assert_eq!(*n.borrow(), 2, "effect must re-run; address reuse must not suppress it");
}
```

**Step 2: Verify it fails** — `cargo test -p velox-core --test signal_effect_id_tests` → FAIL at the second assert.

**Step 3: Add a monotonic id**

```rust
// velox-core/src/signal.rs
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_EFFECT_ID: AtomicU64 = AtomicU64::new(1);

struct EffectInner {
    id: u64,
    body: RefCell<Option<Box<dyn FnMut()>>>,
}
type Effect = Rc<EffectInner>;
type WeakEffect = Weak<EffectInner>;

fn id_of(eff: &Effect) -> u64 { eff.id }
```

Replace every `ptr_id(&eff)` call with `eff.id`, and `subs.retain(|w| w.upgrade().is_some())` (`signal.rs:161`) becomes `w.strong_count() > 0`.

Also bound the thread-local stores: drain `STOPPED_EFFECTS` when it exceeds a threshold, since entries are only meaningful for effects still in `EFFECT_STORAGE`.

**Step 4: Verify** — `cargo test -p velox-core` → all pass.

**Step 5: Commit**

```bash
git commit -m "fix(core): identify effects by monotonic id, not heap address"
```

### Task 1.3: Fix the `computed()` leak

**Objective:** A dropped `computed` must release its effect and captured state.

**Root cause:** `EFFECT_STORAGE` (`signal.rs:23`) holds a **strong** `Rc`. Reproduced: after dropping every handle, the effect still fired.

**Files:** `velox-core/src/signal.rs:23,260-291`

**Step 1: Write the failing test**

```rust
// velox-core/tests/signal_computed_drop_tests.rs
use std::cell::RefCell;
use std::rc::Rc;
use velox_core::signal::{computed, Signal};

#[test]
fn computed_effect_does_not_outlive_its_signal() {
    let base = Rc::new(Signal::new(1i32));
    let fires = Rc::new(RefCell::new(0usize));
    {
        let f = fires.clone();
        let b = base.clone();
        let c = computed(move || { *f.borrow_mut() += 1; b.get() * 2 });
        assert_eq!(c.get(), 2);
    }
    let before = *fires.borrow();
    base.set(5);
    assert_eq!(*fires.borrow(), before, "effect must not fire after its signal is dropped");
}
```

**Step 2: Verify it fails**

**Step 3: Fix** — hold effects in `EFFECT_STORAGE` as `Weak`, and let the `EffectHandle` (owned by the `Signal`) be the sole strong owner:

```rust
static EFFECT_STORAGE: RefCell<Vec<WeakEffect>> = RefCell::new(Vec::new());
```

`effect()` pushes a `Weak`; `EffectHandle` holds the strong `Rc` and removes the entry on `stop`/`drop`.

**Step 4: Verify** — `cargo test -p velox-core` → all pass.

**Step 5: Commit** — `git commit -m "fix(core): computed() releases its effect when the signal is dropped"`

### Task 1.4: Remove the per-read subscriber sweep

**Objective:** `Signal::get()` must be O(1) in the subscriber count.

**Root cause:** `signal.rs:161` runs `subs.retain(|w| w.upgrade().is_some())` on **every read** — an O(n) sweep with a refcount bump per entry, on the hottest path in the framework.

**Files:** `velox-core/src/signal.rs:157-171`

**Step 1: Write the failing benchmark** in `velox-core/benches/signal_read.rs` (create, criterion):

```rust
use criterion::{criterion_group, criterion_main, Criterion};
use std::rc::Rc;
use velox_core::signal::{effect, Signal};

fn bench_read_with_many_subscribers(c: &mut Criterion) {
    let s = Rc::new(Signal::new(0i32));
    let handles: Vec<_> = (0..64).map(|_| {
        let sg = s.clone();
        effect(move || { let _ = sg.get(); })
    }).collect();
    c.bench_function("read_with_64_subscribers", |b| {
        b.iter(|| s.get())
    });
    drop(handles);
}
criterion_group!(benches, bench_read_with_many_subscribers);
criterion_main!(benches);
```

**Step 2: Record the baseline** — `cargo bench -p velox-core --bench signal_read`

**Step 3: Fix** — prune only inside `set()` (already done at `signal.rs:186-195`) and inside `enqueue_effect`, never on the read path. In `get()`, do a single `iter().any()` for the duplicate check.

**Step 4: Re-run the benchmark** — expect a material drop; record both numbers in the commit message.

**Step 5: Commit** — `git commit -m "perf(core): stop sweeping subscribers on every signal read"`

### Task 1.5: Add a recursion-depth guard to the parser and layout

**Objective:** Deeply nested input must produce a clean diagnostic, not a stack overflow or a 12-second hang.

**Evidence:** 2^depth layout (Phase 2 addresses the constant); `velox-sfc` has no depth limit (`grep 'MAX_DEPTH|depth_limit'` → nothing).

**Files:**
- Modify: `velox-sfc/src/template_parse.rs` (add `MAX_TEMPLATE_DEPTH: usize = 256`)
- Modify: `velox-dom/src/layout.rs:2890` (`at()` — add a depth parameter with a cap)
- Test: `velox-sfc/tests/depth_limit_tests.rs` (create), `velox-dom/tests/layout_depth_limit.rs` (create)

**Step 1: Write the failing tests**

```rust
// velox-sfc/tests/depth_limit_tests.rs
#[test]
fn deeply_nested_template_is_a_diagnostic_not_a_crash() {
    let src = format!("{}{}", "<div>".repeat(5_000), "</div>".repeat(5_000));
    let r = velox_sfc::sfc::parse_sfc(&src);
    assert!(r.is_err(), "must be rejected, not overflow the stack");
    let msg = r.unwrap_err();
    assert!(msg.contains("nest") || msg.contains("depth"), "diagnostic must say why: {msg}");
}
```

**Step 2: Verify it fails** (stack overflow aborts the test process)

**Step 3: Implement the guard** — track depth during element parsing; return `Err("template nested too deeply (max 256)")`. Thread a `depth: usize` through `at()`, and when it exceeds the cap, stop descending and return the node with its available rect rather than recursing.

**Step 4: Verify** — both tests pass; `cargo test -p velox-sfc -p velox-dom` green.

**Step 5: Commit** — `git commit -m "fix(sfc,dom): bound template and layout recursion depth"`

### Task 1.6: Investigate the 8 `Arc` not-Send/Sync clippy warnings

**Objective:** Confirm these are benign and document, or fix.

**Files:** located via `cargo clippy ... | grep 'not Send'`

`Signal` uses `Rc`/`RefCell` (single-threaded by design, matching the event loop's threading model). If the warnings come from `Arc<something-not-Send>`, that is a genuine soundness smell. Read each, and either fix or add a comment-justified `#[allow]`.

---

## Phase 2 — Performance: make the framework fast and resource-frugal

**Gate for this phase:** `frame_cost_bench` reports layout ≤ 200 µs at 10 todos (from 1,947 µs), and the depth probe at depth 18 completes in < 50 ms (from 11,979 ms).

### Task 2.1: Split the three flex sizing roles and lay out each subtree once — kills 2^depth

**Objective:** Layout becomes linear in nesting depth.

**Root cause:** `velox-dom/src/layout.rs:3760-3850`. For a flex item with no declared basis, `at()` is called **three times**: a wide probe (`:3793`), a clamped target (`:3815`), and once more on the non-content arm (`:3839`). The probe subtree result is discarded and recomputed. Nesting multiplies — measured ~2.0× per level, so the third call is not always reached.

**Revised from research (N13).** The previous version of this task said "memoize the probe." The spec says something more precise, and following it removes the ambiguity about *what* to memoize. css-flexbox-1 §9.2 defines three distinct numbers:

| Concept | Spec rule | Inputs |
|---|---|---|
| **Flex base size** | "the item's min and max main size properties **are ignored (no clamping occurs)**" | content, available main size |
| **Hypothetical main size** | "the item's flex base size **clamped** according to its min and max main size properties" | base size + min/max — **no re-descend** |
| **Target main size** | "**Clamp each non-frozen item's target main size** by its min and max main size properties" | flex resolve + min/max — **no re-descend** |

Velox conflates all three into repeated `at()` calls. Implement them as three stages where **only the first descends the subtree**:

```rust
// Stage 1 — the ONLY subtree descent. Content-derived, unclamped.
let base_size = self.measure_content_basis(item, available_main);

// Stage 2 — pure arithmetic, no descent.
let hypothetical = clamp(base_size, min_main, max_main);

// Stage 3 — flex resolve, then clamp. Pure arithmetic, no descent.
let target = clamp(resolve_flexible(hypothetical, free_space), min_main, max_main);
```

**Why this is correct, not just faster:** the spec deliberately makes the base size *unclamped* and applies clamping *afterward*. Velox's current code re-descends to recompute because it tries to get a clamped answer from the first pass. Splitting the roles means the clamp is applied to an already-measured number, which is both cheaper and closer to the spec.

**Files:**
- Modify: `velox-dom/src/layout.rs:3760-3850` (replace the three `at()` calls with the three-stage form)
- Test: `velox-dom/tests/flex_probe_memo.rs` (create)

**Step 1: Write the failing test**

```rust
// velox-dom/tests/flex_probe_memo.rs
use velox_dom::{h, text, Props, VNode};
use velox_style::Stylesheet;

fn nested(depth: usize) -> VNode {
    let mut n = h("div", Props::from(vec![("class","item")]),
        vec![h("span", Props::from(vec![("class","txt")]), vec![text("x")])]);
    for _ in 0..depth {
        n = h("div", Props::from(vec![("class","item")]), vec![n]);
    }
    h("div", Props::from(vec![("class","app")]), vec![n])
}

#[test]
fn layout_of_deeply_nested_content_basis_flex_is_fast() {
    let ss = Stylesheet::parse(
        ".app{display:flex;flex-direction:column;width:100%;min-height:100vh}
         .item{display:flex;flex-direction:row}
         .txt{flex:1}");
    let tree = nested(18);
    let t = std::time::Instant::now();
    let _ = velox_dom::layout::compute_layout(&tree, 1200, 800);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    assert!(ms < 50.0, "18 levels took {ms:.1} ms; expected < 50 ms (no exponential blowup)");
}
```

**Step 2: Verify it fails** — expect ~12,000 ms.

**Step 3: Implement the three-stage split.** For a content-basis item, call `at()` (or a dedicated `measure_content_basis`) **exactly once**, then do the clamping and flex resolution arithmetically.

If measurement results must also be reused *across* items or passes (e.g. a shrink pass after a grow pass), add a per-pass cache keyed on `(node identity, available main, containing block)` — bounded at `MAX_MEASURE_ENTRIES = 4096`; past the cap, stop inserting rather than grow without bound. Note this cross-item cache is a *secondary* concern; the primary fix is not descending three times.

**Step 4: Verify**

```bash
cargo test -p velox-dom
cargo test -p velox-dom --features skia-native --test todo_item_pixels
cargo test -p velox-dom --test layout_golden
```
Expected: all pass; the new test reports < 50 ms. The existing `flex_content_basis_probe_passes.rs` is the guard against an over-aggressive shortcut — it must stay green.

**Step 5: Measure and commit**

```bash
cargo test -p velox-renderer --release --features skia-native --test frame_cost_bench -- --ignored --nocapture
git commit -m "perf(dom): split flex base/hypothetical/target sizing — one subtree descent per item"
```

### Task 2.2: Add layout invalidation — stop re-laying-out the whole tree

**Objective:** A keystroke re-lays-out only what changed.

**Evidence:** `grep -rn 'memo|cache|dirty|invalidate' velox-dom/src/layout.rs` → **no matches.** Every frame recomputes cascade, layout, and paint for the entire tree. Layout is 92% of per-keystroke cost at 10+ items.

**This is the largest architectural change in the plan.** Vue's premise is fine-grained reactivity; velox currently re-renders everything, always.

**Files:**
- Modify: `velox-dom/src/lib.rs:4-11` (`VNode` gains an identity slot)
- Modify: `velox-dom/src/layout.rs:2890` (`at()` consults the cache)
- Modify: `velox-renderer/src/lib.rs:1455,2084` (pass the previous frame's tree + layout)
- Test: `velox-dom/tests/layout_invalidation.rs` (create), `velox-renderer/tests/partial_repaint.rs` (create)

**Step 1: Design note — identity.** `velox-dom/src/diff.rs:17-24` already documents why `:key` cannot preserve identity: `VNode` is pure data with no state cell, no instance handle, no lifecycle hook. Adding an identity slot is the prerequisite for both invalidation and real reconciliation. Write this up in `docs/` before coding, because it is a public shape change to `VNode`.

**Step 2: Add a stable node identity**

```rust
// velox-dom/src/lib.rs
pub enum VNode {
    Element { tag: String, props: Props, children: Vec<VNode> },
    Text(String),
}
```
becomes a struct with a `node_id: u64` assigned by a monotonically increasing counter during tree construction, so the same logical element keeps its id across renders when the structure is unchanged.

**Step 3: Introduce a layout cache**

```rust
// velox-dom/src/layout.rs
pub struct LayoutCache {
    entries: HashMap<(u64, i32, i32, ContainingBlockKey), LayoutNode>,
    styled_hash: HashMap<u64, u64>,   // node_id -> hash of its computed style
    structure_gen: u64,              // bumped when any node's children change
}
```

In `at()`, before recursing, look up `(node_id, avail_w, avail_h, containing_block)`. If the entry exists **and** the node's style hash matches, return the cached `LayoutNode` directly. Any change to a node's `props`, its `children` length, or its class bumps its style hash and invalidates it.

**Step 4: Wire the previous frame through the render loop**

`velox-renderer/src/lib.rs:1455` currently calls `make_view(vw, vh)` fresh each frame. Keep the previous `VNode` and `LayoutNode` alive in the loop state, pass them to `style_vnode_with_hover` and `compute_layout`, and let the cache hit.

**Step 5: Verify correctness before speed**

This is the highest-risk task in the plan. A stale-cache bug is silent and visual. Required:
- `layout_golden.rs` and every `flex_*` test stay green
- `todo_item_pixels.rs` pixel proofs unchanged
- New test: mutating one item's text leaves its siblings' cached rects **bit-identical**
- New test: changing the window size invalidates **all** entries

**Step 6: Measure and commit**

```bash
cargo test -p velox-dom -p velox-renderer
cargo test -p velox-renderer --release --features skia-native --test frame_cost_bench -- --ignored --nocapture
git commit -m "perf(dom,renderer): cache layout per node and invalidate on change"
```

### Task 2.3: Stop re-parsing the stylesheet every frame

**Objective:** The stylesheet is parsed once at startup, not per frame.

**Evidence:** `velox-renderer/src/lib.rs:1455` and `:2084` both call `make_view(vw, vh)`, which returns `(VNode, Stylesheet)` — the `Stylesheet` is re-constructed every frame. The bench measured it at only 1.6 µs, so this is not a hotspot; it is nonetheless a per-frame allocation that grows with stylesheet size and should be hoisted.

**Files:** `velox-renderer/src/lib.rs` (the two `make_view` call sites), `velox-sfc/src/codegen.rs` (`make_view` signature)

**Step 1: Split `make_view`** into `make_vnode()` and a separately-cached `build_stylesheet()` that returns `Arc<Stylesheet>` computed once. Update both call sites to hold the `Arc` across frames.

**Step 2: Verify** — `cargo test -p velox-renderer`; confirm the bench's stylesheet-parse line drops to 0.

**Step 3: Commit** — `git commit -m "perf(renderer): parse the stylesheet once, not per frame"`

### Task 2.4: Reuse the paint buffer across frames

**Objective:** Stop allocating a fresh raster surface per frame.

**Evidence:** `frame_cost_bench.rs`'s own header warns that `render_vnode_to_rgba` "builds a fresh raster surface per call, which the real loop does not" — so the bench **overstates** paint. Paint is still 1.6 ms of a 3.7 ms frame. Whether the live loop already reuses its surface must be confirmed, not assumed.

**Files:** `velox-renderer/src/skia_surface.rs`, `velox-renderer/src/presenter.rs`, `velox-renderer/src/skia_render.rs`

**Step 1: Verify the current behaviour** — read the `RedrawRequested` arm in `velox-renderer/src/lib.rs:1776` and trace where the surface is created. If it is already reused, record that in `docs/` and skip the fix.

**Research note (N4):** velox uses `sk::surfaces::raster_n32_premul` — a **CPU raster surface** — with `_gpu_ctx: None` and `_gl_ctx: None`. So the benchmark's "paint+readback" figure is software rasterization plus a full-surface readback to RGBA, and paint optimizations are being measured against a software path. Decide the target before optimizing: see the CPU-reference recommendation in Task 5.8. If velox stays on the CPU path, the wins available are buffer reuse (this task) and damage-limited repaint (Task 2.2's partial repaint), not GPU acceleration.

**Step 2: If not reused**, hoist the surface into the loop state and reallocate only on resize or scale-factor change (which already have a coalescing path via `ResizeState`).

**Step 3: Add a regression test** in `velox-renderer/tests/frame_cost_bench.rs` asserting paint cost at 0 todos stays under 2 ms across 100 consecutive frames, and add a peak-RSS assertion.

**Step 4: Commit** — `git commit -m "perf(renderer): reuse the raster surface across frames"`

### Task 2.5: Reduce per-node allocation

**Objective:** Cut the constant factor on the hot path.

**Evidence:** `Props` is `HashMap<String, String>` (`velox-dom/src/lib.rs:13-16`) — every attribute is two heap allocations plus a hashmap node. `VNode::Element` stores `tag: String`. At 15.5 µs/vnode on flex paths, allocation is a large share.

**Files:** `velox-dom/src/lib.rs`, `velox-sfc/src/template_codegen.rs` (codegen must emit the new shape)

**Step 1: Intern tags.** `tag: &'static str` via a `Tag` newtype with a lazily-built interner, so `h("div", ..)` allocates no `String`.

**Step 2: Small-vector attributes.** Replace `HashMap<String,String>` with a sorted `Vec<(Arc<str>, Arc<str>)>` using a small-vector representation for the common ≤8-attribute case. Most elements have 0–3 attributes.

**Step 3: Measure before and after** with the `iso` probe shape from Phase 2's baseline (block 2,315 ns/vnode; flex 40,266 ns/vnode).

**Step 4: Commit** — `git commit -m "perf(dom): intern tags and use small-vector props"`

---

## Phase 3 — Hot reload: make the dev server actually hot-reload

**Current behaviour (verified by reading `velox-cli/src/commands/dev.rs`):** a file change sends `send_hmr_reload` (`:174`), waits up to 2 s for exit (`:180`), `child.kill()` (`:190`), then `spawn_app_hmr` (`:193`) — a full `cargo build` and relaunch. **The window closes and reopens on every edit.** "HMR" here means process restart.

Additional defects: the watcher is a 400 ms polling scan (`:209`, `changed_file` at `:439`) using `read_dir` + `metadata().modified()`; debounce is a blocking `sleep(150ms)` at `:467` that stalls command handling; every change triggers a full `cargo build` (`:350`, `:381`).

### Task 3.1: Replace polling with a real filesystem watcher

**Objective:** React to changes in < 50 ms without scanning the tree.

**Files:**
- Modify: `velox-cli/Cargo.toml` (add `notify = "6"`, drop nothing)
- Modify: `velox-cli/src/commands/dev.rs:439-470` (`changed_file` → `notify` watcher)
- Test: `velox-cli/tests/watcher_tests.rs` (create)

**Step 1: Write the failing test**

```rust
// velox-cli/tests/watcher_tests.rs
#[test]
fn watcher_fires_within_100ms_of_a_write() {
    let dir = tempdir();
    let (tx, rx) = std::sync::mpsc::channel();
    let w = velox_cli::commands::dev::watch_dir(dir.path(), tx).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(dir.path().join("a.vx"), "<template></template>").unwrap();
    let ev = rx.recv_timeout(std::time::Duration::from_millis(500));
    assert!(ev.is_ok(), "must fire promptly");
    drop(w);
}
```

**Step 2: Verify it fails** (the current 400 ms poll plus 150 ms blocking debounce exceeds the timeout)

**Step 3: Implement with `notify`**, wiring a `RecommendedWatcher` into the existing `mpsc::Sender<DevCmd>` channel. Keep `IGNORED` filtering (`.git`, `target`, dotfiles). Debounce **without blocking** — drain-and-collapse in the event loop using an `Instant` deadline, never `thread::sleep` in the watcher path.

**Research note (N9) — two failure modes this introduces:**
1. **`notify` on Linux uses inotify, which can be exhausted.** Per its documented "Known Problems," inotify limits produce **"No space left on device"**, mitigated by raising `fs.inotify.max_user_instances` / `max_user_watches`. Velox's current `read_dir` poll has no such limit, so this is a **new** failure mode. A silently dead watcher is worse than the current slow-but-working poll: surface the `Err` to the user and keep the dev server alive, and document the sysctl remedy in the `velox dev` docs.
2. **`target/` must be excluded, and that exclusion is load-bearing.** `cargo build` writes thousands of files into `target/` inside the watched tree. Watching it is precisely what will exhaust inotify watches. The `IGNORED` filter must be **verified by test**, not by inspection: create a file under `target/` and assert no event fires.
3. **`notify` emits multiple events per single save** on Linux (create + modify + close_write). The non-blocking drain-and-collapse debounce above is therefore mandatory, not optional.

**Step 3b: Classify the change, not just detect it (N11).** The watcher currently only reports *that* something changed. Vite's model is richer: the update type determines the response, and CSS updates are special-cased to a stylesheet swap with no rebuild. Add a `ChangeKind` to the dev command — `StyleOnly`, `TemplateOnly`, `Script` — determined by parsing which SFC block the edit landed in. This is the prerequisite for Task 3.2 and the single biggest HMR win: **a `<style>` edit must not trigger `cargo build`.**

**Step 4: Verify** — the new test passes; manual check that `dev` still ignores `target/` churn.

**Step 5: Commit** — `git commit -m "perf(cli): watch with notify instead of a 400ms polling scan"`

### Task 3.2: Style-only hot update — no rebuild, no restart

**Objective:** Editing a `<style>` block swaps styles live, preserving all state.

**Why first:** style-only and template-only edits are the overwhelming majority of day-to-day work, and they are the cheapest to make state-preserving. Vue's SFC compiler emits a `css-update` per style block; the `<style>` tag is swapped with no reload.

**Files:**
- Modify: `velox-sfc/src/codegen.rs` (emit per-component style hashes)
- Modify: `velox-cli/src/commands/dev.rs:168-195` (branch on change kind)
- Modify: `velox-renderer/src/hmr.rs` (new `HmrMessage::StyleUpdate { hash, css }`)
- Test: `velox-renderer/tests/hmr_style_update.rs` (create)

**Step 1: Write the failing test**

```rust
// velox-renderer/tests/hmr_style_update.rs
#[test]
fn style_update_changes_paint_without_rebuilding_the_tree() {
    // Build a tree, layout it, capture pixels.
    // Send HmrMessage::StyleUpdate with a new background colour.
    // Assert: the new colour is painted AND the VNode/layout objects are
    //         still the same allocations (no rebuild happened).
}
```

**Step 2: Verify it fails**

**Step 3: Implement** — add a `StyleUpdate` variant to the HMR protocol carrying the merged stylesheet. On receipt, the render loop re-runs `style_vnode_with_hover` against the new sheet and repaints, keeping the existing vnode identity. Crucially, **skip `cargo build` entirely** in `dev.rs` when only `.vx` `<style>` content changed.

**Step 4: Verify** — the test passes; a manual edit of a `<style>` block updates the running window in well under a second with no rebuild.

**Step 5: Commit** — `git commit -m "feat(hmr): apply style-only changes live, with no rebuild"`

### Task 3.3: Template-only rerender preserving component state

**Objective:** Editing a `<template>` re-renders without a restart and without losing component state.

**Files:** `velox-sfc/src/codegen.rs`, `velox-renderer/src/hmr.rs`, `velox-renderer/src/lib.rs` (HMR loop, `:2133-2218`)

**Vue model:** a template change is a `rerender` that preserves component state; only a script change forces a reload. Implement `HmrMessage::Rerender { template_hash }`: the dev server recompiles **only** the template to a new `make_view`, and the render loop rebuilds the vnode while preserving the existing component state and the focused input's caret/selection.

This depends on Task 2.2's node identity — preserving state across a rerender needs stable node identity, which is the same prerequisite as reconciliation. Sequence 2.2 first.

**Gate:** a user typing in an input, then editing an unrelated template expression, must not lose the typed text or the caret position.

### Task 3.4: Make the full-reload fallback correct and fast

**Objective:** A script change reloads promptly, with no orphaned processes.

**Files:** `velox-cli/src/commands/dev.rs:168-195, 343-400`

**Step 1: Fix the 2-second stall.** `:178-186` sleeps up to 2 s waiting for a graceful exit before killing. Do not wait at all: `send_hmr_reload` then `kill()` immediately. The 2 s grace is the dominant cost of a script-change reload.

**Step 2: Guarantee no orphans.** On every exit path — normal quit, `r` command, app crash, dev-server panic — `child.kill()` and `child.wait()`. Audit the `stdin` reader thread (`:313`) and the HMR client slot (`:230`) for leaks; `CaretBlinkTicker` already has a correct `Drop` (`velox-renderer/src/lib.rs:293-301`) and is the pattern to copy.

**Step 3: Coalesce rapid saves.** Multiple saves inside the debounce window must produce **one** rebuild, not a `cargo build` each. Track a pending-change flag rather than acting on the first event.

**Step 4: Recover from a compile error.** On a failed build, keep the watcher alive, print the error, and retry on the next save. Verify: introduce a syntax error, save, fix it, save — the app must come back without manual intervention.

**Step 5: Commit** — `git commit -m "fix(cli): no 2s reload stall, no orphaned children, coalesced rebuilds"`

### Task 3.5: Enable CI on feature branches

**Objective:** These fixes get regression protection.

`.github/workflows/ci.yml` triggers only on `[main, master, alpha]`. The work is on `fix/2A-flex-complete`, **93 commits ahead of origin** — none of it runs CI. Change to `on: [push, pull_request]` for all branches, and add a frame-budget gate (Task 2.7).

---

## Phase 4 — HTML/CSS default-behaviour parity

**Goal:** every element and style behaves like HTML/CSS/Vue by default, so authors never hand-write what a browser provides.

**Credit where due:** `docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md` is an unusually honest document, and its headline P0 (F-01, no user-agent stylesheet) **has been fixed** — `velox-style/src/ua.css` exists and is composed under author rules (`velox-style/src/lib.rs:628`).

### Task 4.1: Establish a default-behaviour conformance suite

**Objective:** Make "matches HTML/CSS defaults" a testable claim instead of an aspiration.

**Files:** `velox-dom/tests/default_parity.rs` (create), `velox-style/tests/ua_completeness.rs` (create)

**Step 1: Write the table-driven parity test.** One row per element, asserting the computed style velox produces equals the value a browser's UA stylesheet specifies. Start with the elements velox already claims to support:

```rust
// velox-dom/tests/default_parity.rs
#[test]
fn ua_defaults_match_html() {
    let cases = [
        // (tag, property, expected per HTML UA stylesheet)
        ("body", "margin-top", "8px"),
        ("h1", "margin-top", "0.67em"),
        ("h1", "font-size", "2em"),
        ("p", "margin-top", "1em"),
        ("ul", "padding-inline-start", "40px"),
        ("button", "padding-top", "6px"),
        ("span", "display", "inline"),
        // ... one row per element velox supports
    ];
    for (tag, prop, expected) in cases {
        let cs = velox_dom::style::computed_for_tag(tag);
        assert_eq!(cs.get(prop), expected, "{tag} {{ {prop} }}");
    }
}
```

**Step 2: Run it and record every failure.** Failures are the work list for 4.2–4.4.

### Task 4.2: Expand `ua.css` to cover the supported element set

**Objective:** Close the gaps the parity test finds. **Substantially expanded by research (N5).**

**Measured current state:** `ua.css` has **9 rules covering 11 tags** — `html, body, button, p, ul, ol, u, var, wbr`.

**Priority order, by how visible the current breakage is:**

**Tier 1 — near-universal in real markup, visibly wrong today. No layout engine needed.** Note `h1` and `h2` are **already correct** in `ua.css:4-5` — the gap starts at `h3`, which makes the omission more clearly an oversight than a decision:

| Selector | Declaration (from Blink `html.css` / WHATWG §15) |
|---|---|
| `h3` | `font-size: 1.17em; margin: 1em 0; font-weight: bold` |
| `h4` | `font-size: 1em; margin: 1.33em 0; font-weight: bold` |
| `h5` | `font-size: 0.83em; margin: 1.67em 0; font-weight: bold` |
| `h6` | `font-size: 0.67em; margin: 2.33em 0; font-weight: bold` |
| `pre` | `font-family: monospace; white-space: pre; margin-block: 1em 1em` |
| `blockquote` | `margin-block: 1em 1em; margin-inline: 40px 40px` |
| `hr` | `display: block; border-style: inset; border-width: 1px; margin-block: 0.5em; margin-inline: auto` |
| `fieldset` | `display: block; border: 2px groove; padding-inline: 0.75em; margin-inline: 2px` |
| `figure` | `display: block; margin-block: 1em 1em; margin-inline: 40px 40px` |
| `dl` | `display: block; margin-block: 1em 1em` |
| `dd` | `display: block; margin-inline-start: 40px` |
| `dt` | `display: block` |
| `address` | `display: block; font-style: italic` |
| `sub` / `sup` | `vertical-align: sub` / `super`; `font-size: smaller` |
| `mark` | `background-color: Mark; color: MarkText` |
| `big` / `small` | `font-size: larger` / `smaller` |
| `s` / `del` / `strike` | `text-decoration: line-through` |
| `i` / `cite` / `em` / `dfn` | `font-style: italic` |
| `strong` / `b` | `font-weight: bolder` |
| `tt` / `code` / `kbd` / `samp` | `font-family: monospace` |
| `center` | `display: block; text-align: center` |

`pre` is the standout: without `white-space: pre`, preformatted text **silently wraps**, corrupting its entire content. `h1`–`h6` is next: every document uses them and velox currently renders them at the parent's size and normal weight.

**Tier 2 — form controls.** Per WHATWG §15.3.10 and Blink's `html.css`:
```css
input, button            { display: inline-block; }
input:is([type=radio],[type=checkbox],[type=reset],[type=button],
         [type=submit],[type=color],[type=search]), select, button
                          { box-sizing: border-box; }
button                   { padding-block: 1px; padding-inline: 6px;
                            text-align: center; white-space: nowrap; }
textarea                 { white-space: pre-wrap; display: inline-block; }
label                    { cursor: default; }
input:not([type=file])   { cursor: text; }
```
Note velox's **documented deviation** on `button`/`input`/`select`/`textarea` being `block` rather than `inline-block` (`layout.rs:2048-2060`). That deviation stays — but the `box-sizing: border-box` and `textarea { white-space: pre-wrap }` rules are independent of it and should be added, since they change rendered geometry.

**Tier 3 — pseudo-elements velox lacks entirely.** Blink ships `:focus-visible { outline: auto 1px -webkit-focus-ring-color }` and a `::selection` highlight. Velox has neither. A focus ring is a genuine accessibility affordance, not decoration — add it.

**Tier 4 — tables. Needs Task 4.5's work first** (there is no table layout algorithm):
```css
table    { display: table; border-spacing: 2px; border-collapse: separate;
           border-color: gray; }
thead    { display: table-header-group; vertical-align: middle; }
tbody    { display: table-row-group; vertical-align: middle; }
tfoot    { display: table-footer-group; vertical-align: middle; }
tr       { display: table-row; }
td, th   { display: table-cell; padding: 1px; }
th       { font-weight: bold; text-align: center; }
caption  { display: table-caption; text-align: center; }
```

**Invariants to preserve:**
- Keep the existing `INLINE_BY_DEFAULT_TAGS` sync invariant: `velox-style/tests/cascade.rs` already fails if `ua.css` and `INLINE_BY_DEFAULT_TAGS` (`layout.rs:2042`) diverge. **Add new tags to both.**
- **Verify, do not assume (N6 — corrected on re-check).** Comparing `INLINE_BY_DEFAULT_TAGS` (`layout.rs:2042-2046`) against Blink, `meter` and `progress` are `inline-block` in browsers but `inline` in velox. **This is already documented as deliberate at `layout.rs:2029-2033` and `ua.css:15-18`**, with the reasoning: "Velox has no `inline-block` layout, and `display: inline` is a far closer approximation than `block`." Do **not** "fix" these. The remaining 27 entries match Blink. The one genuine gap here is that Task 4.3's per-tag table should preserve that justification verbatim, so a future maintainer does not correct them back to `block`.
- **Also unparsed entirely (N1):** `content`, `cursor`, `user-select`, `float`, `clear`, `aspect-ratio`, `writing-mode` have no `set_property` arm. `cursor` and `user-select` are UI-visible and were explicitly requested. Add `cursor` and `user-select` (both easy); record `float`/`clear`/`writing-mode`/`aspect-ratio` as deliberate non-goals for now — they imply layout modes velox does not implement.

### Task 4.3: Replace the flat `default_display_for_tag` list with a per-tag table (N7)

**Objective:** Make every display default individually auditable.

`layout.rs:2061-2067` is a single `INLINE_BY_DEFAULT_TAGS.contains(tag)`. The spec-accurate mechanism is Blink's per-tag *element default style* tables (css-display-3 §2.7, HTML §15.3), which are multi-valued and per-tag.

The existing comment at `layout.rs:2048-2060` is genuinely good — it explains *why* the `inline-block` deviation exists and argues it correctly. Extend that rigor: replace the flat list with a function that returns a `Display` and carries a justification per entry.

```rust
/// The display an element gets when neither the cascade nor an author rule
/// specifies one. Each entry cites the Blink/HTML default it mirrors.
fn ua_display_for_tag(tag: &str) -> Display { /* per-tag, documented */ }
```

Keep the existing sync test; extend it to assert every entry in the table has a justification. This is the task that makes Task 4.1's parity suite mechanically checkable instead of a hand-maintained list.

### Task 4.4: Fix the integer-viewport limitation

**Objective:** Support subpixel layout, as CSS does.

**Evidence:** `velox-dom/src/layout.rs:2888` — `pub fn compute_layout(node: &VNode, viewport_w: i32, viewport_h: i32)`. Viewport dimensions are integers, so layout cannot express fractional widths, capping HiDPI fidelity. All six call sites in `velox-renderer/src/lib.rs` pass `vw as i32`.

Change the signature to `f32` and update every call site. Expect golden-layout and pixel-proof tests to need tolerance adjustments — widen tolerances deliberately and document why, rather than snapping values to keep old assertions green.

### Task 4.5: Document the deliberate deviations

**Objective:** Make parity claims honest. **Expanded by research (N5/N6).**

`button`/`input`/`select`/`textarea` are `inline-block` in browsers; velox lays them out as `block`. The code comments this correctly and declines to fake it — the right call. Record each deviation in a `docs/HTML_PARITY.md` table: element, browser behaviour, velox behaviour, reason, and the issue that would close it. A deviation that is written down is a decision; one that is only in a source comment is a trap.

The research pass confirmed the deviation list is **larger than the source comment claims**. Add to the table:
- `meter`, `progress` — `inline-block` in Blink, `inline` in velox (N6). Either fix or record.
- `float`/`clear` — unimplemented entirely; flex items per spec must ignore them anyway (css-flexbox-1 §3: "float and clear do not create floating or clearance of flex items, and do not take it out-of-flow"), so this is defensible — but it must be written down.
- `writing-mode`, `aspect-ratio` — unimplemented; imply layout modes velox does not have.
- No table layout algorithm exists at all.

### Task 4.6: Decide `:key` and unify the reconcilers

**Objective:** Resolve the two-competing-reconcilers defect and give `:key` a real answer.

**Current state (verified):**
- `velox-dom/src/diff.rs` — complete, duplicate-key-safe, emits `Patch::MoveChild`. **Zero production callers.** Its own module docs (`:17-24`) argue identity preservation needs a state slot on `VNode`, which Task 2.2 adds.
- `velox-renderer/src/lib.rs:891` `reconcile_keyed_children` — its doc comment says *"This helper is wrong, and it is on a dead path."* On a key match it pushes the **stale** old node and discards the new content. Reachable only from `run_window_vnode`, which has zero in-tree callers.
- `velox-renderer/tests/reconcile_keyed_tests.rs` **asserts the stale behaviour as intended**.

**Step 1: Write the decision down in `docs/`** — either `:key` gains real identity semantics, or it is formally declared a non-goal with the tradeoff recorded. The current state (a key attribute that silently does nothing) is the worst option.

**Step 2: Delete `reconcile_keyed_children`** once `run_window_vnode` is removed or re-pointed. Fix or delete the seven tests that reference it — starting with the one asserting stale content.

**Step 3: Wire `diff::diff` into the render loop** (after Task 2.2 gives nodes identity) so `:key` actually reorders and preserves state.

**Step 4: Verify** — add `tests/key_reorder_identity.rs` proving a keyed reorder preserves per-item state, and that unkeyed reordering does not.

### Task 4.7: Never silently ignore a declaration (NEW — N1)

**Objective:** An author is never told a style worked when it did not.

`set_property` (`style.rs:1288`) ends with a bare `_ => {}` at `style.rs:1655`. Three properties are **parsed into fields and never rendered**: `transition` (`:1587`), `transform` (`:1575`), `box-shadow` (`:1582`). Verified: `grep -rn 'box_shadow\|shadow' velox-renderer/src` → **zero**.

**The rule to implement, from MDN's cascade reference:** invalid/unknown declarations are *filtered out* — silently ignored. So velox **should** silently ignore a property it does not recognize. What it must **not** do is silently ignore a property it *parses*.

**Step 1: Write the failing test**

```rust
// velox-dom/tests/declaration_honesty.rs
#[test]
fn a_parsed_but_unrendered_property_is_reported() {
    let mut cs = ComputedStyle::default();
    cs.set_property("transition", "opacity 0.2s ease");
    // Parsing succeeded, so the framework now OWES the author an effect —
    // or an explicit "not implemented" signal.
    assert!(cs.unimplemented.contains("transition"));
}

#[test]
fn a_genuinely_unknown_property_is_ignored_per_spec() {
    let mut cs = ComputedStyle::default();
    cs.set_property("color", "red");
    cs.set_property("-webkit-not-a-real-prop", "1");
    // MDN: invalid declarations are filtered out. No error. Silent is CORRECT.
    assert!(cs.unimplemented.is_empty());
}
```

**Step 2: Implement.** Add `unimplemented: Vec<&'static str>` (or a `HashSet`) to `ComputedStyle`. `set_property` records a name into it whenever it parses a declaration it cannot honour. Then:

- `velox lint` warns on each one, naming the file and line.
- The dev server prints a one-time notice per property on first use, e.g. `transition: parsed but not yet rendered (velox#NNN)`.
- The SFC compiler may emit the same warning at build time for the most common offenders.

This is cheap, requires no rendering work, and converts three silent lies into visible, tracked debt. It also means when `transition` is eventually implemented, removing the name from the list is the definition of done.

**Step 3: Decide the three properties.** Either implement them, or ship them behind the diagnostic. My recommendation: ship the diagnostic now (it is ~20 lines), then implement `transform` first (widest use), then `box-shadow`, then `transition` — which additionally needs the `ControlFlow::WaitUntil` animation clock from Task 5.6.

**Step 4: Verify** — `cargo test -p velox-dom --test declaration_honesty`; then confirm `velox lint` on a stylesheet using `transition` reports it.

**Step 5: Commit** — `git commit -m "feat(dom): report parsed-but-unrendered declarations instead of dropping them"`

### Task 4.8: Implement `min-width` and the flex automatic minimum size (NEW — N2/N3)

**Objective:** `min-width` stops being a no-op.

```
$ grep -c 'min_width' velox-dom/src/layout.rs
0
```

`min_width` is parsed (`style.rs:1329`) and stored (`style.rs:1223`) and **never read by layout**. `min_height` is read only by the viewport-filling heuristic at `layout.rs:2989-3089`, not as a general clamp.

**Step 1: Write the failing test**

```rust
// velox-dom/tests/min_size_tests.rs
#[test]
fn min_width_is_enforced() {
    let ss = Stylesheet::parse(".box{width:50px;min-width:200px}");
    let tree = h("div", Props::from(vec![("class","box")]), vec![]);
    let styled = velox_dom::style::style_vnode_with_hover(&tree, &ss, None);
    let r = velox_dom::layout::compute_layout(&styled, 800, 600);
    assert_eq!(r.width, 200, "min-width:200px must win over width:50px");
}

#[test]
fn flex_item_auto_min_size_is_content_based() {
    // css-flexbox-1 §4.5: a flex item's min-width:auto resolves to its
    // content-based minimum size, so a long unbreakable word cannot shrink
    // below its widest word.
    let ss = Stylesheet::parse(
        ".row{display:flex;width:100px}
         .item{display:flex}
         .txt{flex:1}");
    // ... assert the item's width >= the measured min-content width of the text
}
```

**Step 2: Verify both fail** (the first returns 50; the second shrinks below min-content).

**Step 3: Implement.**
1. In `at()`, after computing a box's width/height, clamp by `min_width`/`min_height` (and `max_*`) when they are definite lengths. Percentages resolve against the containing block, per CSS Sizing 3.
2. Implement the **automatic minimum size** for flex items: when `min-width` is `auto` and the item is a flex item, its used minimum is the content-based minimum size (max of min-content contribution, clamped by any specified min/max).
3. Respect the spec's ordering (§9.2): the **flex base size ignores min/max**; the **hypothetical main size applies the clamp**. Getting this backwards is a subtle correctness bug, so Task 2.1's three-stage split and this task must land together or in immediate succession — **Task 2.1 first.**

**Spec note (from css-flexbox-1 §3):** `float`, `clear`, and `vertical-align` have **no effect on a flex item**. If velox ever implements `vertical-align` (it already parses it at `style.rs:1537`), it must be suppressed for flex items — otherwise the two subsystems will disagree.

**Step 4: Verify**

```bash
cargo test -p velox-dom --test min_size_tests
cargo test -p velox-dom   # golden + flex tests must stay green
cargo test -p velox-dom --features skia-native --test todo_item_pixels
```

**Step 5: Commit** — `git commit -m "fix(dom): enforce min-width/min-height and the flex automatic minimum size"`

### Task 4.9: Unify the error conventions

**Objective:** One error type across the framework.

Three coexist: `VeloxError` (typed, `velox-dom/src/lib.rs:81-103`, used only in `presenter.rs`/`skia_gl.rs`), **56** ad-hoc `Result<_, String>` in `velox-renderer`, and `anyhow` in `velox-cli`. Adopt `VeloxError` throughout the library crates; reserve `anyhow` for the CLI binary boundary. Do this after Phase 1 so the compiler work is not blocked on it.

---

## Phase 5 — Rendering hardening, security, and toolchain

### Task 5.1: Re-enable and actually pass the 16 ignored Skia renderer tests

**Objective:** Stop hiding the most fragile subsystem.

`#[ignore = "requires skia-native feature and GPU hardware"]` covers `skia_text_render`, `skia_border_render`, `skia_border_radius_render`, `skia_dpr_render`, `skia_text_wrap_render`, `skia_hover_render`, `skia_image_filter_render`, `skia_gpu_surface`, `skia_draw`, `skia_init`, `skia_vnode_render`, `skia_batching_render`, plus 2 inline in `skia_render.rs:2708,2739`.

**This has already caused a real miss.** `velox-cli/Cargo.toml` carries the comment: *"an unscoped-merge defect survived a green suite"* — and that is precisely the defect now failing in Task 0.1.

`skia-native.yml` already runs `cargo test -p velox-renderer --features skia-native -- --ignored`. Confirm the job is green, and add a pixel-determinism gate so a rendering regression fails CI instead of shipping.

### Task 5.2: Un-ignore the remaining tests, or justify each

`frame_cost_bench.rs:187` (`#[ignore = "measurement tool"]`) is legitimate — a benchmark is not a correctness gate. The 3 in `velox-sfc/tests/integration_compile.rs` (`#[ignore = "slow"]`) should run in CI, where time is cheap and they prove generated code actually compiles.

### Task 5.3: Add a coverage gate

Run `cargo llvm-cov --workspace --fail-under-lines 70` in CI. Uncovered public surface to target: `velox-core/src/ergonomics.rs` (504 lines), `velox-renderer/src/{presenter,event_binding,text,viewport,hmr}.rs`, `velox-style/src/{visual_effects,fonts}.rs`, `velox-cli/src/commands/{add,lint,init}.rs`.

### Task 5.4: Sweep dead code and legacy branches

**Objective:** Turn the partial inventory into a complete one.

Run a build capturing `never used` / `never read` / `never constructed` warnings across the workspace. Already confirmed: the dead wgpu/`skia_gl.rs` backend, `presenter.rs:289` `is_degraded`, nine `#[allow(dead_code)]` in `layout.rs`/`text_wrap.rs`, the dead `reconcile_keyed_children`. Each `#[allow]` gets a justifying comment or is removed.

Delete the 18 branches fully merged into HEAD (confirmed via `git branch --merged HEAD`): `alpha`, `backup-remove-AGENTS`, `feat/template-compiler-mvp`, `fix/0A` through `fix/2B`, `fix/5B-parser-robustness`, `main`. Keep `cascade/velox-pre-release-reports-and-todo-md-43ac97` and `copilot/release-initial-version` (unmerged). Confirm with the user before deleting `main`.

### Task 5.5: Security review

**Objective:** Close the four items never verified.

1. **`cargo audit` on `Cargo.lock`** — never run. Report vulnerable and yanked crates; decide on pinning policy.
2. **The 18 `unsafe` blocks** — enumerated but never individually assessed. Locations: `velox-renderer/src/skia_gl.rs:154,224,289,292`; `skia_surface.rs:248`; `presenter.rs:72,234,246,444,451,459,460`; `lib.rs:2615,2952`; `velox-cli/src/commands/init.rs:159,165,175,180`. Note `presenter.rs:444-460` is an env-var guard that calls `unsafe { std::env::set_var }` — soundness depends entirely on the single-threaded assumption holding, and the guard type should document that invariant.

   **Audit against the three classes rust-skia's own history says matter** (N10). These are not hypothetical — they are the actual bugs that project hit:
   - **Ownership, not arithmetic.** rust-skia shipped a double-free by wrapping a *borrowed* `SkCanvas::recorder()` pointer in an owning `RefHandle` whose drop called `delete`, when the surface already owned it. For every velox `unsafe` FFI wrapper: does this handle own, or borrow, the underlying object? If it borrows, the wrapper must be lifetime-bound, not owning.
   - **`Send` without `Sync`.** rust-skia's own guidance: "Sync would let two Contexts race on it." This is directly relevant to Task 1.6's eight `Arc` not-Send/Sync clippy warnings — if those are graphics-context handles, **the `Sync` is the bug**, not the warning. Do not `#[allow]` them away.
   - **`zeroed().assume_init()` is unsound for C++ types** — "a zeroed value of an arbitrary C++ type need not be a valid Rust value." `grep -rn 'zeroed()' --include=*.rs velox-*/src` and audit every hit.

   Mitigating context (N4): velox is on the CPU raster path with no live `DirectContext`, so the highest-risk class (GPU resource lifetime) is currently dormant. That lowers urgency; it does not remove the work.
3. **Path traversal in `init.rs`/`add.rs`** — do they follow symlinks or permit `..` escapes when scaffolding?
4. **Command injection** at the `Command::new("cargo")` sites (`dev.rs:350,381`).

**Already verified, no action needed:** `string_lit` (`template_codegen.rs:3327`) is not injection-breakout-able — probed with NUL, U+2028/2029, BOM, form feed, backtick, `${}`; all produce valid non-breaking Rust tokens. The comment "good enough for tests" understates it; correct the comment. `script_setup.content` is embedded as raw Rust (`codegen.rs:157`) — inherent to a source-generating compiler, same trust model as `build.rs`, but **document it explicitly**: a `.vx` file from an untrusted source executes arbitrary code at compile time. If community templates are ever accepted, a sandboxed build story is required.

### Task 5.6: Document the renderer's hot loop

`velox-renderer/src/lib.rs` contains the event loop twice — a plain path (~:1311-1880) and an HMR path (~:2004-2600) — with duplicated arms. The comments show they have already drifted once (`fix/0A-hmr-deadlock`: "This arm used to be a bare `_ => {}` stub with no `focused_input` in scope at all, so typing did nothing under HMR"). Extract the shared arm bodies so the two loops cannot diverge again, and add a test that exercises both paths through one shared implementation.

**Research notes:**
- **The duplication is also a release-mode bloat risk (N11).** Vite's docs require HMR code to be guarded so it "can be tree-shaken in production" (`if (import.meta.hot) { }`). Velox's equivalent is a `cfg` on the HMR loop, not two full copies in one binary.
- **Do not "fix" `ControlFlow::Wait` (N8).** Per the winit docs it is "ideal for non-game applications that only update in response to user input, and uses significantly less power/CPU time than `ControlFlow::Poll`." It *looks* like a bug ("event loop not ticking") and a future maintainer may switch it to `Poll`, burning CPU continuously and directly contradicting the resource-frugal requirement. **Add a comment at the call site citing this.**
- **Use `ControlFlow::WaitUntil` + `AboutToWait` for anything time-based.** The existing `CaretBlinkTicker` (530 ms cadence, focus-gated, correct `Drop` at `lib.rs:293-301`) is the pattern to copy — a focus-gated blink costs **zero** CPU when nothing is focused. This is the clock `transition` will need when Task 4.7 implements it.

### Task 5.7: Build the velox-specific skills

The user asked for project-scoped skills. Build them **after** the fixes land, so they encode the corrected design rather than the current defects:

1. **`velox-frame-budget`** — the `frame_cost_bench` harness plus thresholds; fails the build if layout exceeds budget at a fixed tree size or if depth scaling exceeds linear. Directly prevents 2^depth from returning.
2. **`velox-reactivity-safety`** — the three signal bugs as regression tests, plus the rules: never identify an effect by address; never hold a `RefCell` borrow across an effect body; never keep a strong ref in thread-local storage.
3. **`velox-hmr-contract`** — what each edit type must do (style → live swap; template → rerender preserving state; script → reload), with the dev-server verification steps.
4. **`velox-html-parity`** — `ua.css` as a contract; adding a tag means updating `ua.css` **and** the display table from Task 4.3 in the same commit, enforced by the existing sync test. Include the Blink `html.css` values from Task 4.2 as the reference table.
5. **`velox-audit-workflow`** — the GitNexus/impact discipline `AGENTS.md` mandates, including that `risk: UNKNOWN` means unresolved, never "safe."
6. **`velox-declaration-honesty`** (NEW) — before adding a CSS property, decide: implement it, or add it to the `unimplemented` list. A property that parses but does not render is a bug, not a feature. This skill exists specifically to stop N1 from recurring.

### Task 5.8: Decide the rendering target — CPU reference vs GPU (NEW — N4, needs your call)

**Objective:** Decide deliberately, and record the decision, before Task 2.4's paint optimizations are tuned against the wrong target.

**Current state (verified):** `skia_surface.rs` creates `sk::surfaces::raster_n32_premul` — a **CPU raster surface** — with `_gpu_ctx: None` and `_gl_ctx: None`. No `DirectContext` is ever created. `grep 'resource_cache|set_resource_limit|purge|memory_budget'` → **zero hits**.

**What that means:**
- The benchmark's 1.6 ms "paint+readback" is **software rasterization plus a full-surface readback to RGBA**. It is not representative of a GPU path, and it is the dominant per-frame cost after layout is fixed.
- If a GPU context is ever added, Skia's `GrDirectContext` GPU resource cache **defaults to 256 MB and can occupy twice that when full**, and velox currently sets no limit. That is a latent resource-exhaustion risk.
- CPU raster means no subpixel text antialiasing, so text will never match a browser pixel-for-pixel.

**The decision:**

| Option | Pros | Cons |
|---|---|---|
| **A. Stay CPU** (recommended) | Deterministic — the 949-test suite and the pixel-proof tests depend on it; no device-lost; no 256 MB cache; works headless everywhere; matches Impeller's stated priority of *predictable* over fast | Slower for large paints; text AA differs from browsers |
| B. Add GPU as opt-in | Faster large paints | Loses deterministic pixel tests; needs explicit cache limits, device-lost handling, and a GL context lifecycle (currently `_gl_ctx` is dead code with a per-frame `make_current()` call at `skia_surface.rs:~48`) |

**Recommendation: Option A**, with any GPU path added later as a separate feature-gated backend that runs the pixel tests in a non-deterministic-tolerant mode. Impeller's own design rationale is "the engine controls caching and caches explicitly" and "predictable performance" — that is the standard worth holding velox to, and a framework that ships deterministic golden pixels is far more valuable than 3 ms of paint time.

**If you pick B**, the minimum work is: set `DirectContext` resource limits explicitly, implement device-lost recovery, and gate the pixel tests. Do not leave it at "no limits configured."

**Step 1:** Record the decision in `docs/RENDERING.md` with the reasoning.
**Step 2:** If A, add a comment at the `raster_n32_premul` call site noting it is deliberate and that the pixel tests depend on it — so a future maintainer does not "optimize" it into a GPU context and break 949 tests.
**Step 3:** If B, add a `sk_gpu_tests` feature and a CI job, and set the resource-cache limit.

---

## Phase 6 — Property-based testing and fuzzing (NEW)

**Gate for this phase:** a `proptest` suite exists and runs in CI; no input can panic the compiler; every found regression is persisted in `tests/proptest-regressions/`.

Velox's 949 tests are overwhelmingly example-based. That leaves whole classes of bug uncaught — a hand-written test only checks the case its author imagined. `proptest` is the established Rust choice (`quickcheck` is the alternative; per the Rust Project Primer and lib.rs, proptest is more flexible for complex input shapes, which velox needs).

### Task 6.1: Implement `proptest` strategies for velox's types

**Objective:** Make random-but-valid velox inputs constructible in tests.

**Files:** `velox-dom/src/proptest_impl.rs` (new, `#[cfg(any(test, feature = "proptest"))])`)

Implement `Arbitrary` for `VNode`, `Props`, `Length`, `Color`, and a bounded `Stylesheet`. Bound the generators hard — depth ≤ 12, children ≤ 8, string length ≤ 64. An unbounded tree generator will find the exponential blowup as a *hang*, which proptest reports as a failure but slowly.

```rust
// velox-dom/src/proptest_impl.rs
impl Arbitrary for VNodeStrategy { /* depth-bounded tree */ }
```

### Task 6.2: Property — layout is pure, total, and idempotent

**Objective:** Catch the bug class the 2^depth defect belongs to.

```rust
// velox-dom/tests/layout_properties.rs
proptest! {
    #[test]
    fn layout_never_panics(tree in any_bounded_vnode(), css in any_bounded_css()) {
        let styled = style_vnode_with_hover(&tree, &ss, None);
        let _ = compute_layout(&styled, 1200, 800);
    }

    #[test]
    fn layout_is_idempotent(tree in any_bounded_vnode(), css in any_bounded_css()) {
        let a = compute_layout(&styled_a, 1200, 800);
        let b = compute_layout(&styled_a, 1200, 800);   // same input
        prop_assert_eq!(a, b, "layout must be a pure function");
    }

    #[test]
    fn deeper_nesting_never_costs_exponentially(
        d in 1usize..12) {
        // The regression guard for the 2^depth defect. If this fails,
        // exponential layout has returned.
        let t = Instant::now();
        let _ = compute_layout(&nested(d), 1200, 800);
        prop_assert!(t.elapsed() < Duration::from_millis(50), "depth {d} too slow");
    }
}
```

**`layout_is_idempotent` is the highest-value of the three.** A memoization cache (Task 2.2) that depends on evaluation order will violate it — and this property would have caught that cache bug long before it showed up as a visual glitch.

### Task 6.3: Property — the cascade is order-independent

```rust
// velox-style/tests/cascade_properties.rs
proptest! {
    #[test]
    fn declaration_order_does_not_change_the_result(decls in prop::collection::vec((css_prop(), css_value()), 1..8)) {
        let mut forward = ComputedStyle::default();
        for (p, v) in &decls { forward.set_property(p, v); }
        let mut reverse = ComputedStyle::default();
        for (p, v) in decls.iter().rev() { reverse.set_property(p, v); }
        prop_assert_eq!(forward, reverse, "shorthand/longhand ordering must not matter");
    }
}
```

Catches shorthand/longhand ordering bugs — the classic silent-CSS-error class. Note the caveat: *within one rule*, order must not matter. Cascading *across* rules with different specificity must. Test them separately.

### Task 6.4: Property — the compiler never panics

```rust
// velox-sfc/tests/compile_properties.rs
proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..Default::default() })]
    #[test]
    fn arbitrary_source_compiles_or_errors_but_never_panics(src in ".*") {
        let _ = velox_sfc::sfc::parse_sfc(&src);   // Result is fine; panic is not
    }

    #[test]
    fn arbitrary_css_parses_or_errors_but_never_panics(css in ".*") {
        let _ = velox_style::Stylesheet::parse(&css);
    }
}
```

This is also the empirical validation of the raw-script trust boundary (Task 5.5.5): fuzzing `.vx` input is the only way to know the "it escapes correctly" argument from `string_lit` probing actually holds on adversarial input. Every failure persists automatically to `tests/proptest-regressions/*.txt` — commit those, so each bug becomes a permanent regression test.

### Task 6.5: `cargo-fuzz` targets for the compiler

Proptest and `cargo-fuzz` share `Arbitrary` types, so Task 6.1's work carries over directly. Two targets: `fuzz_sfc` (parse + codegen a `.vx` file) and `fuzz_css`. Run in a nightly CI job with a bounded corpus and `-max_total_time=300`.

### Task 6.6: CI wiring

```yaml
# .github/workflows/proptest.yml
- run: cargo test --workspace --proptest 1000     # 1000 cases in CI
- run: cargo +nightly fuzz run fuzz_sfc -- -max_total_time=300
  continue-on-error: true                          # fuzzing reports, doesn't gate
```

**Important:** commit `tests/proptest-regressions/` to version control. lib.rs and the Rust Project Primer both stress this — it is the mechanism that converts a fuzz finding into a permanent regression test. Add `tests/proptest-regressions/` to `.gitignore` **only** if you intend to regenerate; for velox you want them tracked.

---

## Verification gates

| Phase | Gate | Command |
|---|---|---|
| 0 | Suite green, fmt clean, clippy clean | `cargo test --workspace && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings` |
| 1 | Reentrancy, id-reuse, and computed-drop tests pass; deep nesting is a diagnostic | `cargo test -p velox-core --test signal_reentrancy_tests --test signal_effect_id_tests --test signal_computed_drop_tests && cargo test -p velox-sfc --test depth_limit_tests` |
| 2 | Layout ≤ 200 µs @ 10 todos; depth-18 < 50 ms; keystroke at 200 todos < 4 ms | `cargo test -p velox-renderer --release --features skia-native --test frame_cost_bench -- --ignored --nocapture` |
| 3 | Style edit applies live with no rebuild; script reload < 1 s; no orphans | manual + `cargo test -p velox-renderer --test hmr_style_update -p velox-cli --test watcher_tests` |
| 4 | Parity suite green; deviations documented | `cargo test -p velox-dom --test default_parity -p velox-style --test ua_completeness` |
| 5 | No ignored tests without justification; coverage ≥ 70%; audit clean | `cargo llvm-cov --workspace --fail-under-lines 70 && cargo audit` |
| 6 | Proptest suite green at 1000 cases; no compiler panic; regression corpus committed | `cargo test --workspace --proptest 1000` |

**Ordering rationale.** Phase 0 first because nothing is verifiable against a red suite. Phase 1 before 2 because the reactivity panics crash apps and would corrupt any performance measurement. Task 2.1 before 2.2 because removing the triple descent is a small, local, low-risk change with a huge payoff, whereas invalidation is a large architectural change needing Task 2.1's correctness tests as a safety net. **Task 4.8 immediately after 2.1**, because the automatic minimum size depends on the base-size/hypothetical-size split and the two will conflict if implemented in the wrong order. Task 3.3 after 2.2 because state-preserving rerender requires node identity. Task 5.1 before trusting any pixel proof. Phase 6 last, because its property types should model the *fixed* `VNode` and `ComputedStyle` — writing `Arbitrary` impls against the current defective shapes would mean writing them twice.

**Risk register.** 2.2 (layout invalidation) is the highest-risk task: a stale-cache bug is silent and visual. Its gate is that all golden and pixel tests stay green plus two new targeted tests, and it must not start until 2.1 lands. **4.8 (min-size + automatic minimum size) is the second highest risk** — it changes layout results for many existing trees and the spec's base-size vs hypothetical-size ordering is easy to invert. Its gate is the full golden suite plus the new `min_size_tests`. 4.3 (subpixel viewports) will perturb golden tests — widen tolerances deliberately with a written justification rather than snapping to keep assertions green. 1.1 changes the effect representation and touches every reactive path — the full `velox-core` suite plus the renderer's event tests must pass before merge. 5.8 is a **decision risk, not a code risk**: choosing the GPU path after Task 2.4 has been tuned against CPU timings means redoing that tuning, which is why 5.8 is flagged as needing your call before Phase 2 measurement gates are trusted.

**Corrected from the prior draft.** This plan previously listed `box-shadow`/`text-shadow` under "explicitly out of scope, no confirmed gap." **That was wrong.** Research (N1) verified `grep -rn 'box_shadow\|shadow' velox-renderer/src` returns **zero** — `box-shadow` is parsed into `ComputedStyle` and never rendered. The same is true of `transform` and `transition`. The 2026-09-24 audit's claim that a `visual_effects.rs` implementation exists does not hold for the actual render path. These are now tracked as Task 4.7 (make them visible) and Task 5.8's follow-on (implement them).

**Explicitly out of scope.** The wgpu backend (dead; delete in 5.4 rather than revive). Full table layout (needs a real algorithm; the UA rules are listed in Task 4.2 Tier 4 to apply once that work happens). `writing-mode` and `aspect-ratio` (imply layout modes velox does not have). `flush: 'sync'` reactivity semantics (Vue's own docs call it a performance trap; do not expose the option). Floating layout (`float`/`clear` — defensible to omit, since css-flexbox-1 §3 says they have no effect on flex items, but it must be documented per Task 4.5).
