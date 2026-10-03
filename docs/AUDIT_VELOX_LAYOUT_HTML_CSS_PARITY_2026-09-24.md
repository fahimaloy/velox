# Velox — HTML/CSS/Vue Behavioral Parity Audit

**Date:** 2026-09-24
**Scope:** Full-system deep dive triggered by window screenshots (full-screen “Velox App” boilerplate: sections touching, no margins, header / “not positive” state invisible, count only visible on small resize; overflow, spacing, resizing fundamentally unlike browser).
**Goal:** Every element, style, component, prop, and layout interaction must behave like the browser/ Vue default — including overflow, resize, responsiveness, compositor independence, HiDPI, and viewport filling.
**Auditors:** orchestrator + explorer lanes 1–3 (layout/renderer, SFC/template/style, core/examples). Graph coverage validated via `search_graph` + source `read` fallback.
**Severity:** 🔴 P0 blocker, 🟠 P1 critical, 🟡 P2 important, 🔵 P3 nice-to-have.

---

## 1. Executive Summary

Velox does **not** implement the browser’s default stylesheet, content-based (shrink-to-fit) inline/block model, margin-collapsing, viewport-filling, scrollable overflow, or responsive reflow contract. Instead it does an immediate-mode, full-rebuild render of a Skia raster at a *single* logical size derived from `Viewport`, with a custom flex+block layout that is **necessary but insufficient**: many CSS defaults are hard-zero, container sizing keys off `width:100%+min-height:100vh` (brittle — only injected in one template), text is measured with a 0.6×font heuristic distinct from Skia glyph metrics, overflow is “clip-only” with a *synthetic* `scroll-left/top` property (no wheel/scrollbar), and resize is coalesced-deferred. Result: large windows look cramped and “touching”; mid/small windows expose clipping inversion (the count survives while header/status clip out) exactly as photographed.

There are **24 findings** grouped into 9 domains. 11 are P0/P1 and block “Vue-like” correctness. The attached plan decomposes them into **14 tasks in 5 phases** so that fixes can be proved incrementally (each phase ships testable, visual behavior).

**Most urgent theorem:** Until a browser-equivalent *user-agent stylesheet* + correct block flow (margin collapse, content-height propagation, available-size distribution, and scrollable overflow) are in place, no amount of per-component styling will make Velox feel like HTML.

---

## 2. Symptom → Evidence Map (What the photos prove)

| Photo | Visible symptom | Proved defect |
|-------|----------------|---------------|
| **1 — full screen** | Header “Velox App” clipped to 1-line at top edge; blue-card section fills width edge-to-edge, cards “touching” bottom; “not positive” at card top, buttons with 3× blue bars but no outer gap; two dark cards stacked with no gap and content vertically stretched | **F-01** missing UA margins (body 8px, h1/p margins, card gaps) + **F-02** block sibling collapse collapsed to `max()` only for siblings but not parent-child, so vertical rhythm lost; **F-06** flex `stretch` on indefinite axis causes card to soak `UNCONSTRAINED_CROSS_SIZE` → full-bleed; **F-08** clip hierarchy missing `overflow:visible` propagate → header clipped by card sibling’s `clip` covering viewport |
| **1** | Text “positive / not positive” only partly readable | **F-11** `wrap_text` char-width heuristic vs Skia `measure_text` divergence → layout height under/over-estimated → clip rectangle eats line |
| **2 — tiny window** | Only giant “0” visible; header truncated (“Velox App” half-visible), card top cut, tiny white square artifact at card edge | **F-04** resize coalescing + single rounding correct, but **F-05** `compute_layout` content-height vs declared-height branching treats `min-height:100vh` element as `avail_h - margins` on *every* resize, then distributes `content_h_available = rect_h - pt/pb/bt/bb` downward; on small avail, `content_h_available` is tiny, collapsed margins become 0, and the only child with fixed glyph height (big “0”) survives clipping while sibling clipped rows disappear; artifact is border-radius clip intersecting scroll rectangle |
| **3 — small square** | “not positive” now visible but button “+1” half-clipped by next card; still no page scroll | **F-07** `overflow:auto/hidden/scroll` all map to `clip=Some(rect)` with *no* scrollable extension → no scrollbar, no wheel, so narrow viewport cannot scroll to reveal clipped button; **F-12** synthetic `scroll-left/top` never fed by input, so content cannot be panned |

**Inference:** The bug is not one line; it is a *systemic* absence of the browser box-model defaults and scroll model, plus two implementation divergences (text metrics, clip intersection).

---

## 3. Findings — Detailed

### Domain A — Box Model & Block Flow

#### F-01 — No User-Agent stylesheet (P0 🔴)
- **Location:** `velox-dom/src/style.rs:1549-1601` `ComputedStyle::default()` (margins/paddings/borders = 0, `display:Block`, `gap 0`); `velox-style/src/lib.rs:474-505` `merge_styles` (stylesheet low → inline high, no injected UA); `velox-dom/src/layout.rs:596-715` `style_box_sides_full` (shorthand 1–4 + longhand overrides).
- **Expected (HTML):** body `margin:8px`, h1 `margin:0.67em 0`, p `margin:1em 0`, ul `padding-inline-start:40px`, button `padding:6px 12px` only if not overridden, `display:inline` for span/a etc., `box-sizing: content-box` but many resets use `border-box`.
- **Actual:** Everything 0. So “touching” is *by construction*. Only `velox-style::apply_styles_with_hover:588-600` hard-codes button padding as a bandaid — other tags still zero. Gap only exists if author writes `gap:16px` (counter example does, but card container does not).
- **Impact:** Every fresh page looks cramped; boilerplate author must manually add margins that browsers would provide, violating “Vue-like defaults” contract.
- **Fix outline:** Introduce `ua.css` (embedded string, parsed before author sheets), with HTML5 UA subset + `*,*::before,*::after{box-sizing:border-box}` optional via flag; make `apply_styles` take an `ua_sheet` first layer; adjust `ComputedStyle::default()` comments.

