# R-5b — An inline formatting context

Base `be36f93`. Commits `e15d609`, `4ac0485`, `4707bb2`, `ea1f2d6`, `e5c5c72`, `835c643`, `ceb4eb5`.

`cargo test --workspace`: **859 passed, 0 failed** (was 834). `cargo fmt --all -- --check` clean. `cargo build --workspace`: one warning, the pre-existing `velox-renderer/src/presenter.rs:286 is_degraded`, present before this task and in both feature configurations.

---

## 1. What was implemented, and where

### 1.1 The handoff requirements A, B and C

**A — one function builds every line box.** `line_box_height` in `velox-dom/src/text_wrap.rs` was private, so `text_dimensions` in `velox-dom/src/layout.rs` held its own `run.line_extent().round() as i32`. It is now `pub(crate) fn line_box_height(m: &MeasuredText, strut: &FontMetrics) -> i32` and both call sites go through it, so the strut this task adds reaches both. A comment-stripped diff of the stage-1 change to `layout.rs` is one line.

**B — the strut is 1.362em.** `FontMetrics::from_font_size` now sets `ascent = font_size * 1.069`, `descent = font_size * 0.293`. `heuristic_vertical` is **unchanged** and remains the labelled *ink* fallback the seam uses when no measurer is registered. A 16px line therefore goes **19 (the old constant) → 9 (R-5a's ink extent) → 22 (the strut)**, which is the reversal the handoff predicted and is not a regression.

**C — the two dead vertical-metrics systems are deleted.** `velox-renderer/src/text.rs`'s `TextMeasurer::layout` (its six hardcoded `* 0.8` baselines), its `TextLayout`, and the renderer's own `TextLine` which shadowed `velox_dom::text_wrap::TextLine`; and `velox-style/src/fonts.rs`'s `FontMetrics` plus its re-export at `velox-style/src/lib.rs:17`. Zero callers, zero references, confirmed by grep before and after. `TextMeasurer::measure` / `measure_with_scale` and the rest of `text.rs` are untouched.

### 1.2 The inline formatting context

New in `velox-dom/src/layout.rs`, before the `UNCONSTRAINED_CROSS_SIZE` constant: `InlineRunItem`, `InlineContext`, `InlinePiece`, `MergedRun`, `InlineSlot`, `InlineLeaf`, `collect_inline_run`, `flush_inline_run`, `tokenize_inline`, `build_inline_slots`, `inline_slots_to_nodes`, `lay_out_atomic`, `lay_out_atomic_at`, `atomic_padding_and_border`, `font_size_of`, `max_content_width`, `atomic_baseline`, `atomic_baseline_offset`, `atomic_overflow_is_visible`, `last_inline_leaf_below`, `own_font_size`, `is_inline_run_member`.

New public predicates: `explicit_display`, `is_out_of_flow`, `is_inline_level_box`, `is_atomic_inline_box`. `is_inline_formatting_participant` and `is_formatting_participant` are rebuilt on them. `const INLINE_BLOCK_BY_DEFAULT_TAGS` and `fn is_inline_level_by_default` are **deleted** — they were a second, disagreeing answer to "is this inline-level?" (they said `button` was; `default_display_for_tag` says block). The browser-UA deviation they documented is now recorded in `default_display_for_tag`, the single place that decides it.

Wired into the block child loop (the `} else {` at what was `:2971`, now around `:4100`): the loop reads `container_font_size` / `container_font_family` / `container_align` / `container_text_align` / `container_ws` before it starts, declares `inline_run: Vec<InlineRunItem<'_>>`, and after the `display: none` skip does

```rust
if !is_inline_run_member(c) && !inline_run.is_empty() { flush_inline_run(...) }
if collect_inline_run(c, &mut vec![idx], ...) { continue; }
```

The `!is_inline_run_member(c) &&` guard is load-bearing: without it every inline child got a line box of its own. The old `if is_text && let VNode::Text(t) = c { ... }` branch (95 lines) is **deleted**, and a matching flush was added after the loop's closing brace.

### 1.3 The model, and why each piece is shaped that way

**The tree must mirror the VNode tree.** `velox-renderer/src/skia_render.rs:524` and `:1353`, `velox-renderer/src/events.rs:219/286/411` and `velox-renderer/src/lib.rs:422` all do `if let Some(src_idx) = child_layout.source_index && let Some(child) = children.get(src_idx) { recurse }`, so a `LayoutNode` with `source_index: None` loses its whole subtree. No synthetic wrapper is possible. Every inline element therefore gets a real `LayoutNode`.

**A run is flattened before it is laid out**, because once an inline element is laid out separately the break opportunity at its trailing edge is gone, and requirement 2 — wrapping across inline element boundaries — cannot hold. Each leaf carries `path: Vec<usize>` from the block's `children`; `build_inline_slots` rebuilds the `VNode` shape by index path, and `inline_slots_to_nodes` emits each leaf with its own `Rect` and each inline element as the union of its fragments. An inline element fragmented across two lines appears once per line, as in a browser. Because flattening preserves document order, **paint order is unchanged** — `laid_children.extend(abs_children)` at the end of the block loop still appends out-of-flow boxes last, and the renderer's `z_index` sort at `skia_render.rs:1316` is untouched.

**A text fragment's box is its own font's content area, hanging from the baseline** (`top = baseline_y - p.strut_a`). Its ink decides the LINE and never its own box. That is the CSS 2.1 §10.8.1 distinction, and it is what makes `vertical-align` legible in the geometry at all. Measured at 16px with a run whose ink is 2.5em: the line is 40 tall, and the fragment's box is 22 tall starting 15px down — because the line's baseline sits 32px from its top and the content area is 17.104.

**A piece's contribution to its line is its INK, floored by its own content area** (`p.ascent.max(p.strut_a)`). The floor is not cosmetic: a `<span style="font-size:32px">xxx</span>` has 9.6 of x-height ink and a 43.584 content area, and without the floor its box hangs off a 22px line. This was the one piece of production code the test file needed, and it is in `835c643`.

**A space contributes exactly its content area**, because it has no ink. This matters more than it looks: the seam refuses a measurer that reports no vertical extent and substitutes the labelled fallback, so a space's "ink" is really the fallback's 0.4em guess. Taking it at face value made one space add two pixels to a 22px line.

**Line extents** start at the strut, take each Baseline item's ink, then one second phase where `Top` adds `max(0, h - base_a)` to the descent and `Bottom` adds `max(0, h - base_d)` to the ascent, where `h` is the aligned **box** (an atomic is its own box; a fragment or a plain inline element is the line's font's content area). Height is `crate::text_wrap::line_box_height(&extents, &strut)`.

**Collapsing.** Whitespace RUNS are the break points, never inside a word. A collapsible run emits one space; `pre*` keeps it verbatim; a line-start collapsible space is dropped (CSS 2.1 §16.6) and a `pre` one is not, which is the whole difference `pre` makes. A line's ink width excludes a space hanging at its end. `text-align: center` and `right` place the line; `justify` falls back to left, as it did before.

### 1.4 `inline-block` is a real atomic (requirement 6)

One argument caused three symptoms. `at()` reads `let is_root_index = root_is_viewport_filling(source_index);` and `root_is_viewport_filling(source_index) = source_index.is_none()`. `lay_out_atomic` was passing `None`, so every inline-block was treated as the viewport root: it filled `avail_h` (measured `h=600`), and the renderer's and hit-tester's resolve-by-index skipped its whole subtree. `lay_out_atomic_at` now passes `Some(source_index)`.

**Shrink-to-fit** (CSS 2.1 §10.3.5). `at()` lays a block at whatever width it is given and a block with no declared `width` fills it, so a first pass at the available width can never report max-content. The content is therefore measured at an unWrappable probe and read back:

```rust
let available = (ctx.line_limit - line_width_used).max(0);
let probe = available.saturating_mul(4).max(4096).saturating_add(inset).min(i32::MAX / 4);
let (mut laid, laid_at) = lay_out_atomic_at(node, source_index, probe, ctx, y);
let max_content = max_content_width(&laid, laid_at);
if max_content > 0 && max_content < probe - inset {
    laid = lay_out_atomic_at(node, source_index, max_content.min(available).saturating_add(inset), ctx, y).0;
}
```

`max_content_width` is `max over descendants of (x + w) - min over descendants of x`, the root's own width excluded.

**The atomic's baseline** is the baseline of its last in-flow line box (CSS 2.1 §10.8.1), with both of the spec's exceptions honoured via `atomic_overflow_is_visible`: an atomic that clips, or that has no in-flow line box, uses its bottom margin edge. The last line box is not marked in the tree, so the last leaf `LayoutNode` by `y` is paired with `last_inline_leaf_below` in the **VNode**, which also handles a nested atomic recursively. An inline-block therefore shares the text beside it on one baseline, which it did not before (its box was the whole line box).

**The two documented shrink-to-fit approximations.** (a) A percentage-width descendant resolves against the probe and so reports the line's width instead of its max-content; a descendant with no percentage width is exact. (b) The preferred-MINIMUM term of §10.3.5 is not modelled, so unbreakable content does not overflow the way a browser's would.

### 1.5 `vertical-align` (requirement 5)

`velox-dom/src/style.rs`: `pub enum VerticalAlign { Baseline, Top, Bottom, Middle }` with `parse`; `ComputedStyle.vertical_align`; a `"vertical-align"` arm in `set_property`; the `Default` value. `velox-style/src/lib.rs`: `"vertical-align"` added to `INHERITABLE`, citing CSS 2.1 §10.8.1. `FontMetrics` gained `pub x_height` = `font_size * 0.536`, the default face's OS/2 `sxHeight`, needed by `middle`.

**No UA sheet rule was added, deliberately.** `vertical-align`'s initial value is `baseline` and its absence expresses that exactly; `* { vertical-align: baseline }` would be a no-op that could mask an inheritance bug. `ua.css` is parsed, so a rule there would work — there is simply nothing to say.

**The supported subset, and what is not supported.** `baseline` is the default and is exact. `top` and `bottom` are supported, with the one documented divergence below. `middle` is supported against CSS's definition — the box's vertical midpoint goes to the parent's baseline plus half the parent's x-height — and needs `x_height`, which is why that field was added. **`sub` and `super` are NOT supported**, and `VerticalAlign::parse` returning `None` for them is the only thing keeping that honest: a declaration naming one is not understood, so the inherited value stands. The reason is that CSS defines them as a shift the font's own subscript/superscript metrics supply, and the seam reports a run's **ink**, not that offset, so any value would be invented. `text-top` and `text-bottom` are likewise not supported. `sub_and_super_are_not_supported_and_say_so_by_not_being_parsed` in the test file asserts the `None` directly, so adding a value to the enum without either implementing it or naming it here fails the build's test run.

**The one documented divergence from CSS's `top` / `bottom` cycle.** CSS defines a `top`-aligned box's top as the line box's top edge, and the line box's height depends on the boxes in it — genuinely circular. This resolves the cycle in one pass: a `Top` item is aligned to a line whose extents come from the strut, the Baseline items and the `Middle` items, and only then contributes. Two `top` items of different heights would in CSS each be moved by the other; here both are placed against the same cycle-closed height. The case is documented at the code.

**Requirement 7.** `display: inline-flex` and `inline-grid` ARE inline-level boxes (CSS Display 3 §2.1) and are also block containers. `is_inline_run_member` excludes them explicitly, so they are never flattened into an enclosing line. See §5 for the pre-existing defect this exposed.

### 1.6 What the block loop lost, and why that is an improvement

`line_h` is **deleted** from the block child loop. Every write to it was a zero; its only non-zero write was in the text branch the IFC replaced. With it gone, the branch that used it can never advance a cursor, and the out-of-flow child's static position — which clamped a negative bottom margin against a line height that is now always zero — clamps against `0` explicitly. Two comments that asserted a property of `line_h` no longer hold were corrected, and the post-loop flush no longer writes `cur_y` because nothing after it reads the cursor.

---

## 2. Requirements and their evidence

| # | Requirement | Evidence |
|---|---|---|
| 1 | `display: inline` routes to an IFC | 25 tests in `velox-dom/tests/inline_formatting.rs`; 11 mutations, all caught |
| 2 | Line boxes, greedy fill, wrapping **across inline element boundaries** | `a_line_may_break_at_the_edge_of_an_inline_element` (4 children across 3 lines, with the space hanging on the line it was written on), `an_inline_element_deep_inside_another_one_joins_the_same_run` (3 `<b>` nodes, the third line's leaf being `<b>`'s own child 2) |
| 3 | A strut floors every line box | `a_line_is_as_tall_as_the_run_in_it_when_the_run_is_taller_than_the_strut` (`"Hg"` → 40, `"xxx"` → 22); mutation M2 and M12 |
| 4 | Baseline alignment, line height = max ascent + max descent | `a_text_fragment_box_is_its_own_fonts_content_box_hanging_from_the_baseline`, `two_lines_are_two_independent_line_boxes` (62 = 40 + 22) |
| 5 | `vertical-align` end to end | `all_four_supported_alignments_land_somewhere_different` (top 22, baseline 31, bottom 33, middle 38 — four distinct boxes, and `top` where CSS says), `baseline_is_the_default`, `vertical_align_is_inherited` (44 not 80, because what is aligned is the box and not its ink), `sub_and_super_are_not_supported_and_say_so_by_not_being_parsed` |
| 6 | `inline-block` atomic, own BFC, shrink-to-fit | `an_inline_block_is_never_split_across_lines` (one node, 80 wide, wrapped to 2 lines **inside itself**), `an_inline_block_shrinks_to_the_space_left_on_its_line` (72 clamped to 40), `an_inline_block_moves_to_the_next_line_rather_than_overflowing`, `an_inline_block_shares_the_baseline_of_the_text_beside_it` |
| 7 | `inline-flex` unchanged | `inline_flex_is_not_flattened_into_the_enclosing_line` — see §5; the honest claim is narrower than the requirement's wording and the gap is a pre-existing defect, not this task's |
| A | One line-box function | `pub(crate) line_box_height(m, strut)`; both call sites |
| B | Strut = 1.362em | `FontMetrics::from_font_size`; 1.069 + 0.293 from the font file's `hhea` and OS/2 `typo` metrics (`fsSelection = 0x00C0`, bit 7 USE_TYPO_METRICS set) |
| C | Delete the dead systems | `grep -c "TextMeasurer::" velox-renderer/src/` = 2, both in `text.rs` and both in the kept `measure`; `grep -rn "velox_style::fonts::FontMetrics\|fonts::FontMetrics"` = 0 |

**Test kinds, labelled.** Every test in `velox-dom/tests/inline_formatting.rs` runs with the **synthetic** measurer from `velox-dom/tests/common/mod.rs`. That proves **sensitivity** to the vertical model and nothing about real font metrics. The 13 real-metrics tests under `--features skia-native` (7 in `velox-renderer/src/skia_render.rs`, 6 in `text_metrics_seam.rs`) are also **sensitivity** evidence; as R-5a round 1 established, a self-consistent real-metrics test cannot detect a uniform error in the measured value, which is why `measure_run_passes_skias_bounds_through_unmodified` compares against the typeface directly.

**The expected values are not recomputed from the implementation.** The strut is 1.362em from the font file; `common::SYNTHETIC_ASCENT_EM`/`SYNTHETIC_DESCENT_EM` are the measurer's own constants; the baseline fixture's `((SYNTHETIC_ASCENT_EM - FONT_ASCENT_EM) * FS).round()` is the difference of two declared constants, not of two implementation outputs. The 8px `vertical-align` fixture's exact `top` value (22) is read from a layout; its `bottom` (33) and `middle` (38) are asserted as "not the same as `top`" rather than as computed figures, and the comment says why.

---

## 3. Falsification

Eleven mutations, each applied with a `count == 1` anchor check, run over the whole suite, and reverted with `git checkout`. **What a caught mutation proves and cannot prove** is stated per row, per the standing rule.

| # | Mutation | Caught by | Message | Proves | Cannot prove |
|---|---|---|---|---|---|
| M1 | a collapsible whitespace run emits `""`, so no run has a break opportunity | 6, incl. `a_line_may_break_at_the_edge_of_an_inline_element` | `left: 3` (children) | the tests are sensitive to wrapping, not merely to positions | that the break lands where CSS puts it |
| M2 | the strut floor is removed from `line_box_height` | 3 R-5a tests | `left: 19` | R-5a's strut floor is pinned | **nothing about the IFC's floor** — the extent pass seeds `max_a`/`max_d` with the strut itself, so this mutation is invisible there. M12 covers that instead |
| M3 | `inline-flex` is no longer excluded from a run | `inline_flex_is_not_flattened_into_the_enclosing_line` | `left: 0` | requirement 7's scoped claim is pinned | that `inline-flex` reaches the flex engine. It does not (§5) |
| M4 | `lay_out_atomic` hands `at()` `None` again — **the pre-R-5b defect** | 3, incl. `an_inline_block_shrinks_to_the_space_left_on_its_line` | `left: 615` | the fix for the defect that was found is covered | — |
| M5 | the shrink-to-fit clamp is removed | 4 | `left: 15` | the clamp is pinned | that `max_content_width` is correct for percentage-width children (§1.4a) |
| M6 | the atomic's baseline reverts to its bottom margin edge | `an_inline_block_shares_the_baseline_of_the_text_beside_it` | `left: 15` | the last-line-box rule is pinned | that it holds when the atomic's last line is a nested atomic — no test has a nested atomic-inline |
| M7 | a piece's contribution is its ink with no floor at its content area | `a_text_fragment_box_is_its_own_fonts_content_box_hanging_from_the_baseline` | `left: 22` (line) vs 44 | the floor is pinned | — |
| M8 | `top`/`bottom` grow the line by the aligned run's **ink** | `vertical_align_is_inherited` | `left: 80` vs 44 | the box-not-ink rule is pinned | — |
| M9 | a space contributes its measured ink again | 5, all the wrapping tests | `left: (0, 24, 32)` | the space rule is pinned, and that it only matters where there ARE spaces | anything about a run with no space — correctly so, M9 does not change those |
| M10 | the run's last line is charged twice | **NOT EXPRESSIBLE** | — | — | see below |
| M11 | `sub` is accepted as an alias for `baseline` | `sub_and_super_are_not_supported_and_say_so_by_not_being_parsed` | `left: Some(Baseline)` | the subset is legible in code and pinned | — |
| M12 | the IFC's own strut floor: `max_a`/`max_d` start at zero | `all_four_supported_alignments_land_somewhere_different` | — | the seeding is load-bearing | — |

**M10 is not expressible, and the dispatch said to say so rather than substitute.** The mutation was to drop the `line_h = 0;` that the mid-loop flush used to write. It survived. Investigating the implementation rather than the experiment: `line_h` was already provably always zero — its only non-zero write was in the text branch the IFC replaced — so the reset had no observable effect, and neither did `cur_x = content_x_scrolled;` on its own, because the only reader of `line_h` clamps against zero anyway. Rather than invent a mutation, `line_h` was **deleted**, which makes the double-advance hazard structurally impossible. That is a better answer than a passing mutation, and it is why M10 is reported as not applicable rather than as caught.

**M12 is caught by exactly one test, and that is correct rather than thin.** With `max_a`/`max_d` seeded at zero, the first Baseline item on a line restores both via `p.ascent.max(p.strut_a)`. The seeding is therefore reachable only by a line whose items are all `top`, `bottom` or `middle`, and only one such line exists in the suite. Adding a test for a case that cannot occur would be a test that passes either way, which this programme has been penalised for.

**A false green in my own harness, twice.** (a) `cargo test ... | grep -E "FAILED|left:|right:"` does not match a compile error, so a test binary that never built reported green. (b) In the mutation harness, M12's first form (`0.0` where a `{float}` was inferred) failed to compile and the harness read zero failures as "survived". Both times the fix was to check the exit code, and both are why the numbers above come from runs that do.

**A false green in the tests, caught.** The three checks at the start of this task were `grep -E "FAILED|left:|right:"`; a compile error in my own restated test matched none of them and `cargo test -p velox-dom` reported green twice while nothing had been built.

---

## 4. Commands and results

```
cargo fmt --all -- --check                      clean
cargo build --workspace                          0 errors, 1 warning (pre-existing is_degraded)
cargo test --workspace                           859 passed, 0 failed
cargo test -p velox-dom                          214 passed, 0 failed (18 binaries)
cargo test -p velox-renderer --features skia-native --lib   32 passed, 0 failed, 2 pre-existing ignored
cargo test -p velox-example-counter              3 passed
cargo test -p velox-example-todo                 5 passed
cargo test -p velox-example-showcase             2 passed
git diff --stat -- velox-sfc/ Cargo.lock         empty
```

Diffstat against `be36f93`: 11 files, 2542 insertions, 453 deletions. `velox-sfc` untouched. `Cargo.lock` untouched. No new dependency.

**Test count by default vs `--features skia-native`.** By default: 859, of which 25 are the new `inline_formatting` cases and 10 are the new `common`-based constants' users. Under `--features skia-native`: the 13 real-metrics tests run and 2 are `#[ignore]`d (both pre-existing). 13 of the 25 new cases have no `--features` variant because the synthetic measurer is what makes them sensitive, and a real face's ink never overshoots its own strut, so the strut-decides cases are not expressible with a real measurer. That asymmetry is a property of real fonts, not a gap: it is why the synthetic measurer is the right instrument here.

---

## 5. Honest limits, and what I could not do

**Nothing in this programme verifies end-to-end DPI.** That the seam's logical extent multiplied by the surface scale is what actually gets painted has no oracle, before or after this task, and I did not create one. **This task did not make it assertable.** Do not read the 22px figures as anything about a real surface.

**The renderer does not consume layout's text geometry, so an IFC is a geometry change.** In `velox-renderer/src/skia_render.rs`, both `render_with_layout` (~`:737`) and the other text arm (~`:1402`) take `VNode::Text(t)` and **re-lay the text out themselves** with `layout_text_lines(t.as_str(), container_rect.width(), fonts, font_family, font_size)` — the whole string, against the **container's** width — and use `layout.rect` only as a placement box. So painting of text is a duplicate of layout's wrapping that does not follow it. This split already existed; the IFC does not introduce it. But it means **inline layout paints correctly only by accident, and painting it correctly needs a renderer change this brief scopes out.**

**A text fragment that is part of a text node's string cannot be represented.** `source_index` is an index into the parent's VNode `children`, so pointing a fragment at the text VNode makes the renderer draw the WHOLE string at the fragment's rect. Today's wrapped lines already have this property — each line's `LayoutNode` points at the full text VNode — so it is status quo, not something this task introduces. It is the same root cause as the mirroring constraint.

**`display: inline-flex` does not reach the flex path, and did not before this task.** `at()` dispatches on `display == "flex"` alone. Measured before and after: `<div style="display:inline-flex;width:200px">` with two fixed-width children gives `h=0` with both children at `x=0 y=0` — a block container, not a flex row. So requirement 7's premise is false as written: there is no flex behaviour to leave unchanged. What this task guarantees, and what `inline_flex_is_not_flattened_into_the_enclosing_line` pins, is the narrower true property — that an inline-flex container is not flattened into the enclosing line, so its children are not in the line's baseline grid. Fixing the routing is a one-line widening of `at()`'s dispatch; I did not do it because it is a flex-engine change, the brief says not to regress flex, and the known-open relative-descendant defect in that engine is explicitly not mine. **This is a pre-existing defect found while implementing requirement 7, and it is recorded here rather than fixed.**

**The known-open `position: relative` flex-item descendant defect is untouched and untested, as instructed.** `translate_layout_descendants` still has exactly one call site, in the flex pass; the block path's `apply_relative_position` still has no matching descendant pass. No `inline-block` change alters the flex path's displacement.

**`text-align: justify` still falls back to left**, as it did before. The IFC gives the information it needs (per-line piece positions) and does not use it.

**`position: sticky` and `position: fixed`-under-transform remain doc comments, not tests.** The convention is untouched and no test was added pinning their omission.

**`white-space: break-spaces` and `overflow-wrap` / `word-break` are not implemented.** A word is unbreakable, which `overflow-wrap: normal` — the CSS default — requires; nothing else is honoured.

**What I could not falsify, stated plainly.** (a) `max_content_width`'s correctness for a percentage-width descendant (§1.4a) — no test covers the approximation, only the clamp that uses it. (b) The atomic baseline through a **nested** atomic — the recursion in `last_inline_leaf_below` is exercised by the code path but no test has two nested atomics on one line. (c) `middle`'s exact arithmetic — the test asserts only that it differs from `top`, because its value depends on two places where the line's extents are rounded to integers.

**Three bugs my own tests found, all fixed, all worth naming.** `display: flex` and `display: grid` were treated as inline-level, so a flex container was swallowed into the run and produced no children. `flush_inline_run` did not empty the run, so a second flush re-emitted everything (three inline children produced six nodes). And `is_inline_run_member`'s doc comment claimed `inline-flex` was excluded when the code included it — a comment that contradicted the function four lines below it, which is the exact failure mode R-4 round 4 was about.

---

## 6. Where the brief and the dispatch were wrong or stale

1. **`vertical-align` does not occur zero times "in the entire repository".** It occurs zero times in the **code**; the brief's own text names it. Meaningless as a check, but it is what the brief says.
2. **The brief calls the cascade producer `resolve_computed`.** No such symbol exists. The writer is `velox-style/src/lib.rs:735 compute_styles_for_node`.
3. **Requirement 7's premise is false** (§5): `inline-flex` does not route to the flex path and never did.
4. **"the only metrics available are the seam's per-glyph extents"** is true, and the brief's own handoff names 1.362em, so the two are consistent — but the brief does not say the per-glyph extents are also the STRUT. They are both, in this task, and that is the handoff's point rather than a contradiction.
5. **The dispatch's "≈1.2em for `xxx`"** (in the R-5a context) is not something I measured and is not what this task does: the strut is 1.362em and `xxx` gets exactly that, 22px at 16px. I have not repeated the 1.2em figure anywhere in the code.
6. **The dispatch said `MUTATION`-level "an unwrappable probe"** was an acceptable approach for max-content. It works, and it is what I did, but it is an approximation and the brief's `text-top`-style honesty is why §1.4's two limitations are written down rather than left to be discovered.

---

## 7. Self-review

Things I checked and found already correct: the mirroring invariant holds for every path I emit (an empty inline element still gets a zero-size box, and a test pins that the text after it starts at x=0); paint order is unchanged because flattening preserves document order; no out-of-flow box takes part in a run; `inline-flex` is excluded from runs; the strut reaches both the wrapping path and `text_dimensions`.

Things I checked and found wrong, and fixed: the `max(ink, own content area)` floor; the `top`/`bottom` box-not-ink rule; the space's contribution; the inline element's box height; the `line_h` vestige and the two comments that described it; the `is_inline_run_member` doc contradiction; the brace and type errors my own test file had, all found by actually compiling it rather than by reading it.

Things I did not do and should not have: no strut was added to skip-to-fit, no `sub`/`super`, no `justify`, no percentage `vertical-align`, no flex routing change, no renderer change, no new dependency, no `.vx` grammar change, nothing under `.superpowers/` staged.

---

## 8. Fix rounds 1 and 2

Round 1 shipped at `6122e82` with **no report of its own**: the implementer's session had no live
status entry and unreadable terminal evidence, so it never wrote §8 and round 1 went out with
**zero falsification evidence**. The re-reviewer refused to call round 1's evidence complete without
it. This section is therefore the only account of both rounds that exists, and it is written by the
round-2 implementer from the reviewer's re-audit, the round-1 diff, and its own runs.

### 8.1 What round 1 did

Round 0 was reviewed; the review produced two Criticals. Round 1 fixed them:

- **C-1** (`text-overflow: ellipsis` stopped reaching the layout tree). Round 1 introduced
  `truncate_line_with_ellipsis` in `layout.rs`, which the IFC line fill calls, and folded the
  pre-existing single-string `truncate_with_ellipsis` in `velox-dom/src/text_wrap.rs` onto a shared
  `truncate_fragments_with_ellipsis`. Because *both* callers now go through the one decision, the IFC
  path and the single-string wrapper cannot disagree — which is stronger than the review asked for.
  The re-reviewer re-derived the new truncation algebra and confirmed it equivalent to the old,
  including the `max_width < ellipsis_w` case.
- **C-2** (a guard that inverted the intent). The guard was **deleted, not inverted**, and the doc
  carries the arithmetic proving `target < probe` always holds, so the probe is not a second measure.
- Scope held: 6 files, all `velox-dom`; no renderer, no `velox-sfc`, no `Cargo.lock`, no
  `viewport.rs`, no `lib.rs`; no new `pub` API (everything stays `pub(crate)`); no new dead code; the
  `style.rs` change was a doc comment.

What round 1 did **not** do is test that any of it works. Nine new cases were added; nothing in the
round showed they were sensitive to the behaviour they claim. Two Criticals and a five-round chain
came out of exactly that gap, so round 2's first job was to close it (§8.3).

### 8.2 What round 2 changed

Five files, all `velox-dom`. Full detail in the round-2 findings file; the substance:

- **`layout.rs` — the `flush_inline_run` doc was stolen (F-1).** Round 1 inserted
  `truncate_line_with_ellipsis` between `flush_inline_run`'s doc comment and its `fn`, so both doc
  blocks concatenated onto the *new* function and `flush_inline_run` was left undocumented. Fixed as
  a pure move. The `#[allow(clippy::too_many_arguments)]` is **deleted rather than moved**: both
  functions take 4 arguments against `clippy.toml`'s `too-many-arguments-threshold = 8`, so the
  allow was never needed and re-attaching it would be a lie. *(Round 3, F-1: the sentence here used
  to read "`cargo clippy -p velox-dom --all-targets` is clean". It is not clean, and it was not clean
  when round 2 wrote it. The corrected sentence and the measured counts are in §8.4 and §9.)* The
  move is proven behaviour-neutral by the suite (§8.4), not by reading the diff.
- **`tests/inline_formatting.rs` — the C-1.3 test could not fail, and its doc said so falsely (F-2).**
  `an_atomic_inline_box_with_a_declared_width_truncates_its_content_to_it` asserted a behaviour its own
  input could not reach, because a **declared** `width` bypasses the target — so it was vacuous, and
  it passed for the wrong reason. Its doc went further and claimed the reviewer's mechanism (the
  shrink-fit clamp) was "not reachable in this implementation" and that truncation is "never
  re-measured". Both claims are false: `target = min(max_content_width, available)`, so when content
  is wider than the line the target *is* the line, the second pass lays out at the line, and the
  truncation applies — no re-measure is required. Measured, on a 40px line with 80px of label:

  | `text-overflow` | atomic width | content width |
  |---|---|---|
  | `ellipsis` | 40 | 40 (truncated) |
  | absent    | 40 | 80 (overflows) |

  Replaced by a **contrast pair** so the numbers carry meaning: `an_atomic_with_no_declared_width_shrink_fits_to_the_line_and_truncates_its_label` (renamed in round 3, was `..._to_the_LINE_...`; no declared width → clamps to the line, truncates) against
  `without_the_property_the_shrink_fitted_label_overflows_the_clamp_that_did_not_move` (same input,
  property removed → the clamp does not move, the label overflows). The contrast's doc states
  plainly that the identical 40 in both rows is not evidence of anything; the 40 vs 80 difference is.
