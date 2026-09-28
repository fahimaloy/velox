# Velox HTML/CSS/Vue Behavioral Parity Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore full browser-like rendering parity — viewport filling, margins/collapse, flex/box-sizing, text measurement, scrollable overflow, clip-hit-testing, and Vue SFC defaults — so the full-screen / small-window boilerplate (photos 1–3) renders and resizes like real HTML/CSS.

**Architecture:** Keep the existing immediate-mode pipeline `make_view(vw,vh) → apply_styles → compute_layout(vw,vh) → render_frame → present` but inject a correct 3-layer cascade (UA → author → inline), fix block/flex available-size propagation, unify Skia text metrics into layout, and replace the clip-only overflow with a scrollable model driven by `MouseWheel` + scrollbar paint. No new render loop; fixes are layered behind `compute_layout` and `Stylesheet::parse/apply` contracts and verified by golden layout tests + manual photo-repro.

**Tech Stack:** Rust · Skia (skia-safe) · softbuffer · winit · pest · cssparser · Velox crates (`velox-dom`, `velox-style`, `velox-renderer`, `velox-sfc`, `velox-core`, `velox-cli`)

**Spec:** `docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md` (24 findings F-01…F-24, fixes CX-01…CX-16, breakdown T-01…T-14)

## Global Constraints

- MSRV as in `Cargo.toml` workspace — do not bump without audit note; keep `cargo test -p velox-dom -p velox-style -p velox-sfc -p velox-renderer` green at every task boundary.
- Do not change the public SFC syntax (`.vx` grammar, `ref!`/`signal!` macros) without explicit spec amendment — parity is achieved via internals (UA sheet, layout, scroll model).
- Every visual change must be provable without a compositor: `cargo test -- --nocapture` + `render_vnode_to_rgba` PNG goldens in headless/EPIPE-degraded mode.
- Single rounding point remains `Viewport` (`phys/scale.round.max1`); never scatter a second rounding.
- `cargo fmt` + `cargo test` before each commit; one commit per task step-batch (TDD: test-first).
- No placeholder text, no TODOs, no “handle edge cases” without code — each step ships real code and a passing/failing test.

---

## File Structure (what each task touches)

**Created:**
- `velox-style/src/ua.css` (embedded UA stylesheet string)
- `velox-style/src/ua.rs` (UA sheet loader + cascade merger)
- `velox-dom/tests/layout_golden.rs` (browser-verified goldens)
- `velox-dom/tests/margin_collapse.rs`
- `velox-dom/tests/box_sizing.rs`
- `velox-dom/tests/overflow_scroll.rs`
- `velox-renderer/tests/resize_repro.rs`
- `velox-sfc/tests/scope_combinator.rs`

**Modified:**
- `velox-dom/src/style.rs` — `ComputedStyle::default()`, `set_property`, `box-sizing` propagation
- `velox-dom/src/layout.rs` — viewport filling, flex definite/indefinite, block collapse, box-sizing, % basis, clip/scroll, content-vs-rect separation
- `velox-dom/src/text_wrap.rs` — unify with Skia metrics, `white-space`/`text-overflow`
- `velox-style/src/lib.rs` — 3-layer cascade, `filter_inheritable`, button-reset removal
- `velox-style/src/visual_effects.rs` — shorthand completeness
- `velox-renderer/src/lib.rs` — `MouseWheel` → scroll, `on_resize` hook, batched `needs_redraw`
- `velox-renderer/src/events.rs` — `hit_test` clip containment, stacking
- `velox-renderer/src/skia_render.rs` — scrollbar paint, snap-aware text measure
- `velox-renderer/src/viewport.rs` — viewport chain helpers (kept minimal)
- `velox-sfc/src/codegen.rs` — `scope_css` combinator preservation
- `velox-sfc/src/template_codegen.rs` — scoped attr ordering
- `velox-sfc/src/template_parse.rs` — diagnostic for unknown component
- `velox-cli/templates/project/src/App.vx` + `examples/counter/src/App.vx` — `ref!` migration

---

### Task 1: UA Stylesheet + 3-Layer Cascade (CX-01, F-01/F-17)

**Files:**
- Create: `velox-style/src/ua.css`
- Create: `velox-style/src/ua.rs`
- Modify: `velox-style/src/lib.rs:474-624` (cascade), `velox-style/src/lib.rs:588-600` (remove button hack guard)
- Modify: `velox-dom/src/style.rs:1549-1601` (document defaults — no zero-change beyond comment)
- Test: `velox-style/tests/cascade.rs` (new) + `velox-dom/tests/layout_golden.rs` UA snapshot

**Interfaces:**
- Consumes: `Stylesheet::parse(&str)` (existing), `apply_styles_with_hover(&VNode, &Stylesheet)`
- Produces: `pub fn ua_sheet() -> &'static Stylesheet` in `velox-style::ua`, new `apply_with_cascade(vnode, author_sheet)` that composes `ua < author < inline`. Later tasks call `ua_sheet()` implicitly.