#### F-02 — Margin collapsing incomplete / wrong (P0 🔴)
- **Location:** `velox-dom/src/layout.rs:1910-2141` block path `cur_y`, `last_bottom_margin`, `collapsed_margin_top = if idx==0 cmt else max(last_bottom,cmt)`, `adjusted_cur_y=cur_y+collapsed-cmt`.
- **Expected:** CSS collapse is: adjacent siblings collapse to `max(posMax) + max(neg,0)` with negative handling; parent–first-child and parent–last-child collapse *through* parents with no border/padding; empty blocks collapse through; floats/abspos break collapse.
- **Actual:** Only sibling–sibling `max()`; parent-child not collapsed; negatives not handled (max ignores sign); empty-block through-collapse missing. So `header{margin-bottom:12px}` + `card{margin-top:16px}` → 16px not 16px collapsed correctly in simple case *but* `body(0) + header(12)` should not double-count — currently does (`cur_y = rect.y+rect.h` then first child `cmt` added raw).
- **Visible:** Full-screen stacked cards appear with double gap or zero gap depending on order; header text clipped at top because `cur_y` starts at padding edge then collapsed math pushes first line beyond clip.
- **Fix:** Implement `collapse(m1,m2) -> f32` per spec (positive/negative partition), track `is_first_in_flow`, propagate `margin_through` for parents with `bt/bb/pl/pt==0`.

#### F-03 — Block vs inline vs flex formatting context conflated (P1 🟠)
- **Location:** `layout.rs:1049-1084` display/position switch; `1089-1903` flex vs `1910-2141` block are exclusive; `text_wrap.rs:17-74` interleaved in block only.
- **Expected:** Block containers with inline children wrap via line boxes; `display:inline`, `inline-block`, `inline-flex` exist; anonymous block boxes.
- **Actual:** Any `display` other than `flex` goes to block path; `inline` and `inline-block` render as block; text nodes inside flex containers are wrapped as block lines (no flex-item wrapping). This makes `“Count: {count}”` or `“not positive”` behave as block vs inline unpredictably.
- **Impact:** Vue `<span>` inline regressions; existing card text sometimes full-width block, sometimes wrapped line.
- **Fix:** Add `Inline` path (or treat non-flex non-block as block for now but document), ensure flex children that are text become anonymous flex items.

#### F-04 — Definite vs indefinite cross-size propagation for flex (P1 🟠)
- **Location:** `layout.rs:1099-1140` gaps; `has_definite_cross_size = if column {true} else explicit h/min/max`; `UNCONSTRAINED_CROSS_SIZE = i32::MAX` used for indefinite; `2143-2222` `rect_h` finalization (`if is_viewport_height→_rect_h else declared vs content_h+pt/pb/bt/bb`).
- **Expected:** Flex column with `min-height:100vh` should be definite *for cross distribution* but children with `height:auto` should still shrink-to-content for main axis. Indefinite main should be content-sized, not infinite.
- **Actual:** Column marked definite unconditionally → cross free space distributed even when parent height is indefinite due to content; row marked indefinite unless explicit dims → children measured with `i32::MAX` causing degenerate widths that alias to viewport width after clamping. Photos: card stretched vertically on full screen (definite case) vs collapsed on small (indefinite).
- **Fix:** Tie `has_definite_cross_size` to *resolved* cross size not alias; pass-down `definite_main` flag.

#### F-05 — Viewport-filling (“100% + 100vh”) detection is brittle (P0 🔴)
- **Location:** `layout.rs:884-1022` `is_root(body/html) || is_viewport_filling(100% w&&h) || has_viewport_height(100vh/min-height:100vh) → rect_w/h = avail - margins` else `declared+padding/border or avail`; `viewport.rs:16-77` `Viewport::new` clamps ≥1 logical `(phys/scale).round.max1`; `renderer/lib.rs:710-1128` `run_window_vnode_skia` `logical_size = Viewport::from_i32→logical` then `make_view(vw,vh)`, `compute_layout(vnode,vw,vh)`.
- **Expected:** `html,body{height:100%}` cascade + `min-height:100vh` on root; any intermediate `height:100%` when parent definite should also fill; resizing the window should reflow immediately (like browser flex shrink/wrap).
- **Actual:** Only element tagged `body`/`html` *or* one whose style lists both `width:100%` and `min-height|height:100vh` is treated as viewport-filling. `veloxc/templates/project/src/App.vx:1-51` and examples set that on `.app`, but any extracted component that forgets it breaks; `width:100vw` not recognised; `min-height:100%` chain not recognised. On small resize, `avail` shrinks but `is_viewport_filling` still true so `rect_h = avail - margins` (tiny) → children measured against tiny `content_h_available` → text wraps narrower, clipped height hides all but tallest glyph.
- **Fix:** Normalize: root is always first VNode (assume viewport-filling regardless of style), else respect CSS `height:100%` chain if parent definite; add `100vw/100vh/dvh` to predicate; add integration test resizing 800→360→1920 logical.

