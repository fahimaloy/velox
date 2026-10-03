# Scaffolded-app defects: root causes, plan, and tasks

> **Status:** recon complete for all 9 tasks. **T1 and T4 dispatched** (`fix-28`, `fix-29`).
> Nothing implemented or committed yet — the orchestrator commits centrally.
> **Date:** 2026-10-02 · **Branch:** `feat/premium-boilerplate-core-components` · **HEAD at recon:** `1bb8eb1`

## How this was found

Not by reading code — by **running `velox init` + `velox dev` and looking at the window**. The
scaffolded app from `velox-cli/templates/project/` renders visibly wrong. Five recon lanes then
traced each symptom to source, and **every load-bearing claim was re-verified by hand at source
before being recorded here.** Where a claim could not be verified, this document says so.

Four recon premises were **false** and are corrected below (§6). The pattern is the familiar one:
a cost/absence claim asserted without a measurement or a source citation.

---

## 1. The four symptoms, and what actually causes them

| Symptom | Real root cause | Not the cause |
|---|---|---|
| Dark mode: page background stays light, buttons go dark, title unreadable | **T1** — `scope_css` corrupts any rule preceded by a CSS comment | specificity, ordering, `:class` binding |
| Theme-toggle / check glyphs render as ▯ (tofu) | **T5** — the face velox loads has no `☀`/`☾`/`✓`, and there is no glyph fallback | the characters, the encoding |
| Delete `×` not centred in its button | **T4** — a layout "button hack" throws away `justify-content: center` | the template's flexbox |
| Add button overlaps the input | **T2** — flex placement resizes an item but never re-lays-out its children | `min-width: 0`, `gap`, `box-sizing` |
| Subtitle line rendered twice | **T3** — every wrapped line gets its own `LayoutNode` with the *same* `source_index`; the renderer paints line 0 at each | duplicated source, codegen |
| 4 `unused_braces` warnings from generated code | **T8b** — `template_codegen.rs:2848` wraps every `v-if` child in a superfluous block | the `let __props` block (that inner brace is required) |
| "App started (HMR enabled)" after an HMR bind failure | **T8a** — the bind happens on a thread with **no channel back** to the printer, so `dev_current` structurally cannot know | ordering, timing, the build |

**Three of these are not template problems at all.** They are engine defects that would bite every
velox application, not just this one. The template merely made them visible.

---

## 2. Root causes in detail

### T1 — `scope_css` is comment-blind → the dark-mode failure

`velox-sfc/src/codegen.rs:307-368`. `scope_css` accumulates a selector prelude character by character
until it meets `{`, and special-cases exactly four things: a prelude starting with `@`, keyframe
containers, an empty prelude, and a nested selector inside a declaration block. **It does not
recognise `/* … */`.**

So a comment sitting between two rules is glued onto the front of the **following** rule's prelude.
`scope_single_selector` (`codegen.rs:388-414`) then splits that prelude on whitespace and appends
`[data-v-<scope>]` to **every token** — including the comment's own closing `*/`:

```
authored:  .dark .app
emitted:   [data-v-APP] .dark[data-v-APP] .app[data-v-APP]
        ↑ an extra, scope-tagged, attribute-only leading compound
```

That is a **three-part descendant selector** requiring a scope-tagged ancestor *strictly above* the
`.dark` carrier. The carrier is the root element, whose ancestor list is empty
(`velox-style/src/lib.rs:898`), so `match_prefix` (`lib.rs:662-694`) computes a candidate range of
`1..1` — empty — and can never satisfy it. **The rule can never match.**

Every dark rule immediately following a comment is dead. `velox-cli/templates/project/src/App.vx:398`
carries exactly such a comment, so `.dark .app` — the page background — never applies. All the dark
rules with a clean prelude *do* apply. That is precisely the reported split: dark buttons, light page.