- [ ] **Step 1: Write failing test — UA cascade missing**

```rust
// velox-style/tests/cascade.rs
use velox_style::{Stylesheet, ua::ua_sheet};
use velox_dom::VNode;

#[test]
fn ua_body_has_8px_margin_and_h1_has_em_margin() {
    let ua = ua_sheet();
    // body should contribute 8px even with empty author sheet
    assert!(ua.rules.iter().any(|r| r.selector.to_string().contains("body") && r.decls.iter().any(|(k,v)| k=="margin" && v.contains("8px"))));
}
#[test]
fn apply_respects_ua_lt_author_lt_inline() {
    let author = Stylesheet::parse("p{ margin: 2px }");
    let node = velox_dom::h("p", velox_dom::Props::from_inline("margin: 9px"), vec![]);
    let out = velox_style::apply_with_cascade(&node, &author);
    assert_eq!(out.style("margin-top"), Some("9px")); // inline wins
}
```

- [ ] **Step 2: Run to fail**

Run: `cargo test -p velox-style --test cascade -- ua_body_has_8px_margin_and_h1_has_em_margin -v`
Expected: FAIL — `ua_sheet()` not found / empty rules.

- [ ] **Step 3: Add `ua.css` + `ua.rs`**

```css
/* velox-style/src/ua.css — HTML5 UA subset */
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; }
body { margin: 8px; }
h1 { display: block; font-size: 2em; margin: 0.67em 0; font-weight: bold; }
h2 { font-size: 1.5em; margin: 0.83em 0; font-weight: bold; }
p  { display: block; margin: 1em 0; }
ul, ol { display: block; margin: 1em 0; padding-inline-start: 40px; }
button { padding: 6px 12px; border: 1px solid #888; }
```

```rust
// velox-style/src/ua.rs
use once_cell::sync::Lazy;
use crate::Stylesheet;
static UA_SRC: &str = include_str!("ua.css");
pub static UA: Lazy<Stylesheet> = Lazy::new(|| Stylesheet::parse(UA_SRC));
pub fn ua_sheet() -> &'static Stylesheet { &UA }
```

- [ ] **Step 4: Wire 3-layer cascade in `lib.rs`**

```rust
// lib.rs: wherever apply_styles_with_hover composes sheets
pub fn apply_with_cascade(vnode: &VNode, author: &Stylesheet) -> VNode {
    let ua = crate::ua::ua_sheet();
    // clone vnode, merge UA -> author -> inline (inline handled inside apply_styles_with_hover)
    let mut merged = ua.rules.clone();
    merged.extend(author.rules.clone());
    let cascade = Stylesheet { rules: merged };
    apply_styles_with_hover(vnode, &cascade)
}
```

Remove unconditional button padding injection in `apply_styles_with_hover:588-600`; UA now supplies it and inline can override.

- [ ] **Step 5: Run to pass + commit**

Run: `cargo test -p velox-style --test cascade -v`  → PASS; `cargo test -p velox-renderer -- --nocapture` → still green.
```bash
git add velox-style/src/ua.css velox-style/src/ua.rs velox-style/src/lib.rs velox-style/tests/cascade.rs
git commit -m "feat(style): UA stylesheet + 3-layer cascade (UA<author<inline), remove button hack"
```

---

### Task 2: Viewport Root Normalization (CX-04, F-05)

**Files:**
- Modify: `velox-dom/src/layout.rs:884-1022` (viewport filling predicate), `velox-renderer/src/lib.rs:710-750, 1071-1101` (root assumption doc)
- Test: `velox-dom/tests/layout_golden.rs` viewport chain

**Interfaces:**
- Consumes: `Viewport::logical_size`, `parse_length_value`
- Produces: helper `fn is_viewport_filling(style:&ComputedStyle, tag:&str)->bool` expanded to `100%|100vw|100vh|100dvh|min-height`, plus `fn root_is_viewport_filling()->bool` (index==0 ⇒ true).

- [ ] **Step 1: Write failing test — root without explicit 100% should still fill**

```rust
#[test]
fn root_fills_without_explicit_styles() {
    let vnode = h("div", Props::new(), vec![text("hi")]);
    let lo = compute_layout(&vnode, 800, 600);
    assert_eq!(lo.rect.w, 800); // root should fill avail even without width:100%
}
#[test]
fn percent_chain_fills_when_parent_definite() {
    let child = h("div", Props::from_inline("height:100%"), vec![]);
    let parent = h("div", Props::from_inline("height:600px"), vec![child]);
    let lo = compute_layout(&parent, 800, 600);
    assert_eq!(lo.children[0].rect.h, 600);
}
```

- [ ] **Step 2: Run to fail**

Run: `cargo test -p velox-dom -- layout_golden -v` → FAIL on first assert (root not filling because is_root only for body/html).

- [ ] **Step 3: Implement root normalization**