#### F-06 — Box-sizing not propagated during available fallback (P1 🟠)
- **Location:** `layout.rs:884-960` `style_lookup_len_full` + `style_box_sides_full`; branching `declared+pad/border or avail`; `content_w = rect_w - pl - pr - bl - br` but for `box-sizing:border-box` the declared width already includes padding/border.
- **Actual:** `set_property:1184-1528` parses `box-sizing`, but available-fallback `rect_w = avail - ml - mr` path does not subtract padding/border for border-box vs content-box distinction *consistently* for children with `width:auto`. Visual: full-width blue buttons fill edge-to-edge ignoring intended inner padding.
- **Fix:** Branch on `box-sizing` after `style_box_sides_full`.

### Domain B — Viewport, Resize, Responsiveness, HiDPI

#### F-07 — No responsive unit propagation for container-relative sizes (P1 🟠)
- **Location:** `layout.rs:142-245` `parse_length_value` handles `% (parent_size)`, `rem(root)`, `em(parent_font)`, `vw/vh(viewport)` but `style_lookup_len_full` called with `parent_size = avail_w/h` for width/height but *not* for `margin/padding` left/right vs top/bottom distinction (spec: vertical % margins use *width* as basis).
- **Impact:** `margin: 2%` on tall card yields gap tied to viewport height in Velox vs width in browser → destroyed proportions on resize.
- **Fix:** Pass `containing_width` for margin/padding % resolution.

#### F-08 — Resize coalescing correct, but no content-size scroll fallback (P0 🔴)
- **Location:** `renderer/lib.rs:898-904` `Resized→pending_resize+request_redraw`, `906-930` `ScaleFactorChanged→rescale mouse_pos atomically, pending_resize`, `939 CursorMoved→hit_test_hover`, `1071 RedrawRequested→materialize pending_resize via renderer.resize+presenter.resize+surface.set_scale_factor, logical_size→make_view→...→compute_layout→recompute_targets→render_frame+present`; `viewport.rs:30-77` single rounding point; `skia_surface.rs:15-161` `resize(w,h)` recreates surface; `presenter.rs:11-158` softbuffer resize, EPIPE degraded.
- **Expected:** After resize, layout recomputed at new logical, then overflow auto containers become scrollable with scrollbars/panning.
- **Actual:** Coalescing is fine (R-L3 deferred to RedrawRequested is canonical), single rounding prevents drift — *but* after recompute there is no `overflow:auto` scroll chain, so clipped content simply disappears (photo 2/3). No viewport `min-width` guard.
- **Fix:** Add scrollable overflow model (F-09) + viewport clamp optional.

#### F-09 — HiDPI correctness: snapped font vs layout mismatch (P2 🟡)
- **Location:** `skia_render.rs:18-502` `FontCache::new_with_scale/snap snapped_size=(logical*scale).round/scale`; `text.rs`; `viewport.rs` logical `(phys/scale).round.max1`.
- **Expected:** Layout uses logical units; Skia paints scaled physical; text baseline must align.
- **Actual:** `text_wrap.rs:17-74` measures with `char_width=font*0.6, space=0.5*char` heuristic — *not* Skia `measure_text` (used in `skia_render::measure_text` for painting). So layout width ≠ painted width; at scale 1.25/1.5 the snapped font changes glyph width but layout still uses linear estimate → line-break off-by-one, clip mismatch artifact (white square in photo 2 is border clip vs text rect intersection).
- **Fix:** Unify: either layout calls `measure_text` (bind Skia to layout) or precompute char widths from `FontMetrics` and ensure same snap logic in layout.

### Domain C — Overflow, Clipping, Scroll

#### F-10 — Overflow model = “clip-only”, synthetic scroll never driven (P0 🔴)
- **Location:** `layout.rs:1024-1047` `overflow=str, scroll_x/y from style scroll-left/top len (not wheel)`, `content_x/y_scrolled = content - scroll`; `2143-2222` `clip=Some(rect) if overflow hidden/scroll/auto`; `skia_render.rs:1045-1330` `render_frame: canvas.save+scale, FontCache:scale, draw via render_with_layout (apply_clips: clip_rect/rrect+clip-path inset, children sorted by z_index)`; `events.rs:36-220` `collect_*` intersect chain.
- **Expected:** `overflow:auto/hidden/scroll/visible` with `overflow-y:auto` causing clip+scrollbar when `content_h > rect_h`; wheel/touch creates scroll offset clamped to `[0, content_h - rect_h]`; scrollbar painted; `scrollLeft/Top` animatable.
- **Actual:** `hidden|scroll|auto` all → `clip=Some(elem rect)`; scroll offsets are inert style properties nobody writes (no wheel listener, no scrollbar, no clamping). `visible` → no clip (correct). So small windows cannot scroll; content beyond `rect_h` is permanently hidden.
- **Impact:** Photos 2/3: buttons below fold invisible with no way to reach.
- **Fix:** Model: for each node compute `scrollable = (overflow in {auto,scroll}) && content_h > rect_h`; create `ScrollOffset {x,y,max_x,max_y}`; handle `WindowEvent::MouseWheel` → mutate offset, clamp, request redraw; paint custom scrollbar (optional); ensure `scroll-left/top` style is deprecated.