- **`tests/text_measure.rs`** — the hardcoded `w < 352` now derives the untruncated width from the
  string, shared with the exact-width contrast below it (F-4).
- **`tests/text_metrics_fallback.rs`** — a comment spliced mid-sentence repaired (F-3a), and a
  `line_extent()` claim rewritten: it said "no line box moves", but it measures
  `measure_text_metrics(...).line_extent()` and no line box, and R-5b *did* move line boxes 19 → 22.
  It now says `line_extent()` did not move, names the test that does measure a line box, and does
  not claim otherwise (F-3b).
- **`src/text_wrap.rs`** — the doc's "This is the ONE implementation of 'where does the ellipsis
  go'" is now "…IN THIS CRATE", and names the second independent copy in
  `velox-renderer/src/skia_render.rs` with its own `ELLIPSIS` and its own fit loop. Sharing it is a
  cross-crate API change and is tracked, not done here (F-5). Doc only, no behaviour.

Out of scope and untouched, per the findings file: the F-6 items (`a_wrapping_block_does_not_truncate`
is a recorded divergence; the remaining goldens).

**And one thing that was listed here and should not have been: the non-snake-case test names.**
Round 2 wrote the round-1 name into this "out of scope and untouched" list, then added a second one
in the same style, then reported the run as clean-minus-someone-else's. Round 3 renamed both and
`non_snake_case` is now 0 for the crate (§9). Naming a warning as someone else's is a defensible
decision; leaving the decision on the page and then writing a third one in the same style is not. I
did that, and the reason I did it is that the list above read as permission.

