# Premium Boilerplate, Core Components & Renderer Correctness

Branch: `feat/premium-boilerplate-core-components` (from `fix/2A-flex-complete`)
Date: 2026-10-02
Status: planned, ready to execute

## Origin

Manual QA of the scaffolded boilerplate passed the baseline ("all seems
okay"), then surfaced seven requirements. This plan sequences them against
what the codebase actually supports today.

Four audits were run before writing this. Every claim below marked
**[verified]** was re-checked against source by the planner; the rest is
audit output that has not been independently confirmed.

### Audit reliability warning

Two of the four audits shipped false claims, both caught in review rather
than in use:

- The plan/progress audit named `velox-renderer/src/lib.rs:1014` as the
  `into_direct_context` call site. It is `Ok(SkiaRenderer)` in the
  non-native stub; the real site was `lib.rs:884` and the edit actually
  needed was the re-export at `:1115`. It also missed a fifth defective
  site entirely, and listed two sites that were correct only by accident
  of drop order.
- The same audit's "Phase-6 proptest 6.3" does not exist. There is no
  `proptest` dependency in any `Cargo.toml`, no `proptest!` macro in any
  `.rs`, and no commit in the project's history that ever added one.
  `velox-style` has no `[dev-dependencies]` section at all.

Per AGENTS.md this is the documented failure mode — an empty or
UNKNOWN caller set means "the index could not answer", not "nothing
depends on this". Treat every remaining unverified audit claim as a
hypothesis, not a finding.

### Two reconcilers, opposite fates

The README's "Virtual DOM diffing" claim needs a distinction the brief
got half wrong, because it changes what should be written:

- `velox_renderer::reconcile_keyed_children` — **DELETED**. Grep returns
  only prose in test comments. Recorded in `docs/RECONCILER.md` and
  `CHANGELOG.md:29`.
- `velox_dom::diff::diff` — **still ships**, exported at
  `velox-dom/src/lib.rs:67`, complete and duplicate-key safe. It has zero
  production callers; only tests.

So the machinery is half-present. The README's error was not the
existence of a reconciler, it was implying a reconciler participates in
rendering. `docs/RECONCILER.md` is unambiguous: the loop is immediate-mode,
`compute_layout` runs fresh on every frame, and there is no retained tree
to diff against.

---

## Findings that shape the plan

The recurring theme: **the boilerplate is not missing these features, the
renderer is dropping them.** The template already declares a placeholder
and a `line-through` rule. Both reach the DOM. Neither reaches the screen.

### F1 — Requirement 1 (placeholder) and 2 (strike-through) are renderer gaps

`TodoInput.vx:7` already binds `:placeholder="placeholder"` and
`Todos.vx:64-66` already supplies the text. The attribute survives
codegen — `examples/todo/tests/render_proof.rs:264-266` asserts it is on
the VNode. **[verified]** `grep -i placeholder` across all five `src` trees
returns only comments and unrelated matches: no `placeholder` arm in
`velox-dom/src/style.rs`, no read in `velox-renderer`, no `::placeholder`
pseudo-element machinery anywhere, no placeholder colour token.

`TodoItem.vx:65-68` already has `.todo-item.completed .todo-text {
text-decoration: line-through; color: #8b949e; }`. The renderer's
`text-decoration` arm handles only `underline` and `none`.

Consequence: these are renderer capability tasks. Authoring work in the
template would be wasted.

### F2 — Requirement 3 (caret inset) is four disagreeing origins

**[verified]** For `.input { padding: 6px 10px }`:

| Lane | Text origin |
|---|---|
| Paint — glyphs, selection, caret | `rect.x + 5.0` |
| Click→caret, class-based padding | `rect.x + 0` |
| Click→caret, inline `padding-left: 12px` | `rect.x + 12` |
| Font size (paint vs hit-test) | 14px vs 16px |

Paint hardcodes `TEXT_PAD = 4.0` (`skia_render.rs:1957`) plus a hardcoded
1px border inset (`:1929-1934`). The hit-test lane's
`resolve_text_origin_x` (`lib.rs:372-380`) looks up the literal key
`"padding-left"`, but the cascade stores `padding: 6px 10px` under the key
`"padding"` (`velox-style/src/lib.rs:713-715`) — no shorthand expansion —
so `unwrap_or(0.0)` fires. The template uses the shorthand, which is
exactly the failing case.

Hit testing also uses `DEFAULT_INPUT_FONT_SIZE: f32 = 16.0`
(`lib.rs:226`) while paint inherits (root default 14.0,
`skia_render.rs:1730`).

Fixing this means one shared geometry function used by both lanes, not
two patch-ups.

### F3 — Input paint overrides the author's styling wholesale

**[verified]** `skia_render.rs:1849` paints a hardcoded opaque white and
`:1850` a hardcoded `#c8c8c8`; the border at `:2060-2062` is drawn as a
plain rect, not an rrect. The template's `background: #16213e`,
`color: #e6edf3`, `border-radius: 8px` are all discarded. `parse_style_attr`
(`:312-352`) has no `padding` arm. Value text is painted hardcoded
near-black at `:2013`.

This is why the field looks wrong, and it must be fixed before any
"premium restyle" can land.

### F4 — `<input>` has no intrinsic size, and no UA rule

**[verified]** `velox-style/src/ua.css` has **no** `input` rule — the only
hits for `input`/`select`/`textarea` are the comment at line 34 stating
they are deliberately absent and `button` at line 23. `input` is
`display: block`. With no children, `content_h = 0`, so auto height is
exactly `padding-top + padding-bottom + border-top + border-bottom`
(`layout.rs:5352-5363`). The template's `padding: 10px 12px; border: 1px`
yields a **22px field**.

"Better sizing" is therefore "add a UA default", the same shape as
Phase-4 task 4.2a.

### F5 — Core components do not exist as a concept

**[verified]** `grep -riE 'core.?component|builtin|built-in|global.?registry|portal|teleport'`
across the workspace matches one Zed syntax file and nothing else.
`velox add` (`velox-cli/src/commands/add.rs:81-119`) writes a file and
prints the import line for the user to write by hand; it errors if the
file exists.

Three designs were considered:

1. **Scaffold + require import** (today's `velox add` shape). Zero
   framework change. Fully restyleable and overridable because the user
   owns the file.
2. **Codegen seed** — add `ComponentResolver::with_builtin(source, tag,
   default)` and call it at `codegen.rs:59-62` and `build.rs:88-92` so
   `<Modal>` compiles with no import. `add_import` and the `imports` field
   are currently private (`component_resolver.rs:27-32`, `:85`).
3. **Whole-module injection** — higher risk; `#[path]` module naming and
   `sanitize_mod_name` collisions are unhandled.

**Decision: option 1 for this cycle.** It satisfies "user-styleable and
overridable" by construction and carries no codegen risk. Option 2 is the
follow-on, recorded as B6.

### F6 — Component emits are inert; `emit()` has an empty body

**[verified]** `velox-sfc/src/codegen.rs:507-512`:

```rust
pub fn emit(event: &str, payload: &str) {
    if let Some(_handler_name) = super::emit(event, payload) {
        // The handler will be invoked via the parent's on_event dispatcher.
        // The handler_name corresponds to a method on the parent's State.
    }
}
```

Function props are impossible too — every bound prop is stringified
(`template_codegen.rs:3962-3973`). So `<Confirm @confirm="...">` compiles
and does nothing.

The one channel that does work: the renderer dispatches the raw `on:click`
attr string to the app's single root dispatcher
(`lib.rs:1862-1869` → `make_on_event`, `template_codegen.rs:875-886`),
which calls `state.<handler>()`. Handler name is global and hardcoded.

### F7 — No `on:keydown` of any kind; Esc is positively asserted inert

**[verified]** Every runtime read of an `on:*` attribute is three lines:
`events.rs:216` (`on:click`), `events.rs:217` (`on:click-payload`),
`lib.rs:417` (`on:input`). `grep -rn 'on:keydown|on:key'` over
`velox-sfc/src` and `velox-renderer/src` → nothing.

`edit_action_for_key` (`lib.rs:458-474`) is a closed `match` with no
fallthrough to user code. Esc is not in it, and `lib.rs:3202` **asserts it
is inert** — the test `non_editing_keys_produce_no_action` will need
updating as part of the change.

Requirement 5 (F2 focuses input) and Esc-to-close-modal both need this
hook built first.

Modifier state is also thrown away: `lib.rs:1269-1271` latches only
`shift` into a bool and discards Ctrl/Alt/Meta/Super.

### F8 — The r/q bug is live in both loops, and Confirm would trigger it

**[verified]** `lib.rs:1943` and `:2633`, identical. The only gate is
`input.state == ElementState::Pressed`. **No focused-input check.** Typing
`r` or `q` into a text field kills the app. Any input-bearing core
component makes this a blocker, not a papercut.

### F9 — `position: fixed` works and is the overlay mechanism

**[verified]** `layout.rs:3692-3710` gates `z-index` on
`position != "static"`. Out-of-flow children are appended **last**
(`layout.rs:5450`), so they paint above in-flow siblings, and
`is_fixed` uses the viewport as containing block (`layout.rs:1782-1788`).

Known limits: the sort is per-parent (`skia_render.rs:2094`), so a Modal
nested inside a sidebar will not outrank a later sibling of that sidebar;
no portal/teleport exists; an `overflow: hidden` ancestor clips it
(`layout.rs:5414-5423`).

`opacity < 1.0` does create a stacking context and is honoured in paint
(`skia_render.rs:904`, `:1799`).

### F10 — Named slots do not exist

**[verified]** The producer always emits the literal key `"default"`
(`template_codegen.rs:2079`) with a comment at `:2071-2072` calling named
slots "a future extension". `grep -rn 'v-slot' velox-sfc/src/` returns
only those comments. `<slot name="footer">` always misses and renders its
fallback.

Slot content is emitted in the **parent's** render fn, so it carries the
parent's `data-v-*`. **A Modal's `scoped` CSS cannot style slotted
content**; the caller's scoped CSS can.

### F11 — Hover works only on a hardcoded heuristic

**[verified]** `events.rs:150-159` — `is_hoverable` returns true only for
has-`on:click`, tag `button`, or literal class `btn`. A
`.modal__close:hover` rule never matches on anything else. Any hover
default in a core component must land on a clickable/`button`/`btn` node.

### F12 — Dark mode must be app-level, and `@media` is a landmine

**[verified]** No `prefers-color-scheme`, no custom-property resolution,
no `var()`. `velox-style/src/lib.rs:239-253` captures declarations as
opaque `(String, String)` with no substitution step;
`set_property` has no `--` branch. `--custom-property` is only referenced
by a test asserting the linter does *not* flag it
(`velox-cli/tests/lint_css_tests.rs:132`) — inert, not resolved.

Worse, **[verified]** `velox-style/src/lib.rs:212-223`: `@media` /
`@supports` preludes are **never evaluated**. Inner rules are flattened
into the sheet unconditionally. So `@media (prefers-color-scheme: dark)`
applies in *both* modes — silently wrong, not merely unsupported.

CSS custom properties would be the right foundation for theming, but that
is a cascade rewrite, well outside this cycle's seven requirements. The
theme toggle uses a root class binding plus literal palettes per mode.

### F13 — `.stop`, capture and focus trap are impossible today

**[verified]** `.stop`/`.prevent` are absent from `template_codegen.rs`.
A click resolves to exactly one topmost target (`events.rs:368-384`), so
a panel click bubbles nowhere and a backdrop listener cannot ignore panel
clicks. The workaround is a panel element with its own `on:click`,
hit-tested above the backdrop via paint order + `z-index` — which the
existing stack-order code already does (`events.rs:348-366`).

`autofocus` is absent. `K::Tab` is asserted to produce no action
(`lib.rs:3203`). `is_text_input` (`events.rs:399-410`) is the only
focusable thing.

### F14 — Two release blockers from the earlier audit stand

- `velox-renderer/src/skia_gl.rs:36-38` `into_direct_context(&self)`
  returns an **owning** `DirectContext` while `Drop for SkiaGlContext`
  (`:60-73`) destroys the EGL surface and context. The value provably
  outlives its GL objects. Call sites: `skia_gl.rs:284`, `:340`,
  `skia_surface.rs:209`, `lib.rs:1014`.
- Two golden PNG checksums fail on pristine HEAD (`frame_cost_bench`
  excluded): `render_overflow_hidden_clips_children` and
  `render_z_index_overlap_checksum`. Proven stale via a detached
  worktree at HEAD — byte-identical failures — so not a regression, but
  CI cannot be green until they are regenerated or corrected.

### F18 — A Windows-only test assertion will fail `P2` (known, unverifiable here)

`velox-renderer/src/presenter.rs` gates `force_backend` on a **runtime**
bool — `if cfg!(target_os = "linux")` — not a compile-time `#[cfg]`. On
Windows the body is skipped, so `WINIT_UNIX_BACKEND` is never set, yet
the test helper at `:512-520` asserts
`std::env::var("WINIT_UNIX_BACKEND").as_deref() == Ok("x11")`. That
assertion can only hold on Linux.

It is invisible to the `skia-native-non-unix` CI job because that job runs
a bare `cargo check`, which does not build `#[cfg(test)]` code. It will
surface the first time anyone runs `cargo test` on Windows.

Deliberately NOT fixed here: there is no non-unix rustup target
installed and `skia-bindings` cannot download in this environment, so any
fix would be an untested `#[cfg]` guess. Fixing it blind would replace a
known, precisely-located latent bug with an unknown one. Fix when a
Windows test run can actually verify it.

---

**The trap that decides how a theme must be built.** `scope_css` /
`scope_single_selector` (`velox-sfc/src/codegen.rs:307-308`, `:388-409`)
append `[data-v-<hash>]` to **every compound including the leftmost**,
and the id is **per component** (`:281-288`). `append_scope_attr`
(`:2676-2682`) tags only that component's own elements.

So a rule in `App.vx`'s scoped block written as `.dark .todo-item`
compiles to `.dark[data-v-APP] .todo-item[data-v-APP]` — while
`<Todos/>`'s elements carry `data-v-TODOS` (children are inlined into the
parent's VNode tree, `template_codegen.rs:2067`). **It never matches.**

A theme class on the root is therefore *necessary but not sufficient*.
Every component that must respond to the theme needs its OWN `.dark`
rules inside its OWN scoped block. The duplicated dark rules are the
correct shape, not a workaround. The corollary: an **unscoped** block in
`App.vx` *does* reach child components, because the cascade walks the
whole inlined tree (`velox-style/src/lib.rs:845-848`).

### F17 — Theme mechanism verdict, with the traps that define it

The cascade has **no specificity and no `!important`**: rules apply in
source order, last match wins (`velox-style/src/lib.rs:812-823`). This
makes *ordering* the entire override mechanism — base rules first,
theme overrides last — and makes appending the only way a user can
override.

**What works today.** Attribute selectors work (`lib.rs:114-124`,
exact-string `=` only — no `~=`/`^=`/`|=`). Descendant and ancestor
matching work: `match_prefix` walks the ancestor chain
(`:634-666`) and `Descendant` scans all of it (`:651`), checking the
ancestor's `class` (`:107-113`). Root `:class="{ dark: is_dark }"`
object syntax is proven at pixel level
(`velox-sfc/tests/root_vif_class_behaviour.rs:158`) and re-themes the
frame because the cascade re-runs per frame.

**Adopted mechanism:** root `:class` object binding + per-component
`.dark` descendant rules written LAST in each component's single scoped
block. Zero framework change. Override surface = appending your own
`.dark …` rules.

**Silent-failure traps recorded so they are not re-learned:**

| trap | behaviour |
|---|---|
| `--custom-prop` / `var(--x)` | inert; no `--` arm (`velox-dom/src/style.rs:1761`), no `var()` pass, and lint never reports it (`lint_css_tests.rs:132`) |
| `@media (prefers-color-scheme: dark)` | flattened in **unconditionally** (`velox-style/src/lib.rs:249-268`) — dark wins in light mode |
| `+` / `~` combinators | do not exist; parse as tag `+` (`:549` → `:476-486`), never match, zero tests cover them |
| a second `<style scoped>` block | silently **replaces** the first (`velox-sfc/src/sfc.rs:283-286`) |
| `:class="cond ? 'a' : 'b'"` | always `""` (`template_codegen.rs:4465-4469` + `:919`); only a stderr warning (`:5135-5139`) |
| `:style="cond ? 'a' : 'b'"` | same, and inline outranks every sheet rule (`:674-685`) |
| parent scoped rule reaching a child's elements | never matches (F16) |
| `:hover` on an arbitrary node | only `button` / `on:click` / class `btn` (`events.rs:150-159`) |

Two prior line refs in this plan were stale and are corrected: the cascade
insert is `:820`, and `@media` is `:249-268`.

**Deferred by design:** custom properties are the only mechanism that
naturally crosses the component boundary, since they inherit via
`filter_inheritable`. That is the strongest argument for building them
later — a cascade change in three sub-parts, with no painter work if
substitution happens before the value lands in the `style` string.

---

`examples/todo/tests/render_proof.rs:290-293` asserts
`typed_dark_pixels > empty_dark_pixels + 5`, where `dark_pixels_in`
(`:90-110`) counts pixels with **every channel `< 120`**.

That measure was correct only while the painter hardcoded a white input
box with near-black value text. A4 removed the hardcoding, so the input
now honours the author's background — `#1e293b` = (30, 41, 59), every
channel below 120, so the **background itself** counts as dark — while
the typed value `#e2e8f0` = (226, 232, 240) and the derived placeholder
≈(138, 146, 158) both sit above the threshold and are excluded. Typing
therefore *reduces* the dark count, and the assertion is inverted for
any dark-themed input.

Verified not to be an sfc regression: stubbing `emit_dispatch_registrations`
to emit nothing reproduces identical numbers, `grep -rn "emit(\|
define_emits\|dispatch_emit" examples/todo/src/` returns nothing, and every
VNode-level assertion in the same test passes — including
`input.get("value") == Some("Ship the rewrite")`. Codegen is right; the
probe is wrong.

The fix is to stop measuring brightness. Count pixels that **differ from
the input's own resolved background colour** — an ink measure rather than
a dark measure. That is theme-independent, which is exactly what
requirement 7 needs: the same assertion must hold in light and dark mode.
A brightness threshold cannot express that, because which end of the
scale counts as "ink" is a property of the theme, not of the text.

---

## Sequencing

Phase A and B2/B3 are independent and run in parallel. Phase B4/B5 depend
on A7 and B3. Phase C depends on A3–A6. Phase D is independent
throughout.

```
A1 ──────────────────────────────────────────────► D2
A2 ──────────────────────────────────────────────► B4 (Confirm)
A3 ──► A4 ──► A5 ──► C
A6 ──────────────────────────────────────────────► C
A7 ──┬───────────────────────────────────────────► C2, C4
     └───────────────────────────────────────────► B4 (Esc closes)

B1 ──┬───────────────────────────────────────────► B4, B5
B2 ──┤
B3 ──┴───────────────────────────────────────────► B4, B5
```

---

## Phase A — Renderer correctness (blocking)

Write scope: `velox-renderer/src/**`, `velox-style/src/ua.css`.

### A1 — Fix the `DirectContext` lifetime bug `P0`

`into_direct_context(&self)` → `into_direct_context(self)`, moving the
guard with it. Update the four call sites. Add a test that constructs a
context, drops it, and asserts no crash — plus a Miri or
`cargo +nightly miri` note in the commit if cheap.

### A2 — Guard the r/q reload keys on focus `P0`

Both arms, both loops (`lib.rs:1943`, `:2633`). Skip when an input is
focused, mirroring the guard at `lib.rs:435-437`. Update the existing
`non_editing_keys_produce_no_action` test to assert the focused-input
case does not exit.

### A3 — One shared input content-box geometry `P0`

New `velox-renderer/src/input_metrics.rs` exporting a single function
that computes the text origin and content width from the **computed**
style: border widths + resolved padding sides. Both the paint lane
(`skia_render.rs`) and the hit-test lane (`lib.rs::resolve_text_origin_x`)
call it. Resolves the shorthand problem by reading computed values instead
of re-parsing the authored string, so `padding`/`padding-left`/`12px` all
work.

Also unifies font size: delete `DEFAULT_INPUT_FONT_SIZE`
(`lib.rs:226`) and use the inherited computed size in both lanes.

### A4 — Let author styling through the input paint `P0`

`skia_render.rs:1849-1850` stop hardcoding white and `#c8c8c8`; fall
back to those only when the author specified nothing. Border drawn as an
rrect so `border-radius` survives (`:2060-2062`). Value text colour from
`color` (`:2013`). Add `padding` handling to `parse_style_attr`.

### A5 — Placeholder paint path + UA input defaults `P1`

New: read `placeholder` from attrs, paint it when the value is empty, in a
dimmed colour derived from the field's own `color`. Add `::placeholder`
pseudo-element support so it is styleable, not just functional.

`ua.css`: add an `input` rule — UA padding and a `min-height` — so the
field stops being 22px tall. Update the line-34 comment, which currently
claims inputs are deliberately absent.

### A6 — `line-through` text-decoration `P1`

Extend the `text-decoration` arm. Needs a measure-then-strike pass for
wrapped lines. Add tests for `line-through`, `underline line-through`,
and `none`.

### A7 — Global keybinding hook + Esc + focus/blur API `P0`

The foundation for requirement 5 and for Esc-to-close.

1. Add `EditAction::Dismiss` and a `K::Escape =>` arm in
   `edit_action_for_key`, routed through the existing root-dispatch path
   so it lands in the app's dispatcher. Applied only when no input is
   focused. **Update `lib.rs:3195-3213`**, which currently asserts Esc is
   inert.
2. Lift `input_targets` / `focused_input` out of the two run-loop locals
   into a `WindowState` struct, and expose `focus_input`/`blur_input` so
   an app can drive focus without a click.
3. Preserve modifier state — stop discarding Ctrl/Alt/Meta/Super at
   `lib.rs:1269-1271` — so F2-with-modifier checks are possible.
4. Add `on:keydown` collection alongside the existing `on:click` /
   `on:input` collection, dispatched to the root handler.

---

## Phase B — Core components

Write scope: `velox-sfc/src/**`, `velox-cli/src/commands/add.rs`,
new `velox-cli/templates/**` component files.

### B1 — Make `emit()` actually invoke the parent handler `P0`

`codegen.rs:507-512`. The callback name is looked up but discarded. Wire
it through to the parent's dispatcher. Include function-typed props so
`<Confirm :on_confirm="handler">` is expressible, not just stringified.

### B2 — `velox add modal` / `velox add confirm` `P1`

Extend `add.rs:81-119`. Write the component source from a template,
print the import line. Overwrite-safe (currently errors if the file
exists) with a `--force` flag.

### B3 — Named slots `P1`

`v-slot:foo` and `#foo="slotProps"` in `grammar.pest` and
`template_codegen.rs:2071-2088`. Required for a Modal with header / body /
footer. Document the `data-v` scoping consequence (F10) and make the core
Modal ship an **unscoped** style block so app CSS can override by class
and slotted content is reachable.

### B4 — The `Modal` component `P1`

Backdrop via `position: fixed; left:0; top:0; width:100vw; height:100vh;
z-index:<high>` (F9). Panel as a sibling with its own `on:click` so panel
clicks do not dismiss (F13). Close button must carry `on:click`, be a
`button`, or have class `btn` or hover will never match (F11). Esc and
backdrop-click both route through A7's dispatch. `:open` bound as a
string, driving `v-if`.

### B5 — The `Confirm` component `P1`

Builds on B4. Two fixed handler names resolved in the root dispatcher,
or B1 if it lands first. **Depends on A2** — it has a text input.

### B6 — Codegen-seeded built-ins (deferred) `P2`

Option 2 from F5: `ComponentResolver::with_builtin`, so `<Modal>` works
with no import line. Deferred — option 1 already meets the requirement.

---

## Phase C — Premium boilerplate

Write scope: `velox-cli/templates/project/**`.

### C1 — Palette, tokens, light and dark `P1`

Replace the two hardcoded colours with a token set. Dark and light
palettes as literal values on a root class binding — **not** `@media`
(F12) and **not** `var()` (F12).

### C2 — Theme toggle icon button `P1`

Icon button bound to the root class. Hover styling must land on a node
`is_hoverable` accepts (F11).

### C3 — Restyle input and todo item `P1`

Depends on A3–A6 landing, or it will style properties that are dropped.

### C4 — Wire F2 and Esc `P1`

Depends on A7.

---

## Phase D — Publishability

### D1 — Resolve the two stale golden checksums `P0`

Regenerate on real hardware, or correct the expectations if the current
output is wrong. Until then CI cannot go green. Note in the commit that
the pins were stale on pristine HEAD.

### D2 — Fix the CI trigger `P1`

`.github/workflows/ci.yml:3-6` fires only on `[main, master, alpha]`, so
the dev branch gets zero CI. Add the feature branches, plus a coverage
gate, `cargo audit`, and a proptest job.

### D3 — Documentation accuracy `P1`

CHANGELOG.md:50 claims "205 tests"; the real count is 1160. README
overclaims `box-shadow`, `font-style`, "Virtual DOM diffing" (the
reconciler was deleted), and "hot reload" (every edit is a full process
rebuild). Fix or remove.

### D4 — Kill the dead code `P2`

~4,000 lines with no production callers: `velox-sfc/src/expr.rs` (1,973),
`velox-dom/src/text_wrap.rs` (633), `velox-dom/src/diff.rs`,
`velox-renderer/src/event_binding.rs` (280),
`velox-core/src/provide_inject.rs` (128), `watch.rs`. Note
`provide_inject` is referenced by the plan for theming — check before
removing. Deleting public API needs a deprecation note.

### D5 — CSS specificity and `!important` `P2`

`velox-style/src/lib.rs` has neither; `acc.insert` makes the last matching
rule win outright. Required for real override semantics, and a
precondition for user-styleable core components in the long run. Cascade
rewrite — its own cycle.

### D6 — Remove dead non-optional dependencies `P0` (was `P2`)

`pollster`, `wgpu_glyph`, `ab_glyph` at `velox-renderer/Cargo.toml:41,43,44`.

**Promoted to P0 during D9 triage.** `grep -rn 'wgpu_glyph\|ab_glyph'`
over `velox-renderer/src`, `velox-renderer/tests` and `examples/`
returns 0 hits — all three are genuinely unused. `wgpu_glyph` is the
sole reason the transitive chain `wgpu_glyph → glyph_brush →
glyph_brush_draw_cache → crossbeam-deque → crossbeam-epoch 0.9.18` exists
at all, and `crossbeam-epoch` carries RUSTSEC-2026-0204. Deleting three
unused lines removes a vulnerable crate from the shipped graph, which is
the only available fix: the advisory has no patched version.

### D7 — Make `skia-native` build on non-unix `P0`

Found while fixing A1. The GPU path is **unbuildable on Windows and
macOS**: the stub `SkiaGlContext` (`skia_gl.rs:436`) has no
`into_direct_context`, and `skia_surface.rs` calls
`create_context_from_winit`, which only exists in `unix_impl`. Every gate
so far has run on Linux, so this rot was invisible.

"Publishable" means it builds cross-platform. Add a
`cargo check --features skia-native` job for a non-unix target to CI so
this cannot rot again, and either implement the stub or make the feature
fail with a clear message instead of a missing-method error.

### D8 — Record the `create_direct_context` API break `P1`

`velox_renderer::create_direct_context()` now returns `GlDirectContext`
rather than `skia_safe::gpu::DirectContext`. Callers use `.dctx()` /
`.dctx_mut()` / `.gl()` / `.into_parts()`. Needs a CHANGELOG entry when
the release is cut.

### D9 — Triage the `cargo audit` findings `P0` — DONE (analysis), needs recording

`cargo audit --json` reports **5 vulnerabilities and 0 warnings**. The
"9 further advisories" claim is false; the advisory DB has no warnings
for this tree. All five list **no patched version**, so none can be
resolved by upgrading. They have to be resolved or justified per
advisory, which is what the table below does.

| advisory | package | reached via | verdict |
|---|---|---|---|
| RUSTSEC-2026-0204 | crossbeam-epoch 0.9.18 | `wgpu_glyph` → `glyph_brush` → `glyph_brush_draw_cache` → `crossbeam-deque` | **RESOLVE BY DELETION** — see below |
| RUSTSEC-2026-0194 | quick-xml 0.38.4 | `softbuffer` → `wayland-client` → `wayland-scanner` (proc-macro) | waive — build-time only |
| RUSTSEC-2026-0195 | quick-xml 0.38.4 | same | waive — build-time only |
| RUSTSEC-2026-0067 | tar 0.4.44 | `skia-safe` → `skia-bindings` (build-dep) | waive — build-time only |
| RUSTSEC-2026-0068 | tar 0.4.44 | same | waive — build-time only |

**RUSTSEC-2026-0204 is fixed by D6, not by a version bump.** The whole
chain exists only because `wgpu_glyph` is declared at
`velox-renderer/Cargo.toml:43` while being completely unused: `grep -rn
'wgpu_glyph\|ab_glyph'` over `velox-renderer/src`, `velox-renderer/tests`
and `examples/` returns **0 hits**. `ab_glyph` (`:44`) and `pollster`
(`:41`) are dead the same way. Deleting three unused lines removes a
transitive vulnerable crate from the graph, so D6 is a security task, not
just hygiene.

**The four waivers are the same argument, and it is verifiable.** Both
`quick-xml` and `tar` reach velox only at build time:

- `cargo tree -i tar --edges normal` returns **zero** edges. `tar` is
  reached solely through `skia-bindings`, a `[build-dependencies]` entry.
- `quick-xml` is reached only through `wayland-scanner`, which is a
  **proc-macro**. Proc-macros compile to host dylibs that run during
  compilation; their code is not linked into the artifact a downstream
  user builds. The path also goes through `softbuffer`, which is optional
  and enabled only under `skia-native` on unix
  (`velox-renderer/Cargo.toml:21,30`).

Both vulnerabilities are memory-safety and resource-exhaustion issues in
parsers. velox feeds neither parser untrusted input: one unpacks a
vendored Skia tarball at build time, the other reads Wayland protocol
XML that ships with `wayland-scanner`. A downstream velox user cannot
reach either code path.

Recording this matters because `cargo audit` does not distinguish
build-dependencies from runtime dependencies, which is exactly why the
raw count of 5 looks worse than the exposure. The CI job should encode
the per-advisory justification rather than carry an undifferentiated
ignore list.

### D10 — Stop the feature leak that makes `build-test` non-minimal `P1`

`velox-cli/Cargo.toml:52` dev-depends on `velox-renderer` with
`features = ["skia-native"]`, and the example crates build-depend on
`velox-cli`. Verified with `cargo tree -e features -i velox-renderer
--workspace`: `skia-native` is enabled workspace-wide, so **every**
workspace-wide cargo command compiles Skia natively.

Consequence: the `continue-on-error: true` on the `renderer-features`
job is decorative. `build-test` already catches a Skia failure first, so
that flag implies a permissiveness CI does not actually have. Either
feature-gate the dev-dependency properly or stop implying the
separation.

### D11 — Cross-frame focus is keyed by structural path `P2`

`preserve_input_state` (`events.rs:526-540`) restores focus by matching
`t.path == p.path`. An `<input>` inside a reordered `v-for` therefore
hands its focus and caret to whichever item now occupies its index.

This is the accepted cost of the immediate-mode loop — `:key` is a
declared non-goal, not an oversight — but it interacts with A7: F2-focus
is only meaningful if focus survives the reorder it is meant to target.
Record it in the A7 notes so the focus API is not documented as more
durable than it is.

---

## Verification

Every phase gates on the full suite, not a targeted test:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
cargo test -p velox-renderer --features skia-native -- --ignored
```

Baseline: 1160 passed / 0 failed / 33 ignored. The second command
currently fails 2 (D1).

Per AGENTS.md, `detect_changes({scope:"all"})` before every commit, and
impact analysis before editing any symbol with callers.

## Explicitly out of scope

Portal/teleport, focus trap, `Tab` traversal, `.stop`/`.prevent`, capture
phases, CSS custom properties, `@media` evaluation, specificity,
`!important`, function props beyond B1, images/virtual lists.