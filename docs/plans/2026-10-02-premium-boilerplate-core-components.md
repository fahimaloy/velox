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

### D6 — Remove dead non-optional dependencies `P2`

`pollster`, `wgpu_glyph`, `ab_glyph` at `velox-renderer/Cargo.toml:41,43,44`.

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