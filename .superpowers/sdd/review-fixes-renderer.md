# Review fixes — `velox-renderer`

Scope: geometry-divergence findings in the read-only review of `git diff` scoped to
`velox-renderer/` on branch `feat/premium-boilerplate-core-components`.

**There is no memory-safety defect in this diff and none was introduced.** The review's
framing for M-1 as re-opening an ownership hole was wrong, and nothing below is described
as a soundness fix.

Write scope held to `velox-renderer/**`. Nothing staged, nothing committed, nothing
outside the crate touched. Every line citation in the review was re-verified at source
before use; one was already stale (see §8).

---

## 1. Falsification ledger

Every new test was proven able to fail by mutating the production code it covers. Each
mutation was reverted from a backup immediately after.

| ID | Mutation to production code | Test(s) that went RED | Confirmed RED output |
|---|---|---|---|
| **M1** | The I-1 fix itself: hit-test lane's inherited size reverted `PAINT_ROOT_FONT_SIZE` → `velox_dom::layout::DEFAULT_ROOT_FONT_SIZE` (16), and `metrics.font_size` → the deleted `resolve_font_size` | `the_caret_lane_measures_at_the_size_an_undeclared_font_size_paints_at`, `em_padding_resolves_against_the_same_inherited_size_in_both_lanes`, `a_relative_font_size_reaches_the_caret_lane_at_its_resolved_size` | `disagree at 141 of 200 coordinates, by up to 2 character(s); first divergence: Some((37.0, 0, 1))` / `131 of 200 … Some((83.0, 0, 1))` / `99 of 200 … Some((43.0, 0, 1))` |
| **M2-1** | Deleted the `apply_border_style(… Solid)` reset immediately before the focus ring's `draw_rrect` | `a_dashed_border_does_not_make_the_focus_ring_dashed` | `12/24 pixels in the ring column x=4 (y 8..32) are not the accent colour, first at y=Some(8)` |
| **M3-1** | Replaced all three `super::REM_ROOT_FONT_SIZE` root_size arguments back to `font_size` | `parse_border_resolves_rem_against_the_root_not_the_element`, `parse_padding_resolves_rem_against_the_root_not_the_element` | `a \`rem\` border width resolved against the element's own font size…` / `\`rem\` resolved against the element's own font size` |
| **M4-1** | Dropped `&& scale != 1.0` from `measure_text`'s snap guard | `the_caret_lane_and_the_paint_lane_measure_the_same_advance` | `advance for "Wg" at 16.5px, scale 1: the caret lane measured 26 but the paint lane measured 25` |
| **M-1** | *none attempted* | — | See §7 |

Under M3-1 the pre-existing `parse_text_style_rem_agrees_with_the_dom_layout_root_basis`
stayed **GREEN**, which is the point: it covers a fourth site that was already correct, so
no test in the file distinguished the three broken ones. That is how the bug survived.

### Two mutations were refused

- An I-4 assertion of the form *"a 16.5px run must measure strictly between the 16px and
  17px advances"* was **written, run, and deleted**. It FAILED — not because the fix is
  wrong but because the bundled default face quantizes advances to whole logical pixels
  (`"Hello world"` measures 87px at both 16px and 16.5px; `"MMMMMMMM"` is 120 at all three).
  It tested a property of the font, not the bug, and it would have been a test that passes
  for the wrong reason. The surviving test picks size/text pairs that genuinely
  discriminate (`"Wg"`, `"Hello world"`, `"0"` at 16.5 vs 17) and records the quantization
  in a comment so nobody re-adds the vacuous form. The probe used to establish this was
  itself removed.
- No mutation for M-1, stated plainly rather than manufactured.

---

## 2. I-1 — the caret measured in a different font size than the painter

**Fixed.** `velox-renderer/src/lib.rs`, `velox-renderer/src/skia_render.rs`.

Three constants for one quantity: paint's root base `TextStyle` was `14.0` written out
twice (`skia_render.rs` headless and winit lanes), the hit-test lane carried `16.0`, and
`resolve_font_size`'s `parse_px` required a literal `px` suffix so `em`/`%`/`rem` and bare
inheritance all collapsed to `16.0`.

Deleted — the fix is mostly a deletion:

| Removed | Location | Why it was the bug |
|---|---|---|
| `const DEFAULT_INPUT_FONT_SIZE: f32 = 16.0` | `lib.rs:223-226` | the third hardcoded answer to "how big is this field" |
| `fn parse_px(v: &str) -> Option<f32>` | `lib.rs:350-355` | required a literal `px` suffix; `em`/`%`/`rem` silently fell through |
| `fn resolve_font_size(props) -> f32` | `lib.rs:357-366` | its only caller was the hit-test lane |