**Blast radius is not the template.** 18 rules across all six components are corrupted (listed in
`docs/tasks/T1-scope-css-comments.md`), and **any velox app whose scoped CSS contains a comment gets
the same corruption.** This is a general correctness bug in a documented, user-facing feature
(`<style scoped>`).

### T2 — flex placement resizes an item but never re-lays-out its subtree

`velox-dom/src/layout.rs`. Every flex child is pre-laid-out once at the container's full main size
(`layout.rs:4153`). The basis ladder (`:4380-4447`) then computes `flex-basis`, grow and shrink write
`target_main_size`, and the placement pass writes it:

```rust
// layout.rs:4898-4902
let fb = items[item_idx].target_main_size.round() as i32;
if is_column { ln.rect.h = fb } else { ln.rect.w = fb }
// ...
// layout.rs:4976-4980
translate_layout_subtree(&mut ln, resolved_x - pre_x, resolved_y - pre_y);
```

`translate_layout_subtree` (`layout.rs:1874-1888`) adds `dx`/`dy` to every rect and clip in the
subtree. **It never re-measures.** There is no `at()` call anywhere in the placement region
(`layout.rs:4880-5120`).

So when a flex item's *final* main size differs from the width it was *measured* at, its children keep
the old width and simply move. In the template, `.field` is measured at 556 px, shrinks to 480 px, and
its child `.input` — measured at `width: 100%` = 556 px — is never told. It overhangs by 84 px, which
is the Add button's 68 px plus the 8 px `gap`. Paint order is DOM order, so `.add` paints on top.

**The overhang equals the Add button's width plus the gap, at every window width.** It is structural,
not a narrow-window bug.

Three things that look like fixes and are **not**:

- **`min-width: 0` on `.field`/`.input` is inert.** Velox implements no automatic minimum size at all;
  `parse_length_value` returns `None` for `auto` outright (`layout.rs:2468-2470`), so a flex item's
  minimum main size is unconstrained unless a literal `min-width` is declared. There is no
  content-based floor to defeat. (An earlier review claimed `grep -c min_width layout.rs` returns `0`;
  it returns **8** — `used_min_width` `:1552`, `floor_to_min_width` `:1582`, call sites `:3587`,
  `:4457`. A later review's `:5364-5387` citation is the `min-height` block, not `min-width`.)
- **`gap` is fully supported** (`layout.rs:3801-3840`, consumed `:4542`/`:4597`/`:4691`/`:4812`) and is
  working. `gap: 8px` is not the cause.
- **Removing the `.field` wrapper would work** but is not available: the wrapper is load-bearing, so
  that `.dark .input` has a *strict ancestor* carrying `TodoInput`'s own scope id (documented at
  `TodoInput.vx:18-22` and `App.vx:62-69`).

### T3 — every wrapped line gets its own `LayoutNode` sharing one `source_index`

**This one silently deletes text, which is why it is the highest-priority item.**

`velox-dom/src/layout.rs:1104` opens a loop over line boxes and declares `merged` **inside** it
(`layout.rs:1197`). The merge that collapses pieces of one text `VNode` into one node therefore fires
**only within a single line** (`layout.rs:1240-1250`) — and the comment there already names the exact
hazard:

```rust
// Consecutive pieces of one text VNode are ONE LayoutNode. Two
// nodes pointing at the same text VNode would make the renderer
// draw the same string twice.
```

Each line then appends its own nodes unconditionally (`layout.rs:1292-1295`), and
`inline_leaf_node` (`layout.rs:370-385`) stamps each with the **same** `source_index`.

The renderer resolves the `VNode` by that index, one call per layout child, with no dedup
(`velox-renderer/src/skia_render.rs:2695-2714`). It then re-wraps the **whole** original string
(`skia_render.rs:2774-2777`) and breaks after line 0, because `text_bottom = rect.y + max(rect.h,
line_height)` (`skia_render.rs:2788-2794`) and a per-line rect is only one line tall.