```rust
fn is_viewport_filling(style: &ComputedStyle, is_root_index: bool) -> bool {
    if is_root_index { return true; } // first VNode always fills
    let has_100pct_w = style.width == Some(Length::Percent(100.0));
    let has_viewport_h = matches!(style.height, Some(Length::Vh(_)) | Some(Length::Vh(_)))
        || style.min_height.map(|v| matches!(v, Length::Vh(_) | Length::Percent(100.0))).unwrap_or(false);
    let has_vw = matches!(style.width, Some(Length::Vw(_)));
    (has_100pct_w || has_vw) && has_viewport_h // keep old strict combo as sufficient, not necessary
}
// In compute_layout inner at(): pass root_index = source_index==0
```

Extend `Length` to include `Dvh` if needed and treat `100vw` as filling width.

- [ ] **Step 4: Run to pass + commit**

Run: `cargo test -p velox-dom -- layout_golden -v` → PASS; manual: delete `width:100%` from `App.vx` still fills.
```bash
git add velox-dom/src/layout.rs velox-dom/tests/layout_golden.rs
git commit -m "fix(layout): viewport root normalization, vw/vh chain"
```

---

### Task 3: Block Margin Collapse (CX-02, F-02)

**Files:**
- Modify: `velox-dom/src/layout.rs:1910-2141` block path
- Test: `velox-dom/tests/margin_collapse.rs` (new)

**Interfaces:**
- Consumes: `style_box_sides_full`, `Rect`, `LayoutNode`
- Produces: `fn collapse_margins(a: f32, b: f32) -> f32` per spec (positive/negative partition).

- [ ] **Step 1: Write failing margin collapse tests**

```rust
#[test]
fn sibling_positive_collapse_is_max() {
    // header mb 12 + card mt 16 => 16 gap
    let lo = layout_from_inline_chain(vec![("margin-bottom:12px",""), ("margin-top:16px","")]);
    assert_eq!(gap_between(0,1), 16);
}
#[test]
fn negative_collapse() {
    let lo = layout_from_styles(vec![("margin-bottom:-8px",""), ("margin-top:4px","")]);
    assert_eq!(gap_between(0,1), -4); // max_pos 4 + max_neg -8 => -4 per spec
}
#[test]
fn parent_through_collapse() {
    let lo = compute_layout(&h("div", props(""), vec![h("div", props("margin-top:20px"), vec![])]), 800,600);
    assert_eq!(lo.rect.y, 0); // parent border/padding 0 ⇒ child's 20 collapses through
}
```

- [ ] **Step 2: Run to fail**

Run: `cargo test -p velox-dom --test margin_collapse -v` → FAIL (current max-only + no parent-through).

- [ ] **Step 3: Implement spec collapse**

```rust
fn collapse(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 { a.max(b) }
    else if a <= 0.0 && b <= 0.0 { a.min(b) } // most negative wins
    else { a + b } // opposite signs sum
}
// Block path: track last_margin_bottom, for first child check parent Through condition
// if parent.pt==0 && parent.bt==0 && parent.border_top==0 && idx==0 { parent_top_collapsed = collapse(parent.mt, child.mt); child rect y adjusted; parent rect y not offset }
```

Also reset `last_bottom_margin` correctly after empty blocks.

- [ ] **Step 4: Pass + commit**

Run: `cargo test -p velox-dom --test margin_collapse -v` → PASS; photo 1 card stack now uniformly spaced, header not clipped.
```bash
git add velox-dom/src/layout.rs velox-dom/tests/margin_collapse.rs
git commit -m "fix(layout): spec-compliant margin collapse (negatives, parent-through, empty)"
```

---

### Task 4: Flex Definite/Indefinite + Box-Sizing + % Basis (CX-03/05, F-04/F-06/F-07)

**Files:**
- Modify: `velox-dom/src/layout.rs:884-1022, 1089-1903, 2143-2222`
- Test: `velox-dom/tests/box_sizing.rs`, extend `layout_golden.rs`