#### F-11 — Clip stacking & hit-test leak (P1 🟠)
- **Location:** `layout.rs:1084 clip`, `skia_render::apply_clips`, `events::collect_click_targets:96-154` (`clip intersect chain, z sort, rects_intersect filter`), `hit_test_click/hover/input:230-359` (`point in rect x0<=x<=x1`).
- **Expected:** Child outside `overflow:hidden` parent should be visually clipped *and* not hittable; stacking contexts (`opacity<1|transform|z-index+position!=static`) form groups.
- **Actual:** Visual clip via canvas is correct but hit-test filter only checks `rects_intersect(rect,clip)` (does rect overlap clip rect at all?) not `point in clip`. So a protruding button that intersects parent edge at one pixel is still clickable far outside visible area. Also `stacking_context` not factored beyond z sort.
- **Fix:** `collect_*` must carry `clip_stack: Option<Rect>` intersection and `hit_test` must reject `!clip.contains(point)`.

#### F-12 — Content height vs rect height confusion for scrolling containers (P1 🟠)
- **Location:** `layout.rs:1910-2141` block `cur_y=rect.y+rect.h`, `content_h` accumulation; `2143-2222` final `rect_h: if is_viewport_height→_rect_h else declared vs content_h+pt/pb/bt/bb clamp min/max`.
- **Expected:** Container `rect_h` is viewport-clamped; `scrollHeight = content_h + padding` is independent.
- **Actual:** `min-height:100vh` path forces `rect_h = avail_h - margins`; block path then expands `rect_h` from content if not viewport-height — but flex path with `stretch` on definite cross may have already set children to `content_h_available` which equals `rect_h - padding`, so adding `pt/pb` double-counts. On small windows this yields `content_h` > `rect_h` but not exposed as scrollHeight.
- **Fix:** Separate `layout_h` vs `scroll_h`; propagate scrollHeight upward.

### Domain D — Styles & CSS Parity

#### F-13 — Selector engine incomplete; scoped rewrite drops combinators (P1 🟠)
- **Location:** `velox-style/src/lib.rs:242-367` `parse_selector_part` (tag,class,hover,attr only), `369-398` `parse_selector_list` (splits `,` then whitespace descendant chain, drops `>`), `430-472` `matches_selector` ancestor walk; `velox-sfc/src/codegen.rs:271-332` `scope_css` appends `[data-v-*]` to rightmost part, `336-373` `scope_selector_list` drops `>` .
- **Expected:** `> + ~ :nth-child :not() [attr^=] :hover` as in Vue SFC scoping.
- **Actual:** `div > .card` silently becomes `div .card`; `a:hover` handled but `button:disabled` dropped; combinator loss breaks scoped isolation (style bleeds).
- **Fix:** Preserve `>` as child combinator token; reject or transpile unsupported pseudos with diagnostic; scope inserts attribute after each compound, not just last.

#### F-14 — Shorthand parsing gaps (P2 🟡)
- **Location:** `velox-dom/src/style.rs:1604-1649` `parse_sides_shorthand`, `1651-1780` `parse_border_shorthand`; `velox-style/src/visual_effects.rs:17-389` `BoxShadow/TextShadow/BorderRadius`.
- **Expected:** `margin: 0 auto`, `margin: 8px 0 12px`, `border: 1px solid #fff`, `background: #fff url() no-repeat center`, `box-shadow: 0 2px 8px rgba(0,0,0,.2)` full.
- **Actual:** `parse_border_value: solid only` etc drop URL/gradient. `0 auto` yields `Left=Right=auto→0` (handled) but `margin: 0 auto` centering for block (`margin-left/right:auto`) not implemented → centered dialogs appear left-aligned.
- **Impact:** Screenshots: no auto-centering, no shadow visible though design expects it.
- **Fix:** Round out `margin:auto` block centering; add minimal `background` shorthand; improve `border` to support `1px solid #...`.

#### F-15 — Inheritance filter too narrow (P2 🟡)
- **Location:** `velox-style/src/lib.rs:533-555` `filter_inheritable` only `color/font-size/font-weight/text-decoration/line-height`; `apply_styles_with_hover:514-624` merges acc.
- **Expected:** Inherit `font-family`, `visibility`, `opacity` (as composited), `cursor`, etc.
- **Actual:** `font-family` set on `.app` does not inherit → fallback sans-serif per node; `visibility:hidden` early return in `skia_render` works but not inherited correctly.
- **Fix:** Expand inheritable set to `font-family, font-style, letter-spacing, text-align, visibility`.

### Domain E — Text & Typography

#### F-16 — Text wrapping not CSS-compliant (P1 🟠)
- **Location:** `velox-dom/src/text_wrap.rs:17-74` `wrap_text(char_width=font*0.6, line_height*1.2, space 0.5char, word split, >max_width wrap)`; `layout.rs:1989-2058` `text-align center/right/justify` block path.
- **Expected:** `white-space: normal | nowrap | pre-wrap`, `word-break: break-all`, `overflow-wrap: anywhere`, `line-break`, `hyphens`, `text-overflow: ellipsis`.
- **Actual:** Single hard model: break on spaces, long word forced to next line, no `nowrap`, no `ellipsis`. “not positive” at narrow width wraps incorrectly; count glyph width estimate off.
- **Fix:** Support `white-space` property + `text-overflow: ellipsis` for clipped text; use Skia metrics for wrap decision.