**Net effect for an N-line text node:** line 0 is painted N times, once per `LayoutNode`, each one
line-height lower — and lines 1 through N-1 are **never painted at all**.

For `.tagline` that means the visible duplication *and* the loss of the tail `"natively with Skia."`.
The same shape affects `Modal.vx:12 .body-text`, `Confirm.vx:9 .message`, and `TodoItem.vx:7
.todo-text` for any long todo.

Why no test caught it: `velox-renderer/tests/text_decoration_render.rs:279-316` asserts one
decorated band per wrapped line, and it **passes under the bug** — every invocation paints `lines[0]`
and measures `lines[0]`'s own width, so the band geometry looks right while the glyphs are wrong.
`skia_text_wrap_render.rs:8` is a checksum that was simply recorded from the buggy output.

### T4 — the button hack discards flex centring

`velox-dom/src/layout.rs:5389-5407`:

```rust
if tag == "button" && children.len() == 1 && let Some(child) = laid_children.get_mut(0) {
    // ... vertical centring ...
    let align = style_lookup_str(style, "text-align").unwrap_or_else(|| "left".to_string());
    let offset_x = match align.as_str() {
        "center" => ((content_w - child_w).max(0)) / 2,
        "right"  =>  (content_w - child_w).max(0),
        _ => 0,
    };
    child.rect.x = content_x + offset_x;
}
```

The flex pass computed the correct horizontal centring at `layout.rs:4722` (`"center" => main_start +
extra_space / 2.0`). This block then **unconditionally discards it**, replacing it with a value derived
from `text-align` alone. With no `text-align` — the default — `offset_x = 0` and the glyph is pinned
to the content box's left edge. On the 26 px `.remove` button that is **≈11 px left of centre**.

Three corroborating facts pin the mechanism exactly:

1. `.ghost` (`App.vx:374-385`) sets **both** `justify-content: center` **and** `text-align: center`, so
   it renders centred.
2. `.toggle` and `.check` have **three** children, because the SFC leaves template indentation as
   whitespace text nodes. `children.len() == 1` is false, the hack is skipped, and their flex centring
   survives. That is exactly why only the single-line `×` is wrong.
3. This runs on the flex path too — the flex branch (`layout.rs:3791`) falls through to this shared tail.

`docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md:146-147` already flags it. It has been live for
a year.

### T5 — the loaded face has no `☀`/`☾`/`✓`, there is no glyph fallback, and the bundled fallbacks are stubs

`velox-renderer/src/skia_render.rs:1980-2009` tries six absolute system paths, then two
`include_bytes!` bundles.

**The two bundled fonts are 14-byte text stubs.** `velox-renderer/assets/DejaVuSans.ttf` and
`NotoSans-Regular.ttf` both contain the literal ASCII `<BINARY FILE>\n`. `FontMgr::new_from_data` on
that always returns `None`, so the entire `bundles` loop at `skia_render.rs:2005-2009` is **dead
code**.