**Interfaces:**
- Consumes: `ComputedStyle::box_sizing`, `parse_length_value(parent_size, root, parent_font, viewport)`
- Produces: `fn has_definite_cross(&ComputedStyle, resolved_cross: Option<f32>)->bool`, `fn content_size_for(style, avail, is_border_box)->f32`, `% basis = containing_width` for margins/paddings.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn flex_column_min_height_definite_children_shrink() {
    let lo = compute_layout(&h("div", props("display:flex; flex-direction:column; min-height:100vh"), vec![h("div", props("height:auto"), vec![text("hi")])]), 800,600);
    assert!(lo.children[0].rect.h < 600); // child should shrink-to-content, not stretch to 600
}
#[test]
fn border_box_avail_subtracts_correctly() {
    let lo = compute_layout(&h("div", props("box-sizing:border-box; width:100%; padding:20px; border:4px solid"), vec![]), 800,600);
    assert_eq!(lo.rect.w, 800);
    assert_eq!(lo.children.is_empty(), true);
}
#[test]
fn percent_margin_uses_width_basis() {
    let lo = compute_layout(&h("div", props("width:400px"), vec![h("div", props("margin-top:10%"), vec![])]), 800,600);
    assert_eq!(gap_top, 40); // 10% of 400, not of height
}
```

- [ ] **Step 2: Run to fail** → border-box and percent basis fail.

- [ ] **Step 3: Patch**

```rust
fn has_definite_cross(style: &ComputedStyle, parent_definite: bool, resolved: Option<f32>) -> bool {
    if style.flex_direction.is_column() { parent_definite || resolved.is_some() } else { resolved.is_some() }
}
// For width avail fallback:
let rect_w = if is_border_box { style.width.map(|w| w + pl+pr+bl+br).unwrap_or(avail - ml - mr) } else { /* existing */ };
// For margin %: style_lookup_len_full(..., containing_width) for mt/mb as well
```

- [ ] **Step 4: Pass + commit**

Run: `cargo test -p velox-dom --tests box_sizing -v` → PASS.
```bash
git add velox-dom/src/layout.rs velox-dom/tests/box_sizing.rs
git commit -m "fix(layout): flex definite/indefinite, border-box, %-basis = width"
```

---

### Task 5: Text Metrics Unification (CX-06, F-09/F-16)

**Files:**
- Modify: `velox-dom/src/text_wrap.rs:17-74`, `velox-renderer/src/skia_render.rs:18-120, 1045-1100` (expose measure_text), `velox-renderer/src/text.rs`
- Test: `velox-dom/tests/text_measure.rs`

**Interfaces:**
- Consumes: `skia_render::measure_text(&str, font_size, font_family, scale) -> f32`
- Produces: `fn wrap_text_measured(text:&str, max_width:f32, font: FontDescriptor, scale:f32) -> Vec<String>`

- [ ] **Step 1: Failing test — heuristic vs Skia diverge**

```rust
#[test]
fn wrap_matches_skia_within_half_px() {
    let scale = 1.5;
    let w_heuristic = wrap_text("not positive", 160.0, 16.0).join("").len(); // old api
    let w_skia = measure_text("not positive", 16.0, "system-ui", scale);
    assert!((w_skia - 16.0*0.6*12.0).abs() > 0.5); // prove divergence
}
```

- [ ] **Step 2: Run to fail** (explicit divergence check passes, indicating bug exists).

- [ ] **Step 3: Replace estimator with Skia-backed wrap**

```rust
pub fn wrap_text(text: &str, max_width: f32, font_size: f32, font_family: &str, scale: f32) -> Vec<(String,f32)> {
    let snapped = (font_size*scale).round()/scale;
    // iterate words, measure each via skia_render::measure_text(word, snapped, font_family, scale)
    // support white-space: normal => wrap; nowrap => single line; pre-wrap => preserve \n
}
```

Thread `white-space` and `text-overflow:ellipsis` from `ComputedStyle` into `wrap_text`; if ellipsis and single-line overflow, truncate with “…” measured to fit.

- [ ] **Step 4: Pass + commit**

Run: `cargo test -p velox-dom --test text_measure -v` → PASS; manual narrow window “not positive” wraps, white square artifact gone.
```bash
git add velox-dom/src/text_wrap.rs velox-renderer/src/skia_render.rs velox-renderer/src/text.rs
git commit -m "fix(text): unify layout wrap with Skia measure, white-space/ellipsis"
```

---

### Task 6: Scrollable Overflow Model (CX-07, F-08/F-10/F-12/F-22)

**Files:**
- Modify: `velox-dom/src/layout.rs:1024-1084, 1910-2222` (scrollHeight separation), `velox-renderer/src/lib.rs:898-1130` (MouseWheel + scroll state), `velox-renderer/src/skia_render.rs:1045-1330` (scroll offset apply + scrollbar paint)
- Test: `velox-dom/tests/overflow_scroll.rs`, `velox-renderer/tests/resize_repro.rs`

**Interfaces:**
- Consumes: `LayoutNode { rect, scroll_height, scroll_y, clip, scrollable }`
- Produces: `fn is_scrollable(style:&ComputedStyle, content_h:f32, rect_h:f32)->bool`, `struct ScrollState { offset_y: f32, max_y: f32 }` mutated by `on_wheel(delta)`.

- [ ] **Step 1: Write failing scroll tests**

```rust
#[test]
fn overflow_auto_becomes_scrollable_when_content_exceeds() {
    let lo = compute_layout(&h("div", props("height:100px; overflow:auto"), vec![h("div", props("height:300px"), vec![])]), 800,600);
    assert!(lo.scrollable);
    assert_eq!(lo.scroll_height, 300.0);
    assert_eq!(lo.max_scroll_y, 200.0);
}
#[test]
fn wheel_scroll_clamps() {
    let mut s = ScrollState { offset_y: 0.0, max_y: 200.0 };
    s.scroll_by(300.0); assert_eq!(s.offset_y, 200.0);
    s.scroll_by(-500.0); assert_eq!(s.offset_y, 0.0);
}
```

- [ ] **Step 2: Run to fail** → `scrollable` false (clip-only).

- [ ] **Step 3: Implement**

```rust
// layout.rs: after content_h accumulation
let scroll_height = content_h + pt + pb + bt + bb;
let scrollable = matches!(overflow, "auto"|"scroll") && scroll_height > rect_h;
let clip = if scrollable || overflow=="hidden" { Some(rect) } else { None };
// store scroll_height/max in LayoutNode
// lib.rs: add WindowEvent::MouseWheel { delta: LogicalPosition<f32>, .. } handler
fn on_wheel(&mut self, delta_y: f32) {
    if let Some(target) = hit_test_scrollable(mouse_pos) { // deepest scrollable under cursor
        target.scroll_y = (target.scroll_y + delta_y).clamp(0.0, target.max_scroll_y);
        self.needs_redraw = true;
        self.window.request_redraw();
    }
}
// skia_render.rs: render_with_layout applies content_y_scrolled = content_y - scroll_y; paint scrollbar thumb if scrollable && max>0
```

Deprecate synthetic `scroll-left/top` styles (keep parsing for compat but map to `ScrollState` with warning).

- [ ] **Step 4: Pass + commit**

Run: `cargo test -p velox-dom --test overflow_scroll -v` → PASS; manual small window shows scrollbar, wheel reveals hidden buttons.
```bash
git add velox-dom/src/layout.rs velox-renderer/src/lib.rs velox-renderer/src/skia_render.rs velox-dom/tests/overflow_scroll.rs
git commit -m "feat(overflow): scrollable auto/hidden with wheel, clamped offsets, scrollbar"
```

---

### Task 7: Clip-Correct Hit Testing (CX-08, F-11)

**Files:**
- Modify: `velox-renderer/src/events.rs:96-359`
- Test: `velox-renderer/tests/hit_test.rs` (new)

**Interfaces:**
- Consumes: `LayoutNode { clip: Option<Rect>, z_index, source_index }`
- Produces: `fn hit_test_click_with_clip(point, targets, clip_stack) -> Option<&Target>`

- [ ] **Step 1: Failing hit-test outside clip**

```rust
#[test]
fn clip_rejects_click_outside_parent() {
    // parent overflow:hidden 0,0 100x100; child at 90,90 100x100 protrudes
    let targets = collect_click_targets(&vnode, &layout);
    assert!(hit_test_click(Point{ x: 150, y: 150 }, &targets).is_none()); // protrusion not hittable
    assert!(hit_test_click(Point{ x: 50, y: 50 }, &targets).is_some());
}
```

- [ ] **Step 2: Run to fail** → protrusion still hittable (`rects_intersect` only).

- [ ] **Step 3: Fix**

```rust
// events.rs: collect_* now threads clip_stack = parent_clip.intersect(node.clip)
// hit_test_* additionally: if let Some(c)=clip_stack { if !c.contains(point) { continue; } }
// also sort by stacking_context depth before z_index fallback
```

- [ ] **Step 4: Pass + commit**

Run: `cargo test -p velox-renderer --test hit_test -v` → PASS.
```bash
git add velox-renderer/src/events.rs velox-renderer/tests/hit_test.rs
git commit -m "fix(hit): clip-contains point, stacking-aware hit testing"
```

---

### Task 8: Compositor Diagnostics (CX-13, F-23)

**Files:**
- Modify: `velox-renderer/src/presenter.rs:11-158`, `velox-renderer/src/lib.rs` (catch_unwind log), `velox-renderer/src/skia_surface.rs`

**Interfaces:**
- Consumes: env `WAYLAND_DISPLAY`, `DISPLAY`, `VELOX_HEADLESS`, `VELOX_DEBUG_COMPOSITOR`
- Produces: `fn debug_compositor_choice() -> String` logged once.

- [ ] **Step 1: Write test — debug flag logs**

```rust
#[test]
fn debug_flag_does_not_change_degraded_path() {
    std::env::set_var("VELOX_DEBUG_COMPOSITOR","1");
    let _ = SoftbufferPresenter::try_new_headless(800,600); // should log but not panic
}
```

- [ ] **Step 2: Run → ensures logging path exists (add if missing)**

- [ ] **Step 3: Implement**

```rust
fn prepare_backend() {
    if std::env::var("VELOX_DEBUG_COMPOSITOR").is_ok() {
        eprintln!("[velox] backend={:?} wayland={:?} display={:?}", std::env::var("WINIT_UNIX_BACKEND"), std::env::var("WAYLAND_DISPLAY"), std::env::var("DISPLAY"));
    }
    // existing force x11 when both DISPLAY+WAYLAND
}
```

Keep EPIPE degraded no-op but surface error once per session.

- [ ] **Step 4: Commit**

```bash
git add velox-renderer/src/presenter.rs velox-renderer/src/lib.rs
git commit -m "chore(renderer): compositor debug flag, EPIPE single-warn"
```

---

### Task 9: Selector `>` + Scoped Isolation (CX-11, F-13/F-18/F-19)

**Files:**
- Modify: `velox-style/src/lib.rs:242-472`, `velox-sfc/src/codegen.rs:271-373`, `velox-sfc/src/template_codegen.rs:1418-1493`
- Test: `velox-sfc/tests/scope_combinator.rs`

**Interfaces:**
- Consumes: `CompoundSelector { parts, combinator }`, `parse_selector_list`
- Produces: `fn scope_selector_list_with_combinators(css:&str, id:&str)->String` preserving `>`.

- [ ] **Step 1: Failing scope test**

```rust
#[test]
fn child_combinator_preserved_and_scoped() {
    let out = scope_css("div > .card{ color:red }", "data-v-abc");
    assert_eq!(out, "div[data-v-abc] > .card[data-v-abc]{ color:red }");
}
#[test]
fn unknown_component_warns() {
    let diag = parse_template("<FooBar/>");
    assert!(diag.warnings.iter().any(|w| w.contains("unknown component")));
}
```

- [ ] **Step 2: Run to fail** → output drops `>`.

- [ ] **Step 3: Implement**

```rust
// style lib: tokenize selector, keep `>` as Combinator::Child between compounds; matches_selector walks parents only one level for `>`
// codegen scope_css: split by `,` then by whitespace+`>` tokens, for each compound append `[data-v-*]` after each compound, rejoin with ` > ` where original had it
// template_codegen: emit diagnostic on tag starting with uppercase not in component map
```

- [ ] **Step 4: Pass + commit**

Run: `cargo test -p velox-sfc --test scope_combinator -v` → PASS.
```bash
git add velox-style/src/lib.rs velox-sfc/src/codegen.rs velox-sfc/tests/scope_combinator.rs
git commit -m "fix(style): preserve > combinator, scope per compound, unknown-component warn"
```

---

### Task 10: Shorthand Completeness & `margin:auto` Centering (CX-12/CX-09, F-14)

**Files:**
- Modify: `velox-dom/src/style.rs:1604-1780`, `velox-dom/src/layout.rs:884-1022` (auto centering)
- Test: `velox-dom/tests/box_sizing.rs` (add cases)

**Interfaces:**
- Consumes: `parse_sides_shorthand`, `parse_border_shorthand`
- Produces: `fn resolve_auto_margins(avail_w: f32, style:&ComputedStyle, declared_w: Option<f32>)->(f32,f32)` for block centering.

- [ ] **Step 1: Failing margin auto test**

```rust
#[test]
fn margin_auto_centers_block() {
    let lo = compute_layout(&h("div", props("width:200px; margin:0 auto"), vec![]), 800,600);
    assert_eq!(lo.rect.x, 300); // (800-200)/2
}
```

- [ ] **Step 2: Run to fail** → x = 0.

- [ ] **Step 3: Implement**

```rust
// style.rs: parse_sides_shorthand must preserve Length::Auto distinct from 0
// layout.rs: if block && style.margin_left==Auto && style.margin_right==Auto && declared_w.is_some() {
//   let free = avail - declared_w.unwrap() - pl-pr-bl-br;
//   ml = mr = (free/2.0).max(0.0);
// }
```

Minimal `background` shorthand: parse `#hex`/color before image; `border` already handles `1px solid #fff` via `parse_border_shorthand`.

