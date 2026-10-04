# T9 — the flex automatic minimum size (css-flexbox-1 §4.5)

**Risk:** medium · **Scope:** `velox-dom/**` · **Lower priority** · **Found en route, NOT the overlap's cause**

## Read this first, or you will fix the wrong thing

This task was discovered while root-causing the Add-button overlap (T2). **T9 is not the cause of
that overlap** — `min-width: 0` is inert precisely *because* there is no automatic minimum size, so
there is no content-based floor in the way to defeat. T2 is the cause; this is a separate, real spec
gap found in the same code.

Do not conflate them. Do not let a T2 fix be justified by "it also fixes min-width".

## Root cause

css-flexbox-1 §4.5: a flex item's `min-width: auto` resolves to its **content-based minimum size**
(the min-content size, clamped by any specified min/max), not to zero. Velox has no such rule.

```rust
// velox-dom/src/layout.rs:2468-2470  (parse_length_value)
if val == "auto" { return None; }
```

`min_main_size: Option<f32>` (`layout.rs:4067`) is fed from
`style_lookup_len_full(..., "min-width", ...)` (`:4457-4467`), which returns `None` for `auto`. So a
flex item's minimum main size is **unconstrained — effectively 0 — unless a literal `min-width` is
declared.**

Verified absent: `grep -rn "automatic minimum|content-based minimum|min-content size|intrinsic
minimum" velox-dom/ velox-style/ velox-renderer/` returns **zero hits**.

## What already exists

`min-width` as an explicit length **is** implemented — the original remediation plan's
"`grep -c 'min_width' layout.rs` returns 0" claim was **false** (it returns 8):

| what | where |
|---|---|
| `used_min_width()` — resolves against the containing block | `layout.rs:1552-1580` |
| `floor_to_min_width()` — applies the floor outside the max cap | `layout.rs:1582-1610` |
| block-flow call site | `layout.rs:3587` |
| flex per-item read | `layout.rs:4457-4467` |

**So do not re-implement explicit `min-width`. It is done.**

## What is missing

Only the `auto` resolution: when a flex item's computed `min-width` is `auto` (the initial value, and
the authored default), the used minimum should be the item's **content-based minimum size**.

Per spec that is: min-content contribution, clamped by any specified min/max. Velox has no
intrinsic-sizing subsystem — `compute_layout` measures against an available size, not against zero —
so producing a true min-content size is the hard part. Be realistic about that.

## Realistic options, in order of honesty

1. **Do the narrow, correct thing:** resolve `auto` to the content-based minimum size **only where
   velox already has the measurement**, i.e. reuse the content-basis descent. This is bounded work and
   matches the §9.2 base-size work already shipped.
2. **Ship the diagnostic, not the behaviour.** Per the repo's existing `PARSED_BUT_UNRENDERED`
   discipline: record that `min-width: auto` on a flex item is not yet resolved to §4.5, so `velox
   lint` warns. This is the "make the lie visible" answer, consistent with the 4.7a work.
3. **Defer with a written note.** Only acceptable with an explicit record of what is broken and which
   author-visible symptoms follow. Record it in `docs/` — a deviation in a source comment is a trap.

**Do not** implement a partial min-content approximation and present it as §4.5. A wrong automatic
minimum size produces overflow bugs that are *harder* to diagnose than a missing one, because the
author has been given a rule that appears to work.

## Consequence while unfixed

An author who writes `min-width: 0` expecting the standard flexbox escape hatch gets nothing.
`min-width: 0` reads as "no constraint", which happens to be what velox already does — so the
*result* is often accidentally right, while the *reasoning* is wrong and the workaround is
indistinguishable from doing nothing. That ambiguity is the real cost: there is no way for an author
to discover the rule, and no way for velox to tell them it is missing.

## Tests

Whatever option you choose:
- `min-width: 0` and no `min-width` must be **distinguishable in the code path** (even if they resolve
  to the same value today) — otherwise the support cannot be added later without a breaking change.
- A long unbreakable word in a shrinkable flex item: state and pin what happens. If it overflows, pin
  the overflow. Do not leave it undefined.
- Every existing `flex_*` test in `velox-dom/tests/` must stay green. This is the suite that would
  catch a wrong automatic minimum size.

## Falsification

- M1: change `if val == "auto" { return None }` to return `Some(0.0)` and confirm the test that
  distinguishes the two paths goes RED.
- M2 (if option 1): implement the minimum size as a fixed fraction of the item's font size →
  the long-unbreakable-word test must go RED.

## Gate

```
cargo test -p velox-dom
cargo test -p velox-dom --features skia-native --test todo_item_pixels
cargo test -p velox-dom --test layout_golden
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

## Sequencing

**After T2** — same file, and T2's re-layout at the resolved size interacts with any minimum-size
clamp. Never two lanes on `velox-dom/src/layout.rs`.

## Related, also still open

The remediation plan's §9.2 **base-size vs hypothetical-size vs target-size split**. N13 is precise:
the flex base size **ignores** min/max ("no clamping occurs"); the hypothetical main size **clamps**;
the target main size **clamps after the flex resolve**. Velox's basis ladder conflates the three. This
interacts directly with anything T9 does and must not be silently reversed. Check the current state at
source before assuming it is still open — a prior design pass reported 4.8a and 4.8b as already shipped,
and that claim has not been re-verified.