On this machine the winner is `/usr/share/fonts/google-noto/NotoSans-Regular.ttf` (candidate #4). The
first three candidates do not exist here — the real DejaVu lives at
`/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf`, which is **not in the candidate list**. The
`symbols` family is never consulted at all.

Reading the real cmaps:

| face | `☀` U+2600 | `☾` U+263E | `×` U+00D7 | `✓` U+2713 |
|---|---|---|---|---|
| `NotoSans-Regular.ttf` (wins here) | **missing** | **missing** | present | **missing** |
| real `DejaVuSans.ttf` (not on the list) | present | present | present | present |

`×` is Latin-1 and is in every candidate; the three symbols are not. There is no `Paragraph`, no
`Shaper`, no `SkUnicode`, no glyph-run building anywhere in the workspace — the only text draw is
`canvas.draw_str` (`skia_render.rs:2811`), which forwards to `drawSimpleText`: single typeface, no
shaping, **no fallback**. Unmapped codepoints become glyph 0 = `.notdef` = a hollow rectangle, which
is the ▯ in the screenshot. (Noto Sans's `.notdef` has ink only above the baseline, so the tofu also
lands ~10 px low in the button.)

**This is environment-dependent**: on a machine where the real DejaVu path exists, `☀`/`☾` render
correctly from the same binary.

Two compounding defects:

- `get_or_load_family` (`skia_render.rs:1651-1661`) permanently writes the default face under any
  requested family name on a miss, and `new_with_scale` seeds exactly one entry (`"default"`,
  `:1566-1570`). Combined with `parse_font_family` (`skia_render.rs:512-520`), which keeps only the
  first comma-separated name, **`font-family` is completely inert**.
- `GenericFamily` (`velox-style/src/fonts.rs:217-265`) has **zero consumers** outside its own parser
  and one `pub use` re-export at `velox-style/src/lib.rs:18`.

### T6 — the dark palette has no figure/ground and its borders fail WCAG 1.4.11

Measured by des-1. **The text palette is good** — primary 14.57:1, secondary 7.64:1, Add label
10.26:1, all AAA. The failure is entirely in the **surfaces**:

| pair | ratio | verdict |
|---|---|---|
| card `#161b1f` vs page `#0f1316` | **1.08:1** | no figure/ground at all |
| border `#242c31` on card | **1.22:1** | fails 1.4.11 (needs 3:1) |
| border `#242c31` on page | **1.32:1** | fails 1.4.11 |
| border `#333d45` on card | **1.56:1** | fails 1.4.11 |
| muted text `#75818a` on card | **4.35:1** | **fails AA** (4 places) |
| dialog `#181e22` vs page `#0f1316` | 1.11:1 | invisible |
| hover `#1d2429` on card | 1.10:1 | hover is invisible |

The four AA failures are all one token: `#75818a`, used by `.dark .remove`, `.dark .completed
.todo-text`, `.dark .input::placeholder` and `.dark .empty`.

It is also **internally inconsistent**: five surface values for three roles, four border values for
two roles, and the *same* ghost-button hover uses `#333d45` + `#1d2429` in `App.vx:437-438` but
`#414c55` + `#222a2f` in `Modal.vx:325-326` / `Confirm.vx:269-270`. Light mode has **one** border and
**one** hover border across all six files — dark mode is where it rots.

### T7 — stale and false comments, and a stale test

- `App.vx:78-85` asserts `☀`/`☾` are present in "the only two faces the renderer can load" — **false**
  for Noto Sans, which is the one that wins.
- `TodoItem.vx:48-50` makes the same assertion for `✓` — **false**.
- `App.vx:812` cites `apply_styles_with_hover`; the function starts at `velox-style/src/lib.rs:786`
  and the cascade loop is at `:840-841`.
- `velox-cli/tests/template_scope_coverage.rs` is **stale**: it asserts `.btn-add` in `TodoInput.vx`
  and `.todo-item.completed .todo-text`, both of which no longer exist. This is the failure an earlier
  gate run recorded and I mis-diagnosed as "contaminated by a concurrent edit" — it is a real
  staleness failure.
- `velox-sfc/tests/scope_combinator.rs` **never feeds a comment to `scope_css`**. That is precisely the
  coverage gap that let T1 ship.

### T9 — flex automatic minimum size is absent (found en route; not the overlap's cause)

css-flexbox-1 §4.5: a flex item's `min-width: auto` resolves to its content-based minimum size. Velox
has no such rule — `parse_length_value` returns `None` for `auto` (`layout.rs:2468-2470`). The
consequence is that a long unbreakable word cannot push a flex row wider than its container's idea of
the available space, and no content-based floor protects anything. Real spec gap; lower priority
because T2 is what actually broke the composer.

---

## 3. What is *not* wrong

Worth recording so a later session does not re-investigate:

- `:class="{ dark: is_dark }"` **works** — fully supported, with pixel-level behavioural proof at
  `velox-sfc/tests/root_vif_class_behaviour.rs:928-977`.
- Cascade ordering is correct: the dark block *is* last in the file, and there is no specificity, so
  order is the only mechanism and it is being used correctly.
- `gap` works.
- `box-sizing: border-box` is set globally (`velox-style/src/ua.css:1`, `*` is a supported selector)
  and the UA sheet runs under the author sheet (`velox-renderer/src/lib.rs:15`), so content-box
  overflow is not in play.
- The text palette and the light palette are largely sound.
- `min-width` **is** implemented (8 matches in `layout.rs`) — the original plan's "returns 0" claim
  was wrong, though its conclusion (no automatic minimum size) is right.

---

## 4. Sequencing

Three of the four engine defects touch `velox-dom/src/layout.rs`, and cargo's build lock serialises
everything anyway. **There is at most one writer per file, and never two lanes on `layout.rs`.**

```
Wave 1 (parallel, disjoint files)
  T1  velox-sfc      scope_css comment blindness      LOW risk, biggest visual win
  T5  velox-renderer font loading + real font assets  independent of layout
  T7  templates      false comments, stale test       trivial
  D3  design pass    T3's layout↔paint contract        read-only, no cargo

Wave 2 (serial, all on velox-dom/src/layout.rs, one lane, two commits)
  T4  button hack honours justify-content             small + contained, commit first
  T2  flex placement re-lays-out at the resolved size HIGH risk — the flex hot path

Wave 3 (after D3's answer, plus T8)
  T3  text duplication + silent text loss             HIGHEST risk — text loss, not cosmetic
  T8  velox dev diagnostics                           velox-cli only
  T6  palette                                         code may land any time; CANNOT be
                                                      visually verified until T1 lands
  T9  flex automatic minimum size                      lower priority
```

**T6 is verification-gated, not code-gated.** The corrected dark palette can be written before T1
ships, but no one can *see* it until the page background stops being permanently light. Do not treat
"palette applied" as verified until a screenshot shows it.

---

## 5. Tasks

Each has a task file with root cause, verification recipe, falsification requirements and gate:

| id | title | scope | risk |
|---|---|---|---|
| T1 | `scope_css` must skip CSS comments | `velox-sfc/**` | low |
| T2 | flex placement must re-lay-out a resized item's subtree | `velox-dom/**` | **high** |
| T3 | one text `VNode` must not be painted N times | `velox-dom/**`, `velox-renderer/**` | **highest** |
| T4 | the button hack must not discard `justify-content` | `velox-dom/**` | low |
| T5 | ship real font assets + a correct candidate list + glyph fallback | `velox-renderer/**`, `velox-style/**` | medium |
| T6 | rebuild the dark surface/border ramp to WCAG | `velox-cli/templates/**` | low |
| T7 | delete the false comments; un-stale `template_scope_coverage.rs` | `velox-cli/**`, `velox-sfc/tests/**` | trivial |
| T8 | codegen braces + the HMR-enabled lie | `velox-cli/**`, `velox-sfc/**` | low |
| T9 | flex automatic minimum size | `velox-dom/**` | medium |

Files: `docs/tasks/T1-*.md` … `T9-*.md`.

---

## 6. False premises found during recon

Recorded because they are the reason the fixes must be gated on measurement rather than on review
confidence.

1. **"`:class` object binding does not work."** False — fully supported, with pixel proof.
2. **"The cascade has specificity problems here."** False — there is no specificity at all and the
   ordering is already correct.
3. **"`.add` overlaps the input because `min-width: 0` is missing."** False — `min-width` is
   implemented (8 matches); it is the *automatic* minimum size that is absent, and that is not what
   overlaps.
4. **"`gap` is unsupported, which is why the button overlaps."** False — fully supported and working.
5. **"The subtitle is duplicated in the source."** False — exactly one occurrence; it is generated
   twice at paint time.
6. **"Codegen emits the child twice."** False — one `h()` call, one `Vec<VNode>`
   (`template_codegen.rs:2340-2346`, `velox-dom/src/lib.rs:56`).
7. **"There is a `div`/`p` sibling to the button hack."** False — `tag == "button"` at
   `layout.rs:5389` is the only `tag ==` test in the file.
8. **"The font assets are real TTFs."** False — both are 14-byte `<BINARY FILE>` stubs, so the whole
   bundled-fallback loop is dead code.
9. **"`grep -c min_width layout.rs` returns 0."** False — it returns 8.
10. **"`template_scope_coverage.rs` failed because the tree changed mid-gate."** False — it is a real
    staleness failure, correctly reporting assertions about selectors that no longer exist.

---

## 7. Standing rules for the implementing lanes

- **Falsify every new test.** A mutation that does not turn it RED was not a test.
- **A test that cannot fail is not a test.** Two prior recon sweeps were void because a script bug
  dropped trailing newlines, so no mutation compiled and "did not compile" was miscounted as "not
  caught". Check that the mutation *compiles* before recording the result.
- **Re-grep line numbers at dispatch.** Every citation in this document was verified when written and
  will drift.
- **Never `git add -A` / `.`** — stage by explicit filename.
- **One writer per file.** Never two lanes on `layout.rs`.
- **At most 2 concurrent cargo builds**, `-j 3` each. A prior run drove 12 `rust-lld` processes on
  15 GB RAM and died with `collect2: fatal error: ld terminated with signal 7 [Bus error]` — that is
  disk exhaustion, not a code failure.
- **Never `cargo clean`.** `/home` was at 97%; a full clean is not affordable.
- Gate logs go in `.superpowers/sdd/…/gates/`, **not** `/tmp`.
---

# Part 2 — Images: SVG and raster (image handling parity + boilerplate examples)

**GATING ORDER — this part does NOT start until Part 1 (T1–T9) is complete AND fully
staged+committed. No exceptions.**

### 2.1 Make SVG renderable in velox, with related functionality well implemented
- SVG must be usable as an `<img>` source (and any other place velox loads images).
- All SVG-adjacent behaviour must work: intrinsic width/height, `viewBox` scaling,
  aspect-ratio preservation, sizing via CSS `width`/`height`.

### 2.2 Other images must be usable properly
- PNG/JPEG/GIF/WebP etc. load and paint correctly, at any `src` form velox claims to support.

### 2.3 Image defaults must match HTML/CSS (Vue JS) defaults exactly
- Every image-related default style must equal the HTML/CSS UA default: `display: inline` for
  `<img>`, no default border, `vertical-align: baseline`, intrinsic-size-driven box, CSS
  `width`/`height` overriding attributes, `object-fit` behaviour, `alt` rendering when the
  resource fails, and so on. Every related functionality must behave the same as HTML/CSS/Vue.
- Deviations, if any, must be deliberate, documented in `docs/HTML_PARITY.md`, and match the
  "deviations must be written down" rule from the remediation plan.

### 2.4 The `velox init` boilerplate must demonstrate both image kinds, designed well
- The scaffolded app must use the **velox SVG logo** with appropriate styling and placement
  following best design practice.
- The scaffolded app must also use an appropriate **regular PNG** somewhere appropriate.
- Both image-usage examples must be present in the inited code itself, so the boilerplate shows
  `<img>` with both an SVG and a raster source, styled and positioned deliberately.

### Sequencing for Part 2
1. **All of Part 1 (T1–T9) implemented and its changes staged + committed.**
2. Then audit the current image implementation: what `<img>` supports, whether SVG works at all,
   what the default computed styles for `<img>` are, how `width`/`height`/attributes interact.
3. Findings → plan (this section fills in with concrete tasks T10+).
4. Implement + verify continuously.
5. Finally wire the SVG logo and a PNG into `velox-cli`'s `init.rs` scaffold, with best UI design.

**Do not begin Part 2 until Part 1 is done.**
