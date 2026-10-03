# Round 2 — Manual-test findings, design quality, and image support

**Status:** in progress · **Branch:** `feat/premium-boilerplate-core-components` · **Base:** `ceb14d3`
**Predecessor:** `docs/plans/2026-10-02-scaffolded-app-defects.md` (T1–T9, all landed)

## Ground truth from the manual test

The user inspected the running app (`tmux session veloxdev`, log
`/tmp/velox-devlog/velox-dev.log`) and reported: "mostly all are okay, but…"

1. Theme-toggle icon not centred when the theme is dark.
2. Spacing between sections and text blocks is wrong.
3. **Clear completed and About do nothing on click.** Both should also carry an icon before the label.
4. No image example: wants a velox SVG logo in the main UI and a PNG version in the About modal,
   implemented in the scaffolded code so a new user has a working reference.
5. Colours and typography need to be stable and GNOME/GTK-class.

## What is already ruled out for #3

- Template markup is correct (`App.vx:20-21`).
- Codegen emits the prop: `.set("on:click", "open_clear")` in the generated `app.rs`.
- The dispatch table emits `"open_clear" => { state.open_clear(); }` and
  `"open_about" => { state.open_about(); }`.
- `App.vx:204` `open_clear` sets `clear_open` to `true`; `App.vx:246` `open_about` sets `about_open`.

So the break is **not** codegen, **not** the handler, **not** `v-if`. The theme toggle
(`App.vx:11`, same `@click` mechanism, different parent) works, so it is the control case.

Leading hypothesis under investigation: hit-testing resolves the click to the wrong node because
`.meta` is a flex row whose children were re-laid-out. Two recent commits changed exactly that path —
`8a14e40` (flex subtree re-layout at the resolved main size, with a cross-size write-back) and
`ceb14d3` (flex automatic minimum size + the freeze-and-redistribute resolve loop). Either can leave a
child's `rect` stale relative to where it is painted, or a rect whose `source_index` points at the
wrong VNode; `.meta-fill` is a `flex:1` spacer occupying the row's middle.

## Blocking defect found while setting up the test (not in any task)

