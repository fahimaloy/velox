# Task 16 — Whitespace-only text nodes and inline UA defaults

Branch `fix/2A-flex-complete`, base/HEAD at start `48e25b2`.
Files owned and changed: `velox-dom/src/layout.rs`, `velox-dom/tests/layout_tests.rs`,
`velox-style/src/ua.css`. Nothing else was modified.

## 0. Headline finding you need to know before reading the rest

**Part B (the block-boundary guard) already exists on this branch.** Commit `f1cb6bc`
("fix(layout): drop collapsible whitespace boxes in block flow") is an ancestor of the
base commit `48e25b2` and already added, to `velox-dom/src/layout.rs`:

- `is_inline_formatting_participant`, `is_formatting_participant`,
  `should_drop_collapsible_whitespace`,
- the call site in the block child loop (`velox-dom/src/layout.rs:2516` at `f1cb6bc`; `2542` now),
- and two tests in `velox-dom/tests/layout_tests.rs` (now lines 41-76).

So the brief's premise ("the block loop has no guard today") is **false at HEAD**, and the
literal instruction "capture the pre-fix failure output for both new tests" could not be
satisfied against HEAD. I therefore captured the genuine pre-fix output by reverting
`velox-dom/src/layout.rs` to the state before that commit:

```
git checkout f1cb6bc^ -- velox-dom/src/layout.rs     # no block-loop guard
```

Two consequences I want to be explicit about:

1. **The pre-fix run shows only ONE of the two new tests failing**, not both. The
   negative control passes pre-fix (see §2) — it cannot fail there, because pre-fix
   behaviour was "always keep the whitespace", which is exactly what the negative control
   demands. The negative control's job is to discriminate against a *blanket skip*
   (§5), not against the pre-fix state.
2. **My Part A (UA `display: inline` defaults) is therefore not what makes the
   phantom-box fix work** — `f1cb6bc`'s hardcoded tag list already did that. What Part A
   does is close the remaining parity gap (the UA sheet is now honest, and the layout
   engine's notion of a tag's default `display` is derived from the same documented
   table instead of a second, hand-maintained copy of it). The brief's ruling 2
   ("A and B must ship together") is satisfied: they ship in one commit.

## 1. What changed

### `velox-style/src/ua.css` (Part A)

```css
a, abbr, b, bdi, bdo, cite, code, data, del, dfn, em, i, ins, kbd, label,
mark, q, s, samp, small, span, strong, sub, sup, time, u, var { display: inline; }
```

### `velox-dom/src/layout.rs` (Part B support)

- New `INLINE_BY_DEFAULT_TAGS` (the 28 tags above) and `INLINE_BLOCK_BY_DEFAULT_TAGS`
  (`button`, `img`, `input`, `select`, `textarea`), both documented.
- New `default_display_for_tag(tag) -> &'static str` ("inline" / "block") and
  `is_inline_level_by_default(tag) -> bool` (the inline set ∪ the inline-block set).
- `is_inline_formatting_participant` now calls `is_inline_level_by_default(tag)` instead
  of carrying its own inline `matches!` tag list. Same answers, one source of truth.
- `compute_layout`'s own `display` default changed from the literal `"block"` to
  `default_display_for_tag(tag)` (`velox-dom/src/layout.rs:1496-1498`), so the box-model
  path and the whitespace classifier cannot drift apart.

The second change is provably behaviour-neutral: that local is only ever compared against
`"none"` and `"flex"` (layout.rs:1518 and :1543), and `default_display_for_tag` never
returns either. I verified it empirically too (§7, "no shifted test").

## 2. Pre-fix failure output (both new tests)

Command: `cargo test -p velox-dom --test layout_tests` with
`velox-dom/src/layout.rs` reverted to `f1cb6bc^` and my three new tests in place.