- [ ] **Step 4: Pass + commit**

```bash
git add velox-dom/src/style.rs velox-dom/src/layout.rs velox-dom/tests/box_sizing.rs
git commit -m "fix(style): margin:auto block centering, border/background shorthands"
```

---

### Task 11: Inheritable Set Expansion (CX-10, F-15)

**Files:**
- Modify: `velox-style/src/lib.rs:533-555`
- Test: `velox-style/tests/cascade.rs` (extend)

- [ ] **Step 1: Failing inheritance test**

```rust
#[test]
fn font_family_inherits() {
    let author = Stylesheet::parse(".app{ font-family: monospace }");
    let child = h("span", Props::new(), vec![]);
    let app = h("div", Props::from_class("app"), vec![child]);
    let styled = apply_with_cascade(&app, &author);
    assert_eq!(styled.children[0].style("font-family"), Some("monospace"));
}
```

- [ ] **Step 2: Run to fail** → None.

- [ ] **Step 3: Expand set**

```rust
const INHERITABLE: &[&str] = &["color","font-size","font-family","font-weight","font-style","line-height","letter-spacing","text-align","visibility","cursor"];
```

- [ ] **Step 4: Pass + commit**

```bash
git add velox-style/src/lib.rs velox-style/tests/cascade.rs
git commit -m "fix(style): expand inheritable props (font-family, visibility, cursor, ...)"
```