Added — one named constant, reachable from both lanes:

- `skia_render.rs:23` — `pub(crate) const PAINT_ROOT_FONT_SIZE: f32 = 14.0`, the painter's
  root `TextStyle` size, i.e. the size an element that declares no `font-size` paints at.
  Used at `skia_render.rs:1157` and `:2040` (were bare `14.0`) and as the
  `inherited_font_size` argument in the hit-test lane.

Renamed `resolve_text_origin_x` → `resolve_text_metrics` (`lib.rs:388-424`), now returning
the whole `InputTextMetrics` rather than just `.text_left`. The call site
(`lib.rs:861-882`) makes **one** call and takes both fields:

```rust
let metrics = resolve_text_metrics(props, rect, viewport);
let font_size = metrics.font_size;
let config = crate::text::TextRenderConfig::new(&resolve_font_family(props), font_size);
let measure = |s: &str| crate::text::TextMeasurer::measure_with_scale(s, &config, scale_factor).0;
caret = Some(crate::events::click_to_char_index(&value, rect, x, metrics.text_left, font_size, &measure));
```

This removes a *class*: the origin was shared while the advances were not, so any future
field of `InputTextMetrics` is either used by both lanes or by neither.

**Deviation from the review, deliberate.** The review specified deletion only and predicted
a test comparing the hit-test size to `input_text_metrics(...)`'s at 16.0 vs 14.0. That test
cannot be GREEN after a pure deletion: `input_metrics::resolve_font_size` returns the
*passed-in* `inherited_font_size` when nothing is declared, and the review's own fix path
left `lib.rs` passing 16.0 there. Naming the painter's constant and threading it through was
required to make the review's test expressible. Same defect, one fewer answer.