```
test block_flow_ignores_whitespace_only_text_between_blocks ... FAILED
test block_boundary_collapses_whitespace_only_text ... FAILED

---- block_flow_ignores_whitespace_only_text_between_blocks stdout ----
thread 'block_flow_ignores_whitespace_only_text_between_blocks' panicked at
  velox-dom/tests/layout_tests.rs:96:5:
assertion `left == right` failed: whitespace between two blocks must not create a third
  box, got [Rect { x: 0, y: 0, w: 800, h: 20 },
            Rect { x: 0, y: 20, w: 0, h: 19 },
            Rect { x: 0, y: 39, w: 800, h: 20 }]
  left: 3
 right: 2

---- block_boundary_collapses_whitespace_only_text stdout ----
thread 'block_boundary_collapses_whitespace_only_text' panicked at
  velox-dom/tests/layout_tests.rs:55:5:
assertion `left == right` failed
  left: 3
 right: 2

failures:
    block_boundary_collapses_whitespace_only_text
    block_flow_ignores_whitespace_only_text_between_blocks

test result: FAILED. 26 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
```

That is the reported defect exactly: a `w: 0, h: 19` phantom line box at `y: 20`, and the
following block pushed from `y: 20` to `y: 39`. A throwaway probe (since deleted) also
confirmed the phantom box is inside the viewport-filling root: root `Rect { x: 0, y: 0,
w: 800, h: 600 }`, children `20 / 19(+y20) / 20(+y39)`.

`inline_siblings_keep_their_collapsing_whitespace` **passed** in this pre-fix run
(26 passed includes it), as explained in §0.

`block_flow_keeps_preserved_whitespace_between_blocks` (my third test, `white-space: pre`)
also passed pre-fix and still passes now — pre-fix kept everything, so it is a
regression guard, not a pre-fix failure. Verified as still failing under a blanket skip
in §5.

## 3. Elements given `display: inline`, and the browser default each was verified against