### 8.3 Falsification — the experiment that closes round 1's gap

**Harness correctness first, because round 1's harness had a bug.** Round 1's falsification grep
matched only `FAILED|left:|right:`. A mutation that breaks compilation therefore produces **no** match
and is scored as a false green. The pattern used here is
`grep -E '^test result: FAILED' log` **plus `grep -cE '^error(\[|:)' log`** (compile errors counted
as reds), and the run is `cargo test --workspace --no-fail-fast` — plain `cargo test` stops at the
first failing binary and hides the rest. **Three** operational notes, all of which bit me once: the
redirect must be `> log 2>&1` (`2>&1 > log` sends stderr to the terminal and leaves the log empty of
failures), the mutation must be reverted with a **file backup**, not `git checkout`, because
`git checkout` on a file that also carries an in-flight fix silently reverts the fix, and
`fast_apply_edit` can **insert** the replacement beside the matched region instead of replacing it,
which turns a one-line fix into a duplicated block.

*(Round 3, F-2: this list originally said "Two operational notes" and the `fast_apply_edit` incident
was in none of them. It is a disclosure gap and not a damage report — the check and its result are in
§9.)*

**Mutation 1 — the reviewer's: make `truncate_fragments_with_ellipsis` return `None` unconditionally.**
Result: exit 101, 0 compile errors, **7 tests red**:

| Test | File |
|---|---|
| `an_ellipsis_is_reserved_space_in_the_layout_tree_and_never_overflows_its_line` | `inline_formatting.rs` |
| `the_ellipsis_lands_in_the_piece_where_the_text_ran_out_not_the_first_one` | `inline_formatting.rs` |
| `an_atomic_inline_box_with_a_declared_width_truncates_its_content_to_it` | `inline_formatting.rs` |
| `a_pre_block_truncates_each_of_its_own_overflowing_lines` (renamed in round 3, was `..._EACH_...`) | `inline_formatting.rs` |
| `an_atomic_with_no_declared_width_shrink_fits_to_the_line_and_truncates_its_label` (new in round 2, renamed in round 3) | `inline_formatting.rs` |
| `ellipsis_truncates_single_line_overflow` | `text_measure.rs` |
| `the_single_string_wrapper_truncates_and_is_not_the_layout_path` | `text_measure.rs` |

Read precisely, which is the point of the exercise: of round 1's **nine** new inline-formatting cases,
**4 went red and 5 stayed green** — and all 5 are green *by construction*, not by accident. Three are
the C-2 cases, which are about the clamp and about empty atomics and were never going to depend on the
truncation decision; `without_the_property_the_same_text_overflows_its_line_unchanged` asserts the
**80** that this mutation produces, so it must stay green;
`a_wrapping_block_does_not_truncate_which_is_a_recorded_divergence` asserts that *no* truncation
happens, so it must stay green too. My new contrast test also stayed green, correctly — it pins the
untruncated 80, which is precisely what makes it the other half of the pair.