#### F-17 — `button` hack obscures defaults (P2 🟡)
- **Location:** `layout.rs:2143-2222` “button hack centers single child”; `velox-style/lib.rs:588-600` button padding injection.
- **Impact:** Non-button elements with 1 child get centered inadvertently; button padding cannot be removed via CSS (`padding:0` overridden by default injection if missing).
- **Fix:** Remove hack; rely on flex/button stylesheet defaults.

### Domain F — Template & SFC Compiler

#### F-18 — Silent tag fallback to `div` for unknown components (P2 🟡)
- **Location:** `template_parse.rs:7-194` hand-rolled HTML; `template_codegen.rs:918-1259` `emit_node_with_mode` (component via `data-velox-component` → persistent State field else props); `component_resolver.rs`.
- **Expected:** Vue-like: `<MyCard />` imports must match PascalCase/kebab, warn if unregistered.
- **Actual:** Unknown tags still emit as `h(tag)` with no warning -> appear as block divs with no styles → confusion.
- **Fix:** Emit warning diagnostic; case sensitive mapping via resolver.

#### F-19 — Scoped attribute appended unconditionally, attr order fragile (P2 🟡)
- **Location:** `template_codegen.rs:1418-1424` `append_scope_attr`, `1426-1493` `emit_props_with` (static/bind/on, `:class` object `{cls:cond}`→ternary push, `:key` stripped).
- **Actual:** Scoped `data-v-*` set as `.set(scope,"")` even for slot passthrough; ordering of props matters for specificity tests.
- **Fix:** Ensure scoped attr after user attrs, not before inline merge.

### Domain G — Reactivity & Render Loop

#### F-20 — `Cell` demo non-reactive (P1 🟠)
- **Location:** `examples/counter/src/App.vx:1-83` `Cell<i32>` counter, `count()->i32`; `velox-core/src/ergonomics.rs:27-90` `Ref/ShallowRef` vs `Signal`.
- **Actual:** `Cell` not subscribed → works only because manual `request_redraw` after `on_event`; `computed/watch` demos look broken.
- **Fix:** Migrate boilerplate to `ref!(0)` / `signal!` idiom; lint `Cell` in `<script>`.

#### F-21 — Full rebuild per frame instead of diff; no vsync batching (P2 🟡)
- **Location:** `renderer/lib.rs:28-131` `find_node_at_path, recompute_targets`, `710-1130` `run_window_vnode_skia` full `make_view→apply_styles→compute_layout→recompute_targets→render_frame→present` each `RedrawRequested`; `velox-dom/src/diff.rs:1-198` diff exists but unused at runtime; `velox-core/src/signal.rs:121-291` `flush_queue` synchronous per `set()`.
- **Impact:** Extra recomputes per `set()` (though coalesced via pending redraw, still E2E). Not correctness blocker but will amplify jank when scroll/resize added.
- **Fix (post P0):** Introduce `needs_redraw` flag debounced to `request_redraw` once per event loop; wire `diff` for hit-test retention if perf needed.

### Domain H — Window & Events

#### F-22 — No `MouseWheel` scroll handling; no `scroll` event (P1 🟠)
- **Location:** `renderer/lib.rs:898-930` handles `Resized|ScaleFactorChanged|CursorMoved|MouseInput` only; `event_binding.rs`.
- **Fix:** Add `WindowEvent::MouseWheel { delta, .. } → scroll_by(delta)` path; fire `on_scroll` binding.

#### F-23 — EPIPE degraded path hides real compositor errors (P2 🟡)
- **Location:** `presenter.rs:11-158` `is_compositor_available(WAYLAND/DISPLAY/VELOX_HEADLESS)`, `prepare_backend()` force `WINIT_UNIX_BACKEND=x11` when DISPLAY+WAYLAND, `new`+`present()` EPIPE→degraded no-op; `lib.rs:catch_unwind` headless fallback 800×600 raster.
- **Impact:** CI headless rendering correct, but real compositor edge cases (hybrid Wayland) silently fall back to raster, masking bug where `present` black-screens (photo white square).
- **Fix:** Keep degraded but expose `VELOX_DEBUG_COMPOSITOR=1` diagnostic.

#### F-24 — Missing `resize` / `orientationchange` event for app code (P2 🟡)
- **Location:** No `window.addEventListener('resize')` equivalent.
- **Fix:** Expose `on_resize!()` hook analogous to `on_mounted!` that fires after `RedrawRequested` materialization.

---

## 4. Fixes & Enhancements — Consolidated Catalogue