Verification method: I fetched the WHATWG HTML Living Standard "Rendering" section
(`https://html.spec.whatwg.org/multipage/rendering.html`, page header "Last Updated
25 September 2026") and read the UA rules it declares.

| tag | browser default | evidence |
|---|---|---|
| a, abbr, b, bdi, bdo, cite, code, data, del, dfn, em, i, ins, kbd, label, mark, q, s, samp, small, span, strong, sub, sup, time, u, var | `inline` | The rendering section (§15.3.4 Phrasing content, §15.3.5 Bidirectional text) gives none of them a `display` declaration, so they take CSS Display's initial value, `inline`. It sets only font/underline/colour on them (`cite, dfn, em, i, var { font-style: italic }`, `b, strong { font-weight: bolder }`, `mark { background: yellow }`, `abbr[title], acronym[title] { text-decoration: dotted underline }`, …). |

Elements deliberately **not** given a default, with the spec rule that excludes them:

| tag(s) | browser default | evidence |
|---|---|---|
| div, address, blockquote, center, dialog, figure, figcaption, footer, form, header, hr, legend, listing, main, p, plaintext, pre, search, xmp | `block` | `address, blockquote, center, dialog, div, figure, figcaption, footer, form, header, hr, legend, listing, main, p, plaintext, pre, search, xmp { display: block; }` |
| article, aside, h1-h6, hgroup, nav, section | `block` | `article, aside, :heading, hgroup, nav, section { display: block; }` |
| dir, dd, dl, dt, menu, ol, ul | `block` | `dir, dd, dl, dt, menu, ol, ul { display: block; }` |
| li | `list-item` | `li { display: list-item; … }` |
| fieldset | `block` | `fieldset { display: block; … }` |
| table, caption, colgroup, col, thead, tbody, tfoot, tr, td, th | `table` / `table-caption` / `table-column-group` / `table-column` / `table-header-group` / `table-row-group` / `table-row` / `table-cell` | §15.3.8 |
| ruby, rt | `ruby`, `ruby-text` | `ruby { display: ruby; } rt { display: ruby-text; }` |
| br, wbr | not `inline` (they generate a newline / break opportunity) | `br { display-outside: newline; }`, `wbr { display-outside: break-opportunity; }` |
| button, input | `inline-block` | `input, button { display: inline-block; }` — **quoted verbatim from §15.3.10**, so these must not be given `display: inline` |
| img, select, textarea | `inline-block` | Browser UA sheets (no display declaration in the spec's non-widget blocks) |

Per the brief's ruling 3, `inline-block` is left unimplemented and **no `inline-block`
element gets a UA default** — claiming `display: inline` for `button` would be a false
parity claim. They are still treated as *inline-level* for the whitespace decision
(§4), which is a level question, not a `display`-value question, and is true of them in
browsers.

Obsolete elements (`big`, `tt`, `font`, `nobr`, `output`) were left out even though the
spec also leaves them at the initial `inline`, to keep every added tag individually
checkable. `output`, `progress` and `meter` are the notable gaps if someone needs them.

## 4. How block-level vs inline-level was decided in the block loop

The decision lives in `should_drop_collapsible_whitespace` (called from the block child
loop at `velox-dom/src/layout.rs:2542`). A whitespace-only `VNode::Text` is skipped only
when **all** of these hold:

1. **It is collapsible.** The parent's `white-space` is `normal` (the default) or
   `nowrap`. Anything else — `pre`, `pre-wrap`, `pre-line`, `break-spaces` — is
   significant and is left completely alone.
2. **No line box can form on either side.** For each direction, walk outward from the
   whitespace node to the nearest *formatting participant* and ask whether it is
   *inline-level*:
   - `is_formatting_participant` skips over other whitespace-only text nodes and rejects
     out-of-flow boxes (`display: none`, `position: absolute|fixed`) — a neighbour that
     generates no box cannot share a line with the whitespace.
   - `is_inline_formatting_participant` then decides the level: an explicit
     `display: inline | inline-block | inline-flex | inline-grid` is inline-level; any
     other explicit `display` is block-level; **with no explicit `display`, the element's
     default comes from `is_inline_level_by_default(tag)`** (the `inline` table ∪ the
     `inline-block` table).
   - The node is dropped when *either* side has no inline-level neighbour
     (`!has_inline_before || !has_inline_after`). That is the browser rule: collapsed
     whitespace at a block boundary produces no line box, while whitespace between two
     inline-level boxes collapses to a single space that must be rendered.

So the question asked is "is this neighbour inline-level?" — the same question the brief
asks — and the answer comes from one documented table shared with the UA sheet. The
flex child loop keeps its own simpler whitespace skip (a flex item is a block-level box
by construction, so every whitespace-only child of a flex container is collapsible away);
I mirrored that style rather than changing the rule.

## 5. Proof that the negative control genuinely fails under a blanket skip

**How I checked, not an assertion:** I temporarily patched
`should_drop_collapsible_whitespace` in `velox-dom/src/layout.rs` so that it returned
`true` for *any* whitespace-only `Text` node, ignoring both the `white-space` check and the
neighbour check — i.e. exactly the wrong fix the brief warns about — then ran the suite
and captured the output, then restored the real file (verified: the file no longer
contains the patch, and the suite is green again — §7).

```
---- inline_siblings_keep_their_collapsing_whitespace stdout ----
thread 'inline_siblings_keep_their_collapsing_whitespace' panicked at
  velox-dom/tests/layout_tests.rs:129:5:
assertion `left == right` failed: whitespace between inline siblings must collapse, not
  disappear; got [Rect { x: 0, y: 0, w: 800, h: 38 }, Rect { x: 0, y: 38, w: 800, h: 38 }]
  left: 2
 right: 3

---- block_flow_keeps_preserved_whitespace_between_blocks stdout ----
thread 'block_flow_keeps_preserved_whitespace_between_blocks' panicked at
  velox-dom/tests/layout_tests.rs:160:5:
assertion `left == right` failed: white-space: pre whitespace is significant and must be
  preserved, got [Rect { x: 0, y: 0, w: 800, h: 20 }, Rect { x: 0, y: 20, w: 800, h: 20 }]
  left: 2
 right: 3

---- block_boundary_preserves_whitespace_between_inline_participants stdout ----
thread 'block_boundary_preserves_whitespace_between_inline_participants' panicked at
  velox-dom/tests/layout_tests.rs:74:5:
assertion `left == right` failed
  left: 2
 right: 3

test result: FAILED. 25 passed; 3 failed
```

The negative control fails under a blanket skip, in both my new test and the pre-existing
one from `f1cb6bc`, and the primary block-boundary test still passes under the blanket
skip — i.e. the two tests pull in opposite directions and only the selective rule
satisfies both. The third failure shows the `white-space: pre` guard is load-bearing too.

Honest scope note: velox-dom has no renderer dependency, so this is proven at the
**layout-tree** level — the whitespace survives as a real line box (3 children, the middle
one carrying `source_index == Some(1)` and a non-zero line-box height) instead of
vanishing. It is *not* a pixel-level "there is a visible gap between the a and the b"
assertion, because Velox has no inline layout yet: today the two spans and the space box
stack vertically (y = 0, 38, 57 in the cascade probe). The horizontal advance is out of
scope and is called out as a concern in §9.

## 6. Confirmation that `white-space: pre` is preserved

- New test `block_flow_keeps_preserved_whitespace_between_blocks` (layout_tests.rs:150):
  a `white-space: pre` parent containing `div(20px) / Text(" ") / div(20px)` asserts 3
  layout children with the whitespace node at `source_index == Some(1)`. It passes now and
  fails under a blanket skip (§5), so it is a real guard.
- The load-bearing piece is untouched: `velox-dom/src/text_wrap.rs`'s `WhiteSpace::Pre`
  branch splits on `'\n'` and emits each paragraph **as-is** (a `" "` paragraph becomes one
  line with a real measured width), which is why `white-space: pre` whitespace still
  produces a box. `wrap_text_with_options`' `measured.is_empty()` fallback that
  manufactures the 19px empty line is only reached on the `Normal`/`PreLine` path, and I
  did not modify that file.
- `white-space: nowrap` is also treated as collapsible by the guard (matches browsers: a
  nowrap space between two inline-level boxes is still collapsed, and between two
  block-level boxes still produces no line box).

## 7. Real verification output

`cargo fmt --all --check` → exit 0, no output.
`cargo build --workspace` → `Finished dev profile … in 7.66s`; the only warning in the
workspace is the pre-existing `method 'is_degraded' is never used` in
`velox-renderer/src/presenter.rs:286`. No new warnings.

`cargo test -p velox-dom` (14 test binaries, all green):

```
unittests src/lib.rs        ok. 21 passed; 0 failed
tests/box_sizing.rs         ok.  9 passed; 0 failed
tests/box_sizing_repro.rs   ok.  6 passed; 0 failed
tests/diff_edge_tests.rs    ok. 30 passed; 0 failed
tests/diff_tests.rs         ok.  5 passed; 0 failed
tests/flex_completeness_repro.rs  ok. 11 passed; 0 failed
tests/flex_critical_repro.rs      ok.  5 passed; 0 failed
tests/layout_golden.rs      ok.  5 passed; 0 failed
tests/layout_tests.rs       ok. 28 passed; 0 failed
tests/margin_collapse.rs    ok.  5 passed; 0 failed
tests/overflow_scroll.rs    ok. 11 passed; 0 failed
tests/template_w_h_layout_repro.rs  ok. 1 passed; 0 failed
tests/text_measure.rs       ok.  9 passed; 0 failed
```

New tests, run by name with `-v`:

```
test block_flow_ignores_whitespace_only_text_between_blocks ... ok
test inline_siblings_keep_their_collapsing_whitespace ... ok
test block_flow_keeps_preserved_whitespace_between_blocks ... ok
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo test -p velox-style` (9 binaries: unittests 7, cascade 7, computed_properties 5,
hover 1, scoped_repro 3, selector_combinators 10, style_apply 4, style_parse 1) → all ok,
0 failed. `cargo test -p velox-sfc` → 23 test binaries, all `ok` (0 failed; two binaries
have pre-existing `#[ignore]`s: "0 passed; 2 ignored" and "0 passed; 1 ignored").

`cargo test -p velox-renderer` → 41 binaries, all ok. `cargo test -p velox-cli` → 5
binaries, all ok. `cargo test -p velox-core` → all 10 *tracked* test binaries ok
(computed_tests 4, lifecycle_repro 3, lifecycle_tests 1, next_tick_tests 8,
provide_inject_tests 10, reactive_primitives_demo 2, ref_cell_tests 3, resize_hooks 3,
signal_edge_tests 16, signal_tests 1).

The three example proof suites (the real regression gate), via
`cargo test -p velox-example-todo -p velox-example-counter -p velox-example-showcase`:

```
counter   tests/render_proof.rs:
  test renders_large_viewport_proof_png ... ok
  test small_viewport_keeps_count_status_and_all_buttons_visible ... ok
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

showcase  tests/render_proof.rs:
  test small_viewport_renders_visible_gallery_sections ... ok
  test renders_large_viewport_proof_png ... ok
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

todo      tests/render_proof.rs:
  test every_todo_row_carries_its_key ... ok
  test small_viewport_renders_the_list_and_reacts_to_events ... ok
  test renders_large_viewport_proof_png ... ok
  test typed_draft_reaches_the_input_before_add ... ok
  test events_drive_the_visible_list ... ok
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

All three use the layout-backed APIs (`render_vnode_to_rgba` /
`render_vnode_to_raster_png_with_scale`); `render_vnode_to_raster_png` is not used
anywhere in the repo.

Part A verified end-to-end through the real cascade with a throwaway probe (since
deleted) that ran `apply_with_cascade` and then `compute_layout`:

```
PROBE span0 props.attrs = {"style": "box-sizing: border-box; display: inline;"}
PROBE div  props.attrs = {"style": "box-sizing: border-box;"}
PROBE span("a") / Text(" ") / span("b") layout children =
  [Rect { x: 0, y: 0, w: 800, h: 38 }, Rect { x: 0, y: 38, w: 0, h: 19 }, Rect { x: 0, y: 57, w: 800, h: 38 }]
  count=3      (whitespace preserved)
PROBE div(20px) / Text(" ") / div(20px) layout children =
  [Rect { x: 0, y: 0, w: 800, h: 20 }, Rect { x: 0, y: 20, w: 800, h: 20 }]
  count=2      (no phantom box, with the real cascade + UA sheet)
```

That probe also confirmed the `/* … */` comments I added to `ua.css` parse correctly
(`cssparser`'s `RuleListParser` skips them) and that the UA rule reaches layout through
the cascade. I also re-ran the span-height probe against `HEAD`'s `layout.rs` and got
byte-identical rects, confirming Part A and the refactor shift nothing in the span path.

## 8. Existing tests that shifted

**None.** Every pre-existing test in the workspace passes unchanged. In particular:
`layout_tests.rs` went 25 → 28 (my three additions, no edits to existing tests),
`layout_golden.rs` (5 golden layout snapshots) is unchanged, `flex_completeness_repro.rs`
(11), `flex_critical_repro.rs` (5), `margin_collapse.rs` (5), `box_sizing*.rs` (15),
`diff_*.rs` (35) and `overflow_scroll.rs` (11) are all unchanged, as are the three example
proof suites. Per the instruction, no test was adjusted to make anything pass.

One pre-existing failure, **not caused by this task**: `cargo test --workspace` /
`cargo build --workspace --tests` fails to compile the test target
`velox-core/tests/batch_redraw.rs`:

```
error[E0603]: function `flush_queue` is private
  --> velox-core/tests/batch_redraw.rs:5:34
```

That file is **untracked** (`git status` → `?? velox-core/tests/batch_redraw.rs`) and
belongs to the stashed WIP in `stash@{0}` ("Task 14B signal batching — WIP, BLOCKED by
todo proof"), whose `velox-core/src/signal.rs` change is not on this branch. It is
outside my ownership, so I left it alone and instead ran every *tracked* velox-core test
target individually (all green, §7). It will keep `cargo test --workspace` red until that
stash is resolved — flagging it so it is not mistaken for fallout from this commit.

## 9. Concerns

1. **The brief's Part-B premise was stale.** The block-loop whitespace guard and two
   tests already existed (`f1cb6bc`). My `layout.rs` change is therefore a
   single-source-of-truth refactor plus the `display` default, not the introduction of
   the rule. If the controller expected a novel Part B diff, this is the discrepancy to
   adjudicate.
2. **Part A is not covered by any committed test.** The natural home is
   `velox-style/tests/cascade.rs` (which already asserts the UA sheet contains
   `body { margin: 8px }` and `h1 { margin: 0.67em }`), but that file is outside my
   ownership and outside the commit's file list, so the new UA rule is only verified by
   the throwaway probe in §7. Worth a follow-up task.
3. **The examples do not exercise the new UA defaults.** `grep -ohE "<[a-z]+"` over
   `examples/*/src/App.vx` finds only `div`, `h*`, `section`, `p`, `button`, plus a
   `Ref<i32>` type in the script block — **no phrasing element at all**. So the three
   proof suites prove "no regression" but give Part A zero coverage. A new example (or a
   `render_proof` assertion on a `span`/`label`) would close that.
4. **The negative control proves line-box survival, not a visible horizontal gap.** With
   no inline layout, "a b" and "ab" differ in Velox only by the presence of a line box.
   The pixel-level claim the brief describes is not achievable in a velox-dom test and
   should be re-checked when the inline formatting context lands; that task will have to
   revisit `inline_siblings_keep_their_collapsing_whitespace`, whose current assertions
   (3 children, `source_index`, non-zero height) will still hold but will no longer be the
   whole story.
5. **The two lists must be kept in sync by hand** (`INLINE_BY_DEFAULT_TAGS` in
   velox-dom vs the rule in `ua.css`), because velox-dom cannot depend on velox-style.
   Both sites now say so in comments. A future test that parses `ua.css` and compares it
   against the Rust table would make this mechanical.
6. **Duplicated tests.** `block_flow_ignores_whitespace_only_text_between_blocks` /
   `inline_siblings_keep_their_collapsing_whitespace` largely overlap the pre-existing
   `block_boundary_collapses_whitespace_only_text` /
   `block_boundary_preserves_whitespace_between_inline_participants`. I added the
   brief-mandated names rather than deleting the older ones (the brief explicitly warns
   against omitting the negative control, and deleting a passing test is not mine to do),
   but the pair is redundant and could be collapsed in a cleanup pass.
7. **Minor pre-existing inconsistency left alone:** `is_inline_formatting_participant`
   recognises the strings `inline-flex`/`inline-grid`, but
   `velox-dom/src/style.rs`'s `Display::parse` does not parse those keywords (it knows
   `block|inline|inline-block|flex|grid|none|hidden`). An element styled
   `display: inline-flex` is therefore classified inline-level here and not-flex in the
   layout router. Out of scope; not touched.
8. **Uncovered tag gaps:** `output`, `progress`, `meter`, `br`, `wbr`, `svg` are not in
   either table. `output` is genuinely `inline` in browsers and would be a reasonable
   addition; I left it out to keep every added tag individually verifiable against the
   spec text I read.