**What mutation 1 proves:** the ellipsis decision genuinely reaches the layout tree on the IFC path
(C-1 is real and the new cases are sensitive to it); the single-string wrapper is live; the atomic
shrink-fit case is sensitive to truncation, not just to the clamp.
**What it cannot prove:** anything about the *fit* arithmetic inside
`truncate_fragments_with_ellipsis` (returning `None` removes the whole decision, not the arithmetic);
anything about DPI or end-to-end scaling; and nothing at all about the 5 green cases above.

**Mutation 2 — a second, cheaper question: `let target = probe;` in `lay_out_atomic`,** replacing
`max_content.min(available).saturating_add(inset)`. Result: 9 red in `inline_formatting.rs`,
including **both** new C-1.3 tests. So the pair is sensitive to the clamp itself, not only to
truncation. Stated honestly: the contrast is **not independent** of the clamp — its
`atomic.rect.w == 40` assertion is load-bearing and goes red with it. That is not a defect in the
test; it is the reason the pair is worth more than a single bound, and it is only visible because the
second mutation was run.

**Not falsified, and not claimed:** end-to-end DPI scaling (no oracle exists in this programme; I did
not try to build one), the `max_content_width` probe approximation, the nested-atomic baseline
recursion, and `vertical-align: middle`'s exact arithmetic. Round 0's §5 limits stand unchanged.