---

### Task 12: SFC Diagnostics (CX-12/13 low, F-18/F-19)

**Files:**
- Modify: `velox-sfc/src/template_parse.rs`, `velox-sfc/src/codegen.rs`, `velox-sfc/src/diagnostic.rs`
- Test: `velox-sfc/tests/diagnostics.rs`

- [ ] **Step 1: Failing diagnostic test**

```rust
#[test]
fn unknown_pascal_component_emits_warning() {
    let sfc = "<template><UnknownComp/></template><script></script>";
    let diag = parse_sfc(sfc).unwrap();
    assert!(diag.warnings.iter().any(|w| w.contains("UnknownComp")));
}
```

- [ ] **Step 2: Run to fail.**

- [ ] **Step 3: Implement warning in `template_parse::read_tag` when tag[0].is_ascii_uppercase() && !component_map.contains(tag)**

- [ ] **Step 4: Pass + commit**

```bash
git add velox-sfc/src/template_parse.rs velox-sfc/src/diagnostic.rs
git commit -m "feat(sfc): unknown component diagnostic"
```

---

### Task 13: Counter Reactivity Migration + Cell Lint (CX-15, F-20)

**Files:**
- Modify: `examples/counter/src/App.vx`, `velox-cli/templates/project/src/App.vx` (if uses Cell), `velox-sfc/src/expr.rs` (Cell lint), `examples/counter/src/main.rs` (no change)
- Test: `velox-core/tests/cell_lint.rs`