The false comment at the old `lib.rs:394-396` ("it matches what the paint lane inherits for
a field at the top level") is gone, replaced by one stating what the argument actually is.

### Tests — `lib.rs`, `edit_focus_tests` module

All three were RED before the fix. All are **differential**: two styles that paint
identically must yield the same caret at the same x. No font metric, no advance sum, no
geometry constant — so they cannot pass by re-deriving the painter's own arithmetic. Each
asserts some click resolved into `0 < i < len` first, so "both lanes return 0" cannot
satisfy them.

| Test | Pair | RED |
|---|---|---|
| `the_caret_lane_measures_at_the_size_an_undeclared_font_size_paints_at` (`lib.rs:3786`) | no `font-size` vs `font-size:14px` | 141/200 coords, ≤2 chars |
| `em_padding_resolves_against_the_same_inherited_size_in_both_lanes` (`lib.rs:3805`) | `padding:0 4em` vs same + `font-size:14px` | 131/200 coords, ≤2 chars |
| `a_relative_font_size_reaches_the_caret_lane_at_its_resolved_size` (`lib.rs:3820`) | `font-size:2em` vs `font-size:28px` | 99/200 coords, ≤2 chars |

Helpers: `caret_from_click` (`lib.rs:3717`) pre-focuses via `.focused_at(0)` because
`apply_click_focus` sends the caret to END on focus gain, which would make every comparison
vacuous; `assert_carets_agree_across_field` (`lib.rs:3741`) sweeps x over 200 coordinates
rather than probing one — a single probe can land on a char boundary where two disagreeing
advances agree — and reports the count, worst divergence and first divergence on failure.

Three existing test call sites updated for the rename: `lib.rs:3684`, `:3696`, `:3705`.

---

## 3. I-2 — the focus ring inherited the border's dash

**Fixed.** `skia_render.rs:2594-2617`.

`paints.stroke` is frame-scoped, `apply_border_style` is the only thing that installs or
clears a path effect, and its last call before the ring was at `:2552`/`:2563` — inside the
border draw, before the ring. `border: 2px dashed #888` left intervals `[6.0, 6.0]` live,
so the ring was stroked dashed.

Inserted before the ring's `set_color`/`draw_rrect`:

```rust
apply_border_style(
    &mut paints.stroke,
    &BorderSpec {
        width: 2.0,
        color: sk::Color::from_argb(255, 52, 120, 246),
        style: velox_dom::style::BorderStyle::Solid,
    },
);
```

This mirrors what `:2563-2570` already does for the fallback border. `apply_border_style`
sets **only** the path effect, not the stroke width, so there is no stroke-width
interaction.

### Test — `tests/caret_pixels.rs:610`

`a_dashed_border_does_not_make_the_focus_ring_dashed` renders a focused and an unfocused
`border:2px dashed #888888` field and requires the **entire** sampled run of the ring
column to be `ACCENT`.

Per the review's warning, it does **not** re-derive geometry from the painter: the ring
column is `box.x + 2 (declared border width) + 2 (documented inset)`, box constants, not
the painter's own inset helper. And the "must be solid" claim is checked by **contiguity**,
which no choice of constants can fake — a dash has to break the run. It first asserts the
**unfocused** render has zero accent in that column, so continuity cannot be faked by page
or border ink.

---

## 4. I-3 — `rem` resolved against the element's own font size

**Fixed.** `skia_render.rs:320`, `:450`, `:572`.

`Length::to_px(parent_size, root_size, viewport)` is `Rem(v) => v * root_size`. Three
callers passed the element's own size as *both* arguments:

| Site | Function |
|---|---|
| `skia_render.rs:320` | `parse_border_value` |
| `skia_render.rs:450` | `parse_padding` |
| `skia_render.rs:572` | `padding-*` longhand arm of `parse_style_attr` |

`input_metrics.rs:186` already did it correctly (`len.to_px(font_size, DEFAULT_ROOT_FONT_SIZE, viewport)`),
as does `parse_text_style` at `skia_render.rs:636`. All three now pass
`super::REM_ROOT_FONT_SIZE` (`skia_render.rs:33`, = `velox_dom::layout::DEFAULT_ROOT_FONT_SIZE`),
so `rem` and `em` finally have different arguments.

The review's magnitude estimate is accepted and not inflated: `BoxStyle::padding` feeds only
the vertical centring of an input's text (`skia_render.rs:2342-2345`), where `rem` cancels
for symmetric padding. What survives is (a) asymmetric `rem` padding and (b) `rem` in
`border-width`, which has no cancellation — `border: 1rem` at `font-size:32px` stroked 32px
while the metrics inset by 16px, and since the border is painted after `canvas.restore()`
it cannot cover over-inset text.

### Tests

- `parse_padding_resolves_rem_against_the_root_not_the_element` — `padding_of("1rem", 32.0)`
  is 16 on all sides. The load-bearing line is
  `assert_eq!(padding_of("1rem", 16.0), padding_of("1rem", 32.0))`: invariance under the
  element's size is the property `rem` has and `em` does not, so it cannot pass by
  coincidence.
- `parse_border_resolves_rem_against_the_root_not_the_element` — `border_of("1rem solid
  #ff0000", 32.0).width == 16.0`, invariant across font sizes 16/32/64, with `1em` at 32px
  → 32.0 as the control that pins *which* argument changed.

**Found by the review and not stated in it:** a `rem` assertion did exist — in the *border*
suite, `border_value_resolves_relative_lengths` (`skia_render.rs:4004`) — and it asserted
`18.0` for `border_of("1rem solid red", 18.0)`, with a comment that read:

> "`rem` has no root font size in scope anywhere in the paint path, so it resolves against
> the element font size… a `rem` border on an element that *has* overridden it will be
> wrong. Tracked as a known limitation rather than silently dropped."

That is the defect written down as intended behaviour. Corrected to `16.0` with the message
"(`rem` is a root unit, not an `em`)". This is why mutation M3-1 had to be run against both
new tests *and* this existing one.

---

## 5. I-4 — two snap rules disagree at exactly `scale_factor == 1.0`

**Fixed.** `skia_render.rs`, `measure_text`.

`FontCache::snapped_size` (`:1649-1667`) and `TextMeasurer::snapped_size`
(`text.rs:110`) both early-return unrounded at `scale == 1.0`. The free `measure_text`
(used by the hit-test lane via `TextMeasurer::measure_with_scale`) snapped for **any**
finite positive scale, including 1.0 — so `<input style="font-size:16.5px">` painted at
16.5 and measured at 17.0, a 3% advance error at the most common desktop scale.

Guard now `if scale.is_finite() && scale > 0.0 && scale != 1.0`, matching both siblings.
Doc comment extended with *why*, naming them and the concrete symptom.

### Test — `the_caret_lane_and_the_paint_lane_measure_the_same_advance`

Differential, not a re-derived number: for scale ∈ {1.0, 1.25, 1.5, 2.0} × size ∈ {16.5,
13.333, 0.9375×16, 21.0} × text ∈ {20×`M`, `iiiii`, `Wg`, `Hello world`, `0`}, it asserts
`measure_text(...).width == FontCache::measure_run(SEAM, size, text).width`.
`measure_text` re-snaps internally before delegating, so this compares two *independent*
snap decisions.

The size/text selection is load-bearing and explained in the test: the bundled face
quantizes advances to whole logical pixels, so most pairs measure identically at 16px and
16.5px and would let both lanes disagree while the test stayed green. Only pairs that
actually differ at 16.5 vs 17 are included.

---

## 6. M-1 — `into_parts`: a shape problem

**Fixed as a refactor, and deliberately not described as a soundness fix** in any comment or
in this report.

The review's own verification stands: `GlDirectContext` has no `Drop` impl, `SkiaGlContext`
owns and drops its EGL resources, so `into_parts(self)` was a plain move with no drop glue.
There was no UB and no double free. The real issue was that correctness rested on
`SkiaSurface`'s field *declaration order* matching a type in another file, with a comment
asking readers not to reorder them.

- `skia_surface.rs:31` — the pair `_gpu_ctx: Option<sk::gpu::DirectContext>` +
  `_gl_ctx: Option<SkiaGlContext>` is now a single
  `pub(crate) _gpu: Option<crate::skia_gl::GlDirectContext>`. `GlDirectContext` owns both
  halves and its own field order already guarantees the invariant; keeping it whole deletes
  the ordering question instead of documenting it. Call sites updated: `new_raster:72`,
  `canvas():88`, `present():120`/`:123`, `resize():158-165`, and both construction sites
  (`:233`, `:247`) now pass `_gpu: Some(owned)` with no split.
- `skia_gl.rs` — `into_parts` deleted from the unix impl (was `:114`) and the non-unix stub
  (was `:536`). `grep -n into_parts velox-renderer/src/` now returns nothing. Doc at `:125`
  rewritten to say there is deliberately no way to take the halves apart and to point at
  `dctx_mut()`/`gl()`; the non-unix module doc at `:494` updated to match, since the
  method set exists on every target precisely so `skia_surface.rs` compiles everywhere.

Verified by: `cargo build --features skia-native`, `cargo clippy --features skia`
(the non-unix stub lane), and the full test suite.

---

## 7. Out of scope — recorded, not fixed

- **M-2** dual z-order authority: the paint lane honours the `position: static` gate via
  `layout.rs:3694-3698`, the headless raster path does not. Pre-existing, outside this diff.
- **M-3** stale "fontconfig scan" comment — **comment-only correction applied**, since the
  review called it worth a one-line fix. Verified first at source:
  `FontCache::new_with_scale` (`skia_render.rs:1552-1578`) inserts only `"default"` from
  `load_default_typeface` (6 hardcoded font-file paths, then 2 `include_bytes!` bundles,
  then a 5-family probe) — it never enumerates fontconfig. `get_or_load_family`
  (`:1634-1644`) therefore serves the default face for **every** other family, so
  `font-family: Inter` is a no-op paint-wide. Five comments corrected (`:1818`, `:1877`,
  `:1928`, `:1954`, `:2054`) to say "FontMgr/typeface probe"; the `FontMgr::default()`
  construction is real, only the *scan* was fictional. Added a doc on `get_or_load_family`
  (`:1635-1643`) recording that `font-family` is a no-op **here**, not in the cascade — the
  actionable half. The real fix is a different task and was not attempted.
- **`::placeholder` reads only `color`** (`skia_render.rs:375-388`) while the cascade accepts
  any declaration. Noted; the honest fix belongs in `velox-style`.
- **`parse_color_hex` alpha**: `rgba(0,0,0,50%)` fails to parse and silently becomes opaque;
  the space-separated `rgb(r g b / a)` form is rejected. Pre-existing.
- `PLACEHOLDER_STYLE_ATTR` duplication — **fixed**, inside my scope only. The private
  literal at `skia_render.rs:89` is replaced by `pub use velox_style::PLACEHOLDER_STYLE_ATTR`.
  No circular dependency: this crate already depends on `velox-style` and imports
  `velox_style::{Stylesheet, apply_with_cascade}` at the top of the same file. Nothing in
  `velox-style/**` was edited.
- Also left alone per instructions: the `unix_impl` cfg gate, `non_unix_tests` never
  compiled by CI, `dispatch_keydown` repainting on every keypress, `apply_keydown` unable to
  repair a desynced `focused_input` mirror, the `skia_surface.rs:279-325` unsafe EGL block
  compiling on every target, `skia_gl_lifetime.rs` ignoring, public
  `focused_input_path`/`focus_input_by_id` with no `src/` callers.

---

## 8. Where the review's citations were wrong

- **Already stale at handoff:** the review cited `skia_render.rs:2275` as where paint calls
  `input_text_metrics`, and `:2323` for `metrics.font_size`. Both had shifted; the call is
  now at `:2295` and the field read at `:2343` after the two constants were added. Verified
  by content, not by line number.
- **`apply_border_style` at `:340-361`**, called at `:1199, :2114, :2200, :2527, :2538` —
  all accurate, modulo the same shift. It sets **only** the path effect, not the stroke
  width; the review's fix description is unaffected but worth stating.
- **"No `rem` case exists" in the `parse_padding` suite** is true, but a `rem` assertion
  existed in the *border* suite asserting the wrong value (§4).
- **`measure_text` at `:1896`** — the snap guard is at `:1947` after the additions. Also
  confirmed `TextMeasurer::snapped_size` (`text.rs:110`) already carries `&& scale != 1.0`
  and `measure_with_scale` (`text.rs:142`) double-snaps, which is idempotent at every scale
  *except* 1.0.

---

## 9. Verification

Exact commands and real output. Scoped to `-p velox-renderer`; the workspace gate is not my
job.

```
$ cargo test -p velox-renderer --features skia-native -j 3
53 test binaries ok, 0 FAILED, 342 tests passed, 4 ignored
  (full log: /tmp/opencode/renderer_full2.log)
```

4 ignored are pre-existing: 2 in the lib target (`frame_cost_bench`,
`skia_batching_render`) and 2 in `tests/skia_gl_lifetime.rs` (EGL tests needing a display).

```
$ cargo clippy -p velox-renderer --features skia-native --all-targets -j 3
clean for velox-renderer
```

Two warnings appear in the output and **both are `velox-sfc/src/template_codegen.rs:3488-3489`**
(doc-list indentation) from a sibling agent's crate — not velox-renderer, not mine, not
mine to fix.

```
$ cargo clippy -p velox-renderer --features skia -j 3          # CI's test-feature lane
clean
$ cargo build -p velox-renderer --features skia-native -j 3
clean
```

Test binaries run (53), named: a11y_tree_tests, app_background_render, app_render_test,
backends, caret_pixels (9), dpi_correctness_repro, e2e_tests, edit_typing_contract (10),
event_lifecycle_tests (22), event_loop_arms (13), events_hit_test, events_runtime_tests,
events_tests, focus_blur_api (12), frame_cost_bench (ignored), hit_test (7),
hmr_deadlock_repro (6), hmr_protocol (6), inherited_text_align_render, init_bg_check,
init_preview, input_author_styling (13), input_placeholder (13), integration_tests (12),
keydown_focus (27), lifecycle_renderer_sim (3), mount_tests, overflow_scroll_events (8),
overflow_scroll_render, presenter_pixel_bytes, r8_capture, renderer_api,
single_layout_repro (5), skia_batching_render (ignored), skia_border_radius_render (ignored),
skia_border_render (ignored), skia_dpr_render (ignored), skia_draw (ignored),
skia_gl_lifetime (2 ignored), and the lib target (125 passed, 2 ignored).

`cargo test --workspace` was **not** run, by instruction.

### Hygiene

- `git diff --cached --stat velox-renderer` → empty. Nothing staged, nothing committed.
- Files I wrote: `lib.rs`, `skia_render.rs`, `skia_gl.rs`, `skia_surface.rs`,
  `tests/caret_pixels.rs`. Nothing outside `velox-renderer/`.
- All instrumentation removed. The temporary `probe_snap_sensitivity` test and its
  `println!`s are gone; `grep -n "PROBE\|probe_snap\|dbg!"` over all five files returns
  nothing. The four `eprintln!`s in `lib.rs` (`:1876`, `:1918`, `:2469`, `:2561`) are
  pre-existing — `git diff -U0 velox-renderer/src/lib.rs | grep "^+.*eprintln!"` is empty.
- No `#[allow]` added anywhere. Every fix is a real change.

---

## 10. Deliberately not done

- `font-family` per-family loading (M-3's real half) — a different task.
- `::placeholder` accepting any declaration, and `parse_color_hex` alpha — both belong in
  `velox-style` / pre-existing respectively.
- **No mutation for M-1.** It is a pure structural refactor with no observable behaviour to
  break, and its consumer paths are GPU/EGL, which no test in this crate exercises
  (`tests/skia_gl_lifetime.rs` is 2 ignored). Its verification is build + clippy on both
  feature lanes + the full suite. Saying so is more honest than manufacturing a mutation.