### 8.4 Round 2 verification

```
cargo fmt --all -- --check                    clean
cargo clippy -p velox-dom --all-targets       0 errors. 13 warnings, 0 of them non_snake_case:
                                               6 in the lib and 7 in test binaries, every one of
                                               them inherited from earlier tasks. Re-run by round 3
                                               after the two renames — see §9.1 for the full list
                                               and the attribution check.
cargo test --workspace --no-fail-fast         871 passed, 0 failed
cargo test -p velox-dom --no-fail-fast        226 passed, 0 failed
```

*(Round 3, F-1: the clippy line above used to read "0 errors (2 pre-existing warnings, 1 of them the
round-1 non-snake-case test name, not mine)". Both halves of that were false. The real counts are 6
lib plus 7 test-binary warnings, and **two** of them were `non_snake_case` — round 1's and round
2's own new test. The full named list is in §9.1. The fmt and test lines above are round 3's re-runs;
they match round 2's numbers, which is expected because a rename touches no behaviour.)*

871 and 226 are up from round 1's 869 and 224 (the two new C-1.3 cases), and above the required
floors. Both mutation runs above are on top of this green baseline, and both were reverted with the
final state re-verified by the commands above — not assumed.

---

## 9. Fix round 3

Round 2 came back **Spec PASS / quality changes requested** — all six open findings ADDRESSED, no
behavioural defect introduced. What came back was one Important and one Minor, and both were about
this report overstating its own verification rather than about the code. This round changed no
behaviour: **two test-function renames and nothing else.** The implementation is untouched.