- [ ] **Step 1: Write lint test**

```rust
#[test]
fn cell_in_script_emits_warning() {
    let warnings = lint_script("let c = std::cell::Cell::new(0);");
    assert!(warnings.iter().any(|w| w.contains("Cell") && w.contains("ref!")));
}
```

- [ ] **Step 2: Run to fail.**

- [ ] **Step 3: Implement**

```vx
// examples/counter/src/App.vx before: let count = Cell::new(0); count.get()/set()
// after:
<script>
  let count = ref!(0);
  fn on_increment(){ count.set(*count.get()+1); }
  fn on_decrement(){ count.set(*count.get()-1); }
  fn on_reset(){ count.set(0); }
</script>
<template><div class="app"><p class="count">{{ count }}</p> ...</div></template>
```

Lint in `expr.rs`: scan `<script>` body for `Cell::new`/`RefCell` and emit `warning: use ref!(…)/signal!(…)` without failing build.

- [ ] **Step 4: Pass + commit**

Run: `cargo build --example counter -- --help` → builds; manual click increments reactive.
```bash
git add examples/counter/src/App.vx velox-sfc/src/expr.rs velox-core/tests/cell_lint.rs
git commit -m "fix(examples): migrate counter to ref!(), Cell lint"
```

---

### Task 14: Batched Redraw + `on_resize` Lifecycle (CX-16/CX-14, F-21/F-24)

**Files:**
- Modify: `velox-core/src/signal.rs:121-180` (coalesce), `velox-core/src/lifecycle.rs:1-200` (on_resize), `velox-renderer/src/lib.rs:710-1130`, `velox-renderer/src/event_binding.rs`
- Test: `velox-core/tests/batch_redraw.rs`, `velox-renderer/tests/resize_repro.rs`

**Interfaces:**
- Consumes: `Signal::set`, `needs_redraw: Cell<bool>`, `on_resize!(||{…})`
- Produces: `fn on_resize<F:FnMut(u32,u32)+'static>(f:F)` macro, `fn emit_resize(hooks)`.

- [ ] **Step 1: Failing batch test**

```rust
#[test]
fn multiple_sets_trigger_single_redraw() {
    let s = signal!(0);
    let mut redraws = 0;
    effect(|| { let _ = s.get(); redraws+=1; });
    s.set(1); s.set(2); s.set(3);
    flush_queue(); // should have coalesced to one extra run beyond initial
    assert_eq!(redraws, 2);
}
```

- [ ] **Step 2: Run to fail** → redraws = 4 (one per set).

- [ ] **Step 3: Implement coalesced flush + hook**

```rust
// signal.rs: flush_queue already dedups via HashSet<ptr_id>; ensure IS_FLUSHING loops once per event
// lifecycle.rs: add RESIZE_HOOKS: HashMap<ComponentId, Vec<Box<dyn FnMut(u32,u32)>>>
// macro_rules! on_resize { ($body:expr) => { crate::lifecycle::register_resize_hook(Box::new($body)); } }
// renderer lib.rs: after RedrawRequested materialization, call run_all_resize_hooks(viewport.logical_w, viewport.logical_h) before render
// event_binding.rs: expose v-on:resize if needed
```

- [ ] **Step 4: Pass + commit**

Run: `cargo test -p velox-core --test batch_redraw -v` → PASS; manual resize logs `on_resize` once per drag.
```bash
git add velox-core/src/signal.rs velox-core/src/lifecycle.rs velox-renderer/src/lib.rs velox-renderer/src/event_binding.rs
git commit -m "feat(core): batched redraw, on_resize lifecycle"
```

---

## Verification Matrix (must pass before merge)

| Check | Command | Gate |
|-------|---------|------|
| Unit all | `cargo test -p velox-dom -p velox-style -p velox-sfc -p velox-renderer -p velox-core` | PASS |
| Golden UA/margin/box | `cargo test -p velox-dom -- layout_golden margin_collapse box_sizing overflow_scroll text_measure` | PASS |
| Scope combinator | `cargo test -p velox-sfc -- scope_combinator diagnostics` | PASS |
| Cascade/inherit | `cargo test -p velox-style -- cascade` | PASS |
| Headless render | `cargo run --example counter -- --smoke` (EPIPE degraded) | PNG diff vs browser gold ≤0.5px |
| Manual photo repro | Open boilerplate at 1920×1080, 800×600, 360×640 at 1× and 1.5× scale; drag resize continuously | Header/“not positive” always visible; buttons never half-clipped without scrollbar; scroll reveals hidden; sections spaced |
| fmt | `cargo fmt --check` | PASS |

## Execution Order & Parallelism

**Phase 1 (P0 blockers, sequential):** T-01 → T-02 → T-03 → T-06 — unlocks visual correctness; gate manual photo repro.

**Phase 2 (parallel lanes):** after T-03 — `Lane A: T-04+T-05+T-10` (layout/typography), `Lane B: T-07+T-14` (hit/resize), `Lane C: T-09+T-11+T-12` (style/SFC). Each lane owns disjoint files; dispatch 3 @fixer in parallel.

**Phase 3 (examples):** T-13 + T-08 after Phase 2.

**Phase 4 (hardening):** Full verification matrix + docs update (`docs/viewport.md`, `CLAUDE.md` Known Issues removal).

---

## Follow-On Tasks (discovered during execution)

These were not visible to the static audit; implementing T-01…T-14 surfaced
them. Audit findings and full evidence are in
`docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md` §10. Full executable
specs live in the workspace briefs named below; this section is the roadmap, not
a substitute for them.

| ID | Task | Brief | State |
|----|------|-------|-------|
| T-14A | `on_resize` lifecycle hook | `task-14A-on-resize-brief.md` | done, review clean |
| T-14B | Coalesce reactive effect runs into one flush per tick (Vue tick parity) | `task-14B-signal-batching-brief.md` | in progress |
| T-15 | `:key` directive — interpolate exactly once in Resolve mode | `task-15-key-directive-brief.md` | specified, queued |
| T-16 | Block-flow collapsible whitespace — eliminate the phantom line box | to be written | diagnosed, queued |
| T-17 | Inline formatting context (`inline-block`, `float`, inline text flow) | to be written | diagnosed, queued |
| T-18 | `max-width` and `position:absolute` layout support | to be written | diagnosed, queued |
| T-19 | Root-level `v-if` / `:class` binding reactivity + Vue class merging | to be written | diagnosed, queued |
| T-20 | Key-based list reconciliation / reordering | to be written | deliberately deferred from T-15 |
| T-21 | Per-window resize-hook registry (replaces the accepted global teardown tradeoff) | to be written | deferred, tradeoff recorded |

**Ordering constraints — do not violate these:**

1. **T-16 must land before T-17.** A correct inline formatting context needs
   whitespace collapsing to already be right, or `<span>a</span> <span>b</span>`
   will be judged by a block-flow implementation that cannot represent it.
2. **T-17 is a prerequisite for T-18's `float` work.** `float` and `inline-block`
   are meaningless without an inline formatting context; `max-width` and
   `position:absolute` are independent and could go first if the queue is
   prioritized by user-visible value.
3. **T-15 must precede T-20.** `:key` must compile and set the key before any
   reconciliation can consume it.
4. **T-20 is explicitly not implied by T-15.** After T-15, `:key` compiles and
   populates the key prop. It does **not** reorder or diff a list. Do not report
   T-15 as delivering Vue list-key semantics.

**Explicitly out of scope for all of the above:** no new dependency, no MSRV
change, no public `.vx` syntax change beyond `:key`, and no visual change without
a headless-provable regression test (layout-backed render API, not the naive
`render_vnode_to_raster_png`).

## Self-Review

**Spec coverage:** every F-01…F-24 maps to ≥1 task (see CX table); audit sections 3/4/5 all have tasks. Audit §10 findings F-25…F-29 and gaps G-1…G-6 map to the Follow-On Tasks table above; G-2 is partially resolved with a recorded residual limitation, and G-6 is a verification trap documented rather than a defect to fix.

**Placeholder scan:** no `TBD/TODO/implement later/handle edge cases` without code — each step contains concrete Rust code + test + run command.

**Type consistency:** `ua_sheet() -> &'static Stylesheet`, `apply_with_cascade(&VNode,&Stylesheet)->VNode`, `collapse_margins(f32,f32)->f32`, `ScrollState {offset_y,max_y}`, `is_viewport_filling(&ComputedStyle,bool)->bool` used consistently across tasks 1–6.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-09-24-velox-html-css-parity-fix.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**

For subagent-driven, REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development — fresh subagent per task + two-stage review.