| # | Fix | Maps to | Effort |
|---|-----|---------|--------|
| **CX-01** | Embed `ua.css` user-agent stylesheet + 3-layer cascade (UA < author < inline) | F-01, F-17 | M |
| **CX-02** | Correct block margin collapse (spec-compliant, negatives, parent-through, empty blocks) | F-02 | M |
| **CX-03** | Rectify flex definite/indefinite propagation + box-sizing for avail fallback | F-04, F-06 | M |
| **CX-04** | Viewport-filling root normalization + `100vw/vh/dvh/%` chain handling | F-05 | S |
| **CX-05** | Responsive `%` basis (margin/padding % = containing_width) | F-07 | S |
| **CX-06** | Unify text metrics: layout uses Skia `measure_text` + snapped font; support `white-space`, `text-overflow` | F-09, F-16 | M |
| **CX-07** | Overflow & scroll model: scrollable detection, `MouseWheel`→offset, clamping, scrollbar paint, `scrollHeight` separation | F-08, F-10, F-12, F-22 | L |
| **CX-08** | Clip-correct hit-testing (`point in clip`) + stacking-context awareness | F-11 | S |
| **CX-09** | Default `box-sizing:border-box` reset + `margin:auto` block centering | F-01, F-14 | S |
| **CX-10** | Inheritable properties expansion (`font-family`, `visibility`, etc.) | F-15 | XS |
| **CX-11** | Selector engine: preserve `>` combinator, scope attr per compound, diagnostic for unsupported pseudos | F-13, F-18, F-19 | M |
| **CX-12** | Shorthand completeness (`background`, `border`, `box-shadow` pass-through) | F-14 | S |
| **CX-13** | Harden compositor/EPIPE diagnostics + `VELOX_DEBUG_COMPOSITOR` | F-23 | XS |
| **CX-14** | Expose `on_resize` lifecycle + window `resize` event to SFC scripts | F-24 | S |
| **CX-15** | Migrate counter boilerplate to `ref!`/`signal!`, lint `Cell` | F-20 | XS |
| **CX-16** | Batch redraw scheduling (coalesce signal flush → single `request_redraw`) | F-21 | S |

---

## 5. Task Breakdown (P0 → P3, deps shown)

| Task | Title | Prio | Deps | Risk | DoD — how verified |
|------|-------|------|------|------|--------------------|
| **T-01** | UA stylesheet + 3-layer cascade + button-reset removal | P0 | — | Low | Visual: fresh `.vx` with raw `<h1><p><div>` looks like Chrome UA (8px body, 0.67em h1, 1em p). Unit: `apply_styles` cascade order test. |
| **T-02** | Viewport root normalization + `vw/vh/%` handling | P0 | T-01 | Low | `cargo test -p velox-dom layout viewport_filling`; manual: delete `width:100%` on `.app`, still fills; resize 800→360→1920 reflows without clip inversion; screenshots 1→2→3 no longer diverge. |
| **T-03** | Block margin collapse (spec) | P0 | T-01 | Med | Unit: collapsed positive/negative/empty/parent-through cases match browser pixel diff ≤0.5px; Photos: card stack gaps uniform, header not clipped. |
| **T-04** | Flex definite/indefinite + box-sizing + % basis | P1 | T-02,T-03 | Med | Layout tests: column `min-height:100vh` with `height:auto` children vs row `width:auto`; `box-sizing:border-box` with `padding`; `%` margin tied to width. Visual: buttons inner padding consistent. |
| **T-05** | Text metrics unification (Skia measure in layout) | P1 | T-04 | Med | `text_wrap` uses `skia_render::measure_text` + `FontCache::snapped_size`; `cargo test -p velox-renderer text`; narrow window “not positive” wraps correctly, count not giant-cropped, white artifact gone. |
| **T-06** | Scrollable overflow model (wheel→offset, clamping, scrollHeight) | P0 | T-04,T-05 | High | `cargo test -p velox-dom overflow scrollable`; manual: small window shows scrollbar, wheel scrolls hidden buttons, `overflow:hidden` stays clipped without scroll, `overflow:auto` appears only when needed. Photos 2/3 now scrollable. |
| **T-07** | Clip-correct hit testing + scrollbar hit pass-through | P1 | T-06 | Low | Unit: point outside `overflow:hidden` parent → `hit_test` miss; e2e: clicking clipped protrusion does nothing. |
| **T-08** | Softbuffer/Skia EPIPE diagnostics + compositor flag | P2 | — | Low | `VELOX_DEBUG_COMPOSITOR=1` logs backend chosen; headless CI still passes; no silent raster fallback. |
| **T-09** | Selector combinator `>` + scoped isolation hardening | P1 | T-01 | Med | `cargo test -p velox-sfc scope_css`; `div > .card` no longer matches descendant; scoped attr on each compound; snapshot tests. |
| **T-10** | Shorthand completeness & `margin:auto` centering | P2 | T-03 | Low | Unit: `margin:0 auto` centers block in viewport; `border:1px solid #fff` parses; `background:#1a1a2e` fallback. |
| **T-11** | Inheritable set expansion | P2 | T-01 | Low | `font-family` on `.app` reaches children; `visibility:hidden` inherited; snapshot. |
| **T-12** | Component/ SFC diagnostics (unknown tag warning) | P3 | T-09 | Low | Build warns `unknown component <Foo>`; `cargo test -p velox-sfc`. |
| **T-13** | Counter boilerplate reactivity + `Cell` lint | P1 | T-01 | Low | Example counter switches to `ref!(0)`, todo computed still passes, dev server HMR shows reactive updates without extra click. |
| **T-14** | Batch redraw scheduling + `on_resize` lifecycle | P2 | T-06 | Med | Multiple `signal.set` within same event → single `request_redraw`; new `on_resize!{}` hook fires after `RedrawRequested` materialization; manual resize log. |

**Parallelization:** Lanes after auditing: `Lane A (layout)` = T-03+T-04+T-05+T-10; `Lane B (render/scroll)` = T-06+T-07+T-08+T-14; `Lane C (style/SFC)` = T-01+T-09+T-11+T-12; `Lane D (examples)` = T-02+T-13. T-02 unblocks T-03 explicitly; T-06 unblocks T-07+T-14.

---

## 6. Non-Goals / Explicitly Deferred

- Full CSS Grid, `position:relative` offset layout (already parses but not positioned in flex), transitions/animations — deferred until parity audit proves layout/scroll stable.
- True Skia GPU path verification (kept raster fallback).
- Shadow DOM or `:deep()` scoping.
- VNode diff-driven incremental rendering — keep full rebuild until T-14 proves coalescing sufficient.

---

## 7. Verification Strategy (per domain)

- **Layout:** Add `velox-dom/tests/layout_golden.rs` goldens vs browser screenshots (Chromium headless `layout_test.html` → `getBoundingClientRect` dump); `cargo test -p velox-dom` with ~30 margin/flex/box-sizing cases.
- **Resize / Responsiveness:** `velox-renderer` integration test (hidden, no compositor): `LogicalViewportTest::resize(1920,1080)→800,600→360,640 logs `layout.rect` + `clip` diff; snapshot.
- **Overflow / Scroll:** Simulate `MouseWheel {delta:120}` → assert `scroll_y` clamped to `scrollHeight - rect_h`; visual harness (Skia raster PNG) diff pixel-perfect for `overflow:hidden` vs `auto`.
- **Typography:** `cargo test -p velox-dom text_wrap_measure` compares `wrap_text` decision boundary to `skia_render::measure_text` within 0.5px.
- **Style / SFC:** `velox-sfc` insta-snapshots for scope rewrite with `>` and pseudo handling; `velox-style` cascade order test (UA < author < inline).
- **Manual:** Re-run boilerplate full-screen/tiny/small (photos 1–3) on 1× and 1.5× scale; confirm header/“not positive” always visible, buttons never half-clipped without scroll, full-screen sections spaced like browser.

---

## 8. Appendix — Evidence Anchors

- `velox-dom/src/layout.rs:142-245` `parse_length_value` (units); `:596-715` box sides; `:843-2259` `compute_layout`; `:1089-1903` flex; `:1910-2141` block; `:2143-2222` clip+rect_h; `:305-470` relative/sticky/absolute → positioning model.
- `velox-dom/src/style.rs:1116-1528` `ComputedStyle` defaults & `set_property`.
- `velox-dom/src/text_wrap.rs:17-74` heuristic; `velox-renderer/src/text.rs` Skia text.
- `velox-renderer/src/viewport.rs:16-178` rounding; `skia_surface.rs:15-161` resize; `presenter.rs:11-158` compositor guard; `lib.rs:710-1128` `run_window_vnode_skia` event loop & coalesced `pending_resize`; `skia_render.rs:18-502` parse/style + `1045-1330` `render_frame`; `events.rs:36-359` collect/hit_test.
- `velox-sfc/src/codegen.rs:250-373` scope id/css; `template_codegen.rs:149-1259` template→VNode + `1418-1493` scope attr; `template_parse.rs:7-194` hand parser.
- `velox-style/src/lib.rs:242-505` selector parse & cascade; `:588-600` button default; `:533-555` inheritable filter.
- `velox-core/src/signal.rs:121-291` effect/flush; `ergonomics.rs:27-90` Ref/Cell; `lifecycle.rs:1-352` mount hooks; `examples/counter/src/App.vx` Cell demo; `veloxc/templates/project/src/App.vx` root `.app` flex contract.

---

## 9. Recommendation

Merge blocker fixes T-01…T-06 first (audit P0s) into a `fix/html-parity-phase1` branch, gate on goldens + manual photo repro, then ship T-07…T-14. Estimated phase-1 effort: 3–5 engineer-days. Do not ship incremental style tweaks piecemeal — they interact (UA margins affect collapse; collapse affects content-height; content-height gates scroll model).

---

## 10. Addendum — Findings discovered during implementation (2026-09-25)

Sections 1–9 above are the pre-implementation audit. The fixes below were
implemented and reviewed, and doing the work surfaced a further class of
defects that the original static audit could not see. They are recorded here
because they are part of the same HTML/CSS-parity contract the user is asking
for, and because several are still open.

### 10.1 Resolved

| ID | Finding | Resolution |
|----|---------|------------|
| F-25 | **UA stylesheet was never applied in production.** `velox-style::apply_styles` is author-sheet-only, and every production render loop called it. Browser UA defaults (h1–h6/p/blockquote/list margins, control font inheritance) were absent from real windows. The rewritten examples only looked correct because they declared every style explicitly — the exact workaround HTML/CSS would not require. | New `style_vnode_with_hover` in `velox-renderer/src/lib.rs:12-17` delegates to `apply_with_cascade_with_hover`; all production call sites converted. Regression test `velox-renderer/tests/ua_cascade_render.rs` asserts UA / author / inline margins at three layers. |
| F-26 | **Component-bound attribute expressions rendered as `String::new()`.** `make_resolve` registered only `{{ }}` interpolation keys, so `:value="draft"` on a component emitted `resolve("draft")` against a map that never contained it. | Shared `emit_bind_attr` normalization + `collect_resolver_keys`; unresolvable bindings now diagnose. 4 review rounds. |
| F-27 | **The `velox init` template shipped a stale duplicate of `examples/`.** Two copies of the same components drifted, and the template carried an inert `@submit` binding. | `init.rs` now uses compile-time `include_str!` of `veloxc/templates/project/...`, so a distributed binary still embeds the assets while there is one source of truth. Test `velox-cli/tests/build_tests.rs` rejects `@submit` in generated output. |
| F-28 | **Resize-hook size dedup was per-thread, not per-window**, so one window's committed size could suppress another's hooks. | Core dedup removed; each render loop owns a `ResizeState`. Accepted tradeoff: teardown remains global (see 10.3). |
| F-29 | **`Cell`/`RefCell` lint panicked on non-ASCII input** and reported only the first occurrence per line. | Byte-boundary-safe scanning; per-occurrence reporting. |
| F-30 | **`min-width` was a silent no-op on every block-flow box.** The style layer parsed it and stored it in `ComputedStyle`, but `velox-dom/src/layout.rs` never reads `ComputedStyle` — it reads the style *string* — so no block box was ever floored. Only the flex main axis clamped a minimum, and only via its own resolve. | `used_min_width` + `floor_to_min_width` in `velox-dom/src/layout.rs`, reading through the task-2.1c `style_lookup_len_full` memo (no second cache). Applied *outside* `cap_to_max_width` so `min-width` wins over `max-width` (css-sizing-3 §3.1). Negative values dropped as invalid (CSS 2.1 §10.4); `auto`/absent = no constraint, never `0`; `0` kept as an explicit opt-out. Tripwire `block_min_width_overrides_max_width` in `velox-dom/tests/maxwidth_absolute.rs`. **Block-flow only — the flex resolve is untouched and still has a base-clamp divergence** (see DEFERRED in the 4.8a report). |

### 10.2 Open — diagnosed, specified, not yet implemented

**G-1 — `:key` generates uncompilable code in Resolve mode.** `CLAUDE.md`
documents `:key` as supported list syntax and the parser accepts it, but the
value is interpolated twice: `velox-sfc/src/template_codegen.rs:1711-1719`
applies `rewrite_if_expr(key_val)` to a value the attribute pass has already
rewritten. State mode is correct. Spec: `task-15-key-directive-brief.md`.
**Scope limit to carry forward:** this makes `:key` compile and set the key. It
does **not** implement key-based list reconciliation or reordering, and must not
be reported as if it did.

**G-2 (partial) — flex item descendants were laid out at the origin.** The flex
child collection (`velox-dom/src/layout.rs:1769-1784`) lays out an item and all
its descendants in origin-local coordinates, then the placement passes mutated
only the item's own rect. Resolved by recursively translating the subtree before
relative/sticky are applied. **Residual limitation:** flex sizing can still
overwrite the pre-laid item width/height (`:2180-2189`, `:2246-2259`,
`:2279-2288` for `flex-grow` and `align-self: stretch`), so descendants may wrap
against pre-flex dimensions. Coordinates are correct; intrinsic sizing is not
yet.

**G-3 — collapsible whitespace between block-level boxes creates a phantom
line box.** The whitespace-only guard at `velox-dom/src/layout.rs:1464-1471` is
**flex-only**; the block child loop has no equivalent, and
`velox-sfc/src/template_parse.rs:261-278` normalizes every whitespace run to a
`" "` text node while cleanup at `:298-302` only filters the template root. The
empty fallback line in `velox-dom/src/text_wrap.rs:263-265` then gives it a full
~19px line height, pushing the following block down. The correct fix is
formatting-context-aware suppression — **not** a blanket skip, which would break
`<span>a</span> <span>b</span>`, and **not** parser cleanup, because the parser
has no computed `display` information. A real inline formatting context is larger
than this one fix; `velox-dom/src/layout.rs:1358-1361` still routes everything
except `display:none`/`display:flex` into block flow.

**G-4 — four properties parse but do nothing.** `max-width`, `float`,
`inline-block` and `position:absolute` are accepted by the style layer and
ignored by layout. `position:absolute` in particular has a half-built path at
`velox-dom/src/layout.rs:305-470`. This is a substantial slice, not a patch.

**G-5 — root-level `v-if` and `:class` bindings are inert.** `make_resolve`
recognizes only interpolation keys, so a `v-if` on the root element never
toggles, and a bound `:class` replaces the static class list rather than adding
to it — the opposite of Vue's class-merging semantics.

### 10.3 Known defect, accepted with a recorded tradeoff

**Resize-hook teardown is global, not per-window.** `run_all_destroy_hooks`
clears the entire thread-local registry, so tearing down one window can clear a
second window's hooks on the same thread. This was deliberately chosen over the
alternative: the opposite bug (leaking destroyed components' hooks into a
surviving window) was introduced during implementation and **does** occur on an
in-tree path — HMR full reload — whereas simultaneous same-thread Skia windows
have no in-tree caller, since every entry point blocks in one `event_loop.run`.
A true per-window registry is queued as its own task.

### 10.4 Verification trap worth knowing about

`render_vnode_to_raster_png` is a naive rasterizer that does **not** run
`compute_layout`. The two APIs that do — `render_vnode_to_rgba` and
`render_vnode_to_raster_png_with_scale` — both run the real layout pipeline. A
visual proof built on the wrong one will pass while the layout is broken, which
is precisely how the first examples proof suite missed F-26. All example proofs
now use the layout-backed APIs.