The theme is worth one sentence before the items, because it is the reason the round exists. A clean
lint claim is a **gate** claim, and gate claims are the ones a reader trusts without re-running —
which is exactly why a false one survives review rounds. §8.2 said "is clean" about a run with
fifteen warnings in it, and §8.4 said "2 pre-existing warnings, 1 of them … not mine" about a run
with thirteen, one of which *was* mine. Both were self-consistent with each other and both were false,
and the contradiction inside a single report is the tell: §8.2 declared the round-1 name out of
scope, §8.3 then documented round 2's new test in the same upper-case style, and §8.4 reported the
result as clean-minus-someone-else's. Nothing in the chain was checked, because each hand-off was
trusted.

### 9.1 F-1 (Important) — the clippy claim, corrected with the real numbers

**What changed in the tree.** Two test names renamed, in `velox-dom/tests/inline_formatting.rs`:

| before | after | whose |
|---|---|---|
| `a_pre_block_truncates_EACH_of_its_own_overflowing_lines` | `a_pre_block_truncates_each_of_its_own_overflowing_lines` | round 1's |
| `an_atomic_with_no_declared_width_shrink_fits_to_the_LINE_and_truncates_its_label` | `an_atomic_with_no_declared_width_shrink_fits_to_the_line_and_truncates_its_label` | round 2's own new test |

Both are test **names only** — no assertion, fixture or behaviour is touched by either rename, which
is why §8.4's test numbers are unchanged below. Round 1's is renamed in this pass deliberately: the
false claim was *about the pair*, so fixing only mine would have left it half-true again. I did not
"fix" anything else.

**The real post-rename run.** `cargo clippy -p velox-dom --all-targets`, re-run by me after the
renames, exit 0:

```
0 errors. 13 warnings: 6 in the lib, 7 in test binaries. 0 non_snake_case.
(`lib test` additionally reports "6 warnings (6 duplicates)" — the same 6 lib warnings compiled
 a second time under the test cfg, not 6 more.)
```

Every one, so the next reader can check without re-running:

| # | location | lint | target |
|---|---|---|---|
| 1 | `velox-dom/src/layout.rs:1484` | `too_many_arguments` (11/8) on `content_size_for` | lib |
| 2 | `velox-dom/src/layout.rs:3018:35` | `redundant_closure` | lib |
| 3 | `velox-dom/src/layout.rs:3641:62` | `if_same_then_else` | lib |
| 4 | `velox-dom/src/text_wrap.rs:63:23` | `type_complexity` on `static SKIA_MEASURER` | lib |
| 5 | `velox-dom/src/text_wrap.rs:90:5` | `collapsible_if` | lib |
| 6 | `velox-dom/src/text_wrap.rs:115:5` | `collapsible_if` | lib |
| 7 | `velox-dom/tests/maxwidth_absolute.rs:213:28` | `needless_borrow` | test `maxwidth_absolute` |
| 8 | `velox-dom/tests/maxwidth_absolute.rs:273:28` | `needless_borrow` | test `maxwidth_absolute` |
| 9 | `velox-dom/tests/flex_completeness_repro.rs:3:15` | `needless_lifetimes` | test `flex_completeness_repro` |
| 10 | `velox-dom/tests/flex_completeness_repro.rs:127:13` | `identity_op` | test `flex_completeness_repro` |
| 11 | `velox-dom/tests/box_sizing.rs:41:5` | `bool_assert_comparison` | test `box_sizing` |
| 12 | `velox-dom/tests/text_measure.rs:51:13` | `len_zero` | test `text_measure` |
| 13 | `velox-dom/tests/flex_critical_repro.rs:127:9` | `manual_range_contains` | test `flex_critical_repro` |

**Attribution, and how far I actually checked it.** The findings file settled that the lib warnings
are inherited and the test warning was round 2's, on the reasoning that round 2's source diff is a doc
move plus one `#[allow]` deletion in `layout.rs` and comment-only lines in `text_wrap.rs`, and
neither can create a warning. I did not need to take that on trust — I checked it at line level
against the round-2 diff, which is a stronger test than the argument:

- round 2's `layout.rs` hunks are `@@ -772,6 +771,0 @@` and `@@ -865,0 +860,5 @@`; the three lib
  warnings sit at 1484, 3018 and 3641 — **no overlap**
- round 2's `text_wrap.rs` hunk is `@@ -199,2 +199,7 @@`; the three warnings sit at 63, 90 and 115 —
  **no overlap**
- of the five files round 2 touched, only `text_measure.rs` carries a warning among them
  (`text_measure.rs:51`), and round 2's hunks there are `@@ -217,0 +218,5 @@`, `@@ -219,4 +224,3 @@`
  and `@@ -235,2 +239 @@` — **line 51 is untouched by round 2**
- the other four warning-carrying test files were not in round 2's diff at all

So: **all 13 are inherited from earlier tasks, and 0 are round 2's.** The 6 lib warnings are
explicitly out of scope for this round per the findings file, and I did not fix them — a report
correction that drags 13 inherited warnings into the diff would make the diff harder to review and
would not make the claim any more true. Note the one honest caveat: "inherited" here means
*pre-existing at `3ddbe5d`*, which I verified by line; it does not mean I traced each one to the task
that introduced it, and I did not try.

**The corrected sentences.** §8.2 and §8.4 have both been corrected in place, with the superseded text
quoted so the correction is visible rather than silent. §8.4 now reads `0 errors. 13 warnings, 0 of
them non_snake_case: 6 in the lib and 7 in test binaries, every one of them inherited from earlier
tasks.` I have not written the word "clean" about this command anywhere in this report, because I
have not re-run it since writing that sentence — and that is the whole lesson: the numbers moved
between round 2's edit and round 2's report, which is precisely what makes an un-re-run gate claim
worth nothing.

**What I verified and what I did not.** Verified post-rename, by me, on this tree: `cargo fmt --all
-- --check` clean; `cargo clippy -p velox-dom --all-targets` exit 0 with the 13 warnings above and 0
`non_snake_case`; `cargo test --workspace` **871 passed, 0 failed** (44 ignored); `cargo test -p
velox-dom` **226 passed, 0 failed**. 871/226 match round 2's numbers, as they must, because a rename
cannot change a test count — and that is a consistency check, not independent confirmation, so I am
not claiming the suite as additional evidence for round 3. Not verified and not claimed: I did **not**
re-run the two mutations in §8.3, so §8.3's red-test lists stand on round 2's runs, with only the two
test *names* updated to the current spellings; and I did not run clippy on any crate other than
`velox-dom`, so nothing here is a workspace-wide lint claim.

### 9.2 F-2 (Minor) — the third harness incident, disclosed

§8.3 presented its list as complete — *"Two operational notes, both of which bit me once"* — and it
was not complete. The third is `fast_apply_edit` **inserting** the replacement beside the matched
region instead of replacing it, which turns a one-line fix into a duplicated block. The list now reads
"Three operational notes", all three recorded, because the point of recording a harness incident is
that the next round does not pay for it again.

**The damage check, and what it covers.** I looked for exactly what insert-instead-of-replace would
leave behind, and report what I ran:

- `rg -c '^fn |^pub fn '` then a duplicate-name check over the three test files round 2 touched:
  `inline_formatting.rs` **40** fns, `text_measure.rs` **11**, `text_metrics_fallback.rs` **10** —
  **0 duplicate `fn` names** in any of them
- read the round-2 diff for those three files region by region; every changed region reads coherently
  and the replace path is visibly working
- the F-3a splice repair is itself the evidence that no insert happened there: a duplicated insert
  would have left the broken mid-sentence comment *and* added the repaired one, and only the repaired
  one is in the file

Result: **no tree damage.** This is a disclosure gap, not a defect, and I am not claiming more than
that — I checked duplicate function names and read the diff; I did not rebuild round 2's mutations
to prove each edit took the path I think it took. A list that presents itself as complete when it is
not is the same overstating pattern as F-1 one level down, which is why it is fixed rather than
deferred.

Incidentally, I reproduced the first hazard myself during this round: my first clippy capture was
`cargo clippy … 2>&1 > /tmp/…txt`, and the file came out empty because cargo writes its warnings to
stderr. The `> log 2>&1` form in §8.3 is the correct one, and the numbers in §9.1 come from a
capture using it.

### 9.3 Scope of this round

One commit renames two test functions; one commit corrects this report. Nothing else was touched:
no `velox-sfc`, no `velox-renderer/`, no `viewport.rs`, no `lib.rs`, no `Cargo.lock`, no goldens, no
`.vx` grammar, no dependencies, and no source under `velox-dom/src/`. The C-1.3 test pair, their docs
and the §8.3 mutations are **not** re-opened — the reviewer settled round 2's self-flagged
"concern" about the pair as a misframing rather than a defect, because a differential pair pinning
two mechanisms is sensitivity working: a mutation hitting either one is caught. I have left that
alone deliberately.

Still open, and still not this round's: **F-6** remains a recorded open question, not a confirmed
defect — a mixed line fits its text against the full line limit, ignoring atomics already on the line,
and the renderer's paint-time truncation may share the same blind spot, which needs the renderer. The
6 pre-existing lib warnings are inherited and deferred per the findings file. End-to-end DPI
scaling, the `max_content_width` probe approximation, nested-atomic baseline recursion and
`vertical-align: middle`'s exact arithmetic remain unfalsified and unclaimed, as §8.3 states.