`velox init` stamps the **build-time commit** into the scaffolded `Cargo.toml`
(`velox-cli/src/commands/init.rs:320` → `crate::velox_git_rev()`). Building the CLI from a local
branch ahead of the remote produces an app that **cannot resolve its own dependencies**:
`error: failed to get 'velox-core' as a dependency`. Every app scaffolded from a locally built binary
is unbuildable. Worked around by hand for this test (path deps + `.cargo/config.toml` reusing the
repo's target dir). Needs its own fix: detect a rev that is not on the remote and fall back to path
deps, or pin to the merge-base.

## Lanes

| Lane | Agent | Scope | State |
|---|---|---|---|
| exp-22 | explorer | root cause of the dead `.ghost` clicks (hit-test vs layout rect) | running |
| exp-23 | explorer | image capability matrix — `<img>`, PNG decode, SVG, asset resolution, dev-time serving | running |
| des-2 | designer | GNOME/libadwaita-class design system: type scale, spacing rhythm, surface levels, semantic colours, button icons, glyph centring | running |

## Landed

**Image support, part 1 — `<img>` is a replaced element (`velox-dom`, no new dependency).**
A second callback seam beside the one layout already owns (`text_wrap::set_skia_measurer`), same shape
and locking:

```rust
pub type IntrinsicSizeProbe = fn(src: &str) -> Option<(i32, i32)>;
pub fn set_intrinsic_size_probe(f: IntrinsicSizeProbe);   // renderer registers at init
pub fn is_replaced_element(node: &VNode) -> bool;
```

A function pointer, not a size table, so the renderer answers from the image cache it is already
building — no second copy of the sizes in `velox-dom`, nothing to invalidate on eviction.
`compute_layout(node, w, h)` is untouched, so no renderer/CLI/SFC call site changes shape and there is
no breaking API change.

Sizing precedence, decided in one place (`replaced_used_size`), per axis:
**CSS declaration → HTML `width`/`height` attribute → intrinsic px → 0×0.** CSS beats the attribute
because the HTML spec maps those attributes to presentational-hint declarations that any author
declaration overrides. An attribute applies only in HTML's grammar (`<dimension>`/`<percentage>`), so
`width="2em"` falls through to intrinsic, as a browser does. Unknown size is `0×0`, **never "fill"** —
`content_size_for` answers `avail` for an undeclared block, and a replaced element must not inherit
that (it would be a full-page clickable region around nothing).

Presence of a `src` **attribute** makes a box replaced, tag-agnostic. `script`/`link`/`source`/`track`
carry `src` and are `display:none` in a browser; velox has no such UA sheet, so they now get a zero-size
box where they previously got a full-container-width block. Nothing is painted or hit-tested either way.
Pinned by test so the decision is auditable.

New tests: `velox-dom/tests/replaced_img_layout.rs` (16) and
`velox-dom/tests/replaced_img_no_backend.rs` (2, in its own binary because the probe is a process global
with no unregister — the same constraint `tests/common/mod.rs` already documents for the text measurer).
`velox-dom` is 356 passing / 0 failing across 32 binaries. Four mutations, all confirmed RED and
reverted — including `lay_out_atomic`'s replaced branch, whose absence made `width="25%"` resolve a
quarter of **zero**. Cost measured A/B on a 2000-element tree, release, best-of-5 × 20 passes: the added
`HashMap::contains_key("src")` per element per pass is **not separately measurable at the probe's noise
floor** (ranges overlap, "with" slightly lower).

**Design system pass (`40ebaed`, six `.vx` files + the WCAG test + a render-proof harness).**
Type scale reduced from `{11,12,13,14,15,16,17,27}` — which had two 15s at different line-heights and a
27px display on nobody's ladder — to `{11,12,13,15,16,20,28}`, drawn from libadwaita's family. Spacing
moved onto `{8,12,16,24,32,40}` from an ad-hoc `{1,7,8,10,11,12,14,16,18,20,22,24,26,32,44}`. **The actual
spacing defect was that the `.rule` sat flush against the composer (gap 0); it is now 24.** Radius:
8 control, 12 card, 16 panel, 999 pill/circle.

**Elevation is fill + hairline only** — `box-shadow` has no consumer in this framework, so card-on-page
separation is 1.09:1 by design. Three darker page fills were measured and rejected: they buy 1.13–1.15
separation but push muted text below 4.5:1 and drop the control border to 2.98:1, failing 3:1.

Five colour tokens moved, each with its measured ratio: text-secondary 6.07 → 8.25 on white; text-muted
5.07 → 5.68 on white and **4.33 → 4.75 on inset (the old value was genuinely failing AA)**; decorative
border 1.479 → 1.382; divider 1.199 → 1.283; dark decorative 1.675 → 1.534.

**Theme-toggle centring: the box was always centred; the defect was ink inside the advance.** DejaVu's
U+2600 sits 4.5px right of its advance centre and U+263E 0.5px left, so dark mode was 2.0px off. `margin`
on the glyph is dropped by layout for a lone button child (`velox-dom/src/layout.rs:5922` places a lone
child at `content_x + (content_w − child_w)/2` and never reads the child's margin) — proved by setting it
to 24px and re-measuring unchanged. The only lever that is read is the button's own padding, so
`.dark .toggle { padding: 0 4px 0 0 }` and nothing else. Measured after: light 0.5px, dark 0.0px.

Icons added: `⌫` U+232B (Clear completed — a cross would read as a second delete beside the row `×`),
`ℹ` U+2139 (About), `+` U+002B (Add). `.btn-label { white-space: nowrap }` is load-bearing: a `<button>`
only lays out **element** children as flex items, so a bare text label would not sit beside its icon.
Five candidate glyphs were rejected as tofu after checking the cmap rather than shipping them.

Glyph coverage is now enforced permanently: `the_shipped_face_carries_every_glyph_the_template_uses`
dumps every non-ASCII codepoint out of the six templates and asserts each against DejaVu (5955
codepoints), the face `load_default_typeface` actually prefers. Noto Sans (2965) is the fallback and
lacks several of them.

## Two framework bugs found by the design pass

> **B1 — RESOLVED, and it was the whole of the user's "buttons do nothing".** Four recon agents failed
> on this question; it was traced by hand, then **proved with printed evidence**.
>
> The click path was never broken. `collect_click_targets` collects all three handlers with correct rects
> and a centre-click resolves to the right one — `open_clear` rect `(512,157 141x32)`, `open_about` rect
> `(661,157 75x32)`. **The click always fired.** The dialog then rendered one viewport below the fold:
> panel `y = viewport_h + 299` at *every* height (1059@h=760, 1419@h=1000, 2619@h=1800) while the
> `.overlay` itself sat correctly at `(0,0)`. Worse, **the open frame was byte-identical to the closed
> frame — 0 of 42990 bytes differ.** That is literally "does nothing", not even a dim (the missing dim
> is B2 below).
>
> **Mechanism.** `apply_absolute_position` (`velox-dom/src/layout.rs:3148`) is a tail pass over an
> already-built subtree (`PendingAbsolute`). It overwrote `node.rect.x/y` and **nothing else**, so the
> box moved to the viewport while its children stayed at the page's foot. The **sticky** path at
> `layout.rs:5514-5516` already did the right thing via `translate_layout_descendants`; the
> absolute/fixed path simply omitted it. Fixed by capturing `(pre_x, pre_y)` before the offsets
> overwrite the rect, then translating the subtree by `(dx, dy)` **and shifting the box's own `clip`** —
> `translate_layout_descendants` starts at the children and would otherwise leave the clip behind.
>
> **Remaining approximation, named not hidden:** an out-of-flow **resize** (`left:0; right:0`) still does
> not re-flow children; a browser re-runs layout there. Translation is correct; resize is the same
> approximation `apply_sticky_position` already makes.
>
> **The hypothesis I dispatched was wrong, and the disproof is worth keeping.** I predicted duplicated
> `source_index` ordinals would make `collect_click_targets` (`velox-renderer/src/events.rs:235-255`)
> walk the same VNode twice and skip a sibling, and told the lane a dedup would be "a mitigation, not a
> fix". Duplicated ordinals are real and in exactly the predicted place (`.tagline` wraps to two lines and
> emits `child[0] src_index=Some(0)` and `child[1] src_index=Some(0)`) — but **that is the only
> representable encoding**: one VNode child has one index, so N line fragments must all reference it. The
> fragments belong to a single VNode child, so re-walking it re-walks the same element with the same
> rect, and every other sibling keeps its own distinct ordinal. Measured on a button beside a four-line
> run: collected exactly once, hit correctly. **A dedup would have been a fix for a bug that does not
> exist, and would itself have introduced one.**
>
> New tests: `velox-dom/tests/fixed_position_carries_subtree.rs` (6, DOM layer) and
> `velox-renderer/tests/fixed_dialog_below_the_fold.rs` (8, over the real templates) — including the
> byte-diff proof that opening a dialog changes the frame, and a control case proving the click path was
> never the defect. RED proof: reverting only the translation turns 4 dom + 4 renderer tests RED with the
> exact symptom, while the 8 click-path and ordinal tests stay GREEN — precisely the claims not made.

**B1 (original finding, for the record) — dialogs render one viewport below the fold, at every viewport
height.** A `position: fixed`
overlay has its own box corrected in a tail pass **after** its children are laid out, so the panel is
placed relative to the overlay's **static** position — the foot of `.app`, which is `min-height: 100vh`.
Panel y measured at `h + (h − panel_h)/2`: y=1059 @ h=760, y=1058 @ h=1800. Moving the carriers first does
bring the panel in frame, but paint order is DOM order, so the page then paints over it. This is why the
Confirm/About dialogs are unusable today, independent of the dead-button bug.

**B2 — `rgba(...)` never paints.** `velox-style/src/lib.rs:316-326`: `DeclarationParser::parse_value`
loops on `next_including_whitespace()`, which errors at the first comma, so the cascade hands the
renderer `background: rgba(;`. Visible in a layout dump. Every `rgba()` scrim in the templates is
silently dropped, which is why the dialog PNGs show no dim. The scrim *values* are unaffected — the WCAG
test reads them from the sheet text, not the cascade — so the contrast arbiter still holds.

> **CORRECTION 2026-10-02 — mechanism now pinned by measurement (fix-41).** Both of my earlier accounts
> were wrong in detail, though both were right that the symptom is real. Checked at source and measured:
> - **The renderer is fine.** `velox-renderer/src/skia_render.rs:259-282` handles `rgb(`/`rgba(` with
>   three or four components and converts alpha as `(v * 255.0) as u8`.
> - **The sheet parser is the culprit, and it is worse than "stops at a comma".**
>   `velox-style/src/lib.rs:316-326` `DeclarationParser::parse_value` iterates top-level tokens calling
>   `to_css` and **never calls `parse_nested_block`**, so **every function token loses its arguments** —
>   `rgba()`, `rgb()`, `calc()`, `var()`, any future function. `background: rgba(20,24,27,0.34)` becomes
>   `background: rgba(`.
> - **Why a class rule fails and an inline `style` succeeds:** the class rule goes through
>   `DeclarationParser`; an inline `style` attribute is not parsed at all, it passes through. That is the
>   whole asymmetry, and `velox-dom/tests/composer_pixels.rs:33-36` had already measured it
>   (`.add { background: rgba(0,0,255,0.5) }` paints nothing; the same value inline paints `#8080ff`)
>   and routed around it with an inline style, calling it "a separate defect, out of scope here".
> - **Blows up any future template too:** any `calc()`, `var()` or `rgb()` in the scaffolded components is
>   silently destroyed the same way. This is a framework bug, not a template bug.
>
> Every scrim in the templates is a class rule, so every scrim is dropped — which is why the user saw
> literally *nothing* happen rather than a dimmed page. **The fix belongs in
> `velox-style/src/lib.rs:316-326`** (handle nested blocks), not in either colour parser.

Neither is in the user's list of five, but B1 makes the About dialog unreachable-looking and B2 makes any
overlay fix look wrong until it is fixed. Both need their own lanes.

**Renderer image support (`48a7f6e`).** Probe registered in `prepare_frame` before `compute_layout` (the
only place it is consulted) and in `render_frame` for callers arriving with a precomputed layout; answers
from the image cache, never re-reading per query. Both `ImageCache` sites hoisted to a thread-local
matching the font cache's take/Drop-restore pattern. Decode counter is a **separate `Cell`, not a cache
field** — the cache is out of its slot for the duration of a paint walk, so a field would read back as
zero to exactly the observer most likely to look (one spanning a frame).

SVG via `resvg 0.48.1` / `usvg 0.48.1` / `tiny-skia 0.12.0`. **`load()` tries Skia's `from_encoded` first
and only falls to resvg on `None`** — ordering, not a content sniff, is what keeps a real JPEG safe from
miserouting through the SVG branch. Both branches converge on one representation, so nothing downstream
can tell which produced a bitmap. An image cache needs **no scale key** (verified against skia-safe's
`draw_image_rect` signature: `src = None` means scale at paint time, nothing baked into the bitmap),
unlike the font cache which resyncs because glyphs are rasterised at scale.

`https:` and `data:` remain unsupported and are **pinned as test cases**, so adding scheme support later
has to update that test rather than pass it for the wrong reason. Known limitation: an SVG rasterises at
its own size and is scaled by Skia — sharp on a 24 px icon, soft on a 400 px hero.

**Clippy fixes in the render-proof harness (`f7fd1c5`).** `ink_box`'s nine positional arguments were four
unrelated ideas plus a bare half-open four-tuple in an unnamed order; now three small structs
(`Raster`/`Ink`/`Window`), no `#[allow]`. The collapsible `if` became an edition-2024 let-chain with
identical semantics. `velox-cli` 117 passed before and after; behaviour verified byte-identical by the
toggle-centring report, not just "still green".

## SHIPPED THIS ROUND — the log of what landed and why

| commit | what |
|---|---|
| `ceb14d3` | T9 flex automatic minimum size |
| `293fbe5` | T8 + T8b |
| `cfbde3b` | Noto-era font pin reseed after the DejaVu swap |
| `5b9a2e5` | `<img>` as a real replaced element |
| `40ebaed` | GNOME/libadwaita design pass + `scaffold_render_proof.rs` |
| `48a7f6e` | renderer SVG + persistent image cache + intrinsic-size probe |
| `f7fd1c5` | two clippy lints fixed properly (structs, not `#[allow]`) |
| `01050db` | **B1** — `position:fixed` must carry its subtree |
| `fff7b2d` | README licence + trademark position for the logo |
| `c0f7554` | logo assets wired into `init` + `dev`; the field stops painting its own edge |

**Logo design decisions (des-3).** 64px white plate, gear at 48px — 48 and not 32 because the gear has 32
teeth and at 32px each tooth is a sub-pixel feature that fuses into a grey ring; `rasterize_svg`
rasterises once at the file's intrinsic 106px and Skia scales at draw time with no mipmaps, so every
extra factor of downscale is blur you cannot recover. Top-aligned, not centred: the brand column is
three lines, so centred puts the plate's midpoint between title and tagline and it reads as belonging
to the tagline. **The plate is white in BOTH themes, deliberately** — the source SVG is `stroke="black"`
on a transparent centre so there is no inverted variant; black ink is 21:1 on white either way and the
plate is 19.4:1 against the dark page. Both `.vx` files carry a comment saying so. PNG is 3,261 bytes
at 128×128, rasterised by velox-renderer itself; regeneration is an `#[ignore]`d test.

**The composer's real defect was NOT the reported one (des-4).** Measuring the pixels rather than the
layout showed the row flush and symmetric at 1px on both sides — no 4px/0px asymmetry. What was there:
**1046 pixels** forming a square-cornered box inside the plane, the input's own 1px `#c8c8c8` outline
(`skia_render.rs:2948-2965`, a deliberate fallback so an unstyled input still looks like a field). Fixed
with `border: 1px solid transparent`; `border: 0` **provably does not work** (the painter forces
`stroke_width = 1.0` whenever an author border exists at all) and the longhands cannot work either (the
painter reads only the `border` shorthand). Padding `0 12px` → `0 11px` to hold the text at 12px from
the plane's inner edge.

**TWO MEASUREMENT TRAPS any future pixel test here must know.** The proof PNGs are **620×760**, not
640×760. And **the pre-render layout and the painted frame disagree by 8px vertically**, because the
renderer swaps in its own text measurer on the first frame — reading geometry off the layout pass gives
you a paragraph that is not the one on screen.

**B2 shipped (fix-42).** `write_component_values` in `velox-style/src/lib.rs` consumes the component
value list properly via `parse_nested_block`; `ToCss` only emits the opening half, so the closing
delimiter is written by hand to avoid double-emitting. Recurses; whitespace passes through verbatim.
`parse_prelude` got the same treatment as a deliberate extension — `@media (min-width: 700px)` was
arriving as `@media (`. **All four dialog backdrops in the scaffolded templates now paint.**

## Still to do on images

- **Persistent `ImageCache`** (`velox-renderer/src/skia_render.rs:2118` builds a fresh one every frame,
  directly under the persistent font cache at `:2116`). There are **two** `ImageCache::new()` sites —
  the second is in the headless `render_vnode_to_rgba` proof path (`:1176`, drawing at `:1264`) — both
  must be hoisted. Scale-independence verified rather than assumed: `draw_image_rect(img, None, rect, …)`
  passes `None` for the source rect and scales at paint time, so no scale key is needed (unlike the font
  cache, which must resync because glyphs are rasterised at scale).
- **SVG.** Nothing exists — zero hits for `resvg|usvg|tiny-skia` in any `Cargo.toml` or `.rs`. Everything
  downstream is already format-agnostic (the cache returns `sk::Image`; layout, paint, `opacity` and
  `filter:` already work), so it is one insertion point in `ImageCache::load`. `resvg 0.48` +
  `usvg 0.48` + `tiny-skia 0.12` resolve cleanly (44 packages, 2.34 MiB of source). **Version trap: the
  host has tiny-skia 0.8.4 cached from an unrelated dep; resvg 0.48 needs `^0.12`, so let cargo pick.**
  Deliberate decision to make: `default-features = false`, because resvg's defaults pull `system-fonts`
  + `memmap-fonts` (→ fontdb, rustybuzz, ttf-parser, fontconfig-parser) purely for SVG `<text>`, which is
  the single largest new build cost and is unnecessary for logo/icon SVG.
- **Asset pipeline** (`velox-cli`): `assets/` is created empty by `init.rs:105` and never populated;
  `ImageCache::load` is CWD-relative `std::fs::read` with no `current_exe` fallback anywhere in the
  workspace; `velox dev` watches `<project>/src` only (`dev.rs:1103`), so replacing a logo triggers no
  rebuild; `build.rs` emits `rerun-if-changed` only for `.vx`.
- **CSS `background-image`** is parsed into `ComputedStyle` (`style.rs:1274`, `:1588`) and listed in
  `PARSED_BUT_UNRENDERED` (`:1352`); `parse_style_attr` (`skia_render.rs:561-642`) accepts colours only.

## Sequencing

1. exp-22 lands a root cause → a fixer lane fixes the hit-test/layout-rect bug. This must land
   **before** any visual sign-off, because a dead button cannot be visually verified.
2. exp-23 lands the capability matrix → an implementation lane adds image support to the framework
   (`<img src>`, PNG decode, and whatever SVG path is viable), then a lane wires the velox logo SVG
   into the main UI and a PNG into the About modal in the scaffolded templates.
3. des-2 lands the design system. Its colour work is gated on `velox-cli/tests/template_palette_wcag.rs`
   staying green — that test, not taste, is the arbiter for contrast.
4. A final render proof: rasterise the scaffolded app in both themes and inspect.

## Standing constraints for every lane

- **Never two implementers in parallel** (cargo lock; a dev server currently holds the target dir).
- Line numbers drift — re-grep at dispatch, never copy from an earlier brief.
- A cost or deadness claim without a measured baseline is not evidence.
- A `grep` that finds nothing is a RESULT; never put `set -e` before an expected-empty search.
- Commit centrally, by explicit filename, never `git add -A`/`.`.
- Remove all instrumentation before commit; prove a mutation goes RED before believing a test.
