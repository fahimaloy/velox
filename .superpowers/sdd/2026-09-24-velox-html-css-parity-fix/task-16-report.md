# Task 16 Report — Flex item descendants use the resolved item position

## Status

DONE_WITH_CONCERNS

## Implementation

- Added private `translate_layout_subtree` helper in `velox-dom/src/layout.rs`.
  - Recursively translates each `LayoutNode.rect.x/y`.
  - Recursively translates each `LayoutNode.clip.x/y` when present.
  - Leaves clip width and height unchanged.
  - Does not create or alter scrollable boxes.
- Updated the flex placement pass to:
  - Capture each flex item's pre-placement root coordinates.
  - Compute the existing final row, column, reverse, and cross-axis position into local resolved coordinates.
  - Recursively translate the item subtree by the final-minus-pre-placement delta.
  - Restore the item root to its resolved coordinates before applying relative/sticky positioning, preventing offset double-application.
- Added the required regression test to `velox-dom/tests/flex_completeness_repro.rs`, covering centered row flex items with text descendants and retaining exact item-root assertions.

## TDD record

The regression test was added before the implementation and the required targeted command was run first:

```text
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-dom --test flex_completeness_repro
```

Before the fix, the new test failed at `velox-dom/tests/flex_completeness_repro.rs:42` with:

```text
assertion `left == right` failed
  left: 36
 right: 96
```

The other 9 tests in that binary passed. After the implementation, the targeted suite passed all 10 tests.

## Verification

- `cargo fmt && cargo fmt --check` passed.
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-dom` passed: 21 unit tests and all velox-dom integration/doc tests green, including `flex_completeness_repro.rs`, `flex_critical_repro.rs`, `layout_tests.rs` (including fit-content), `box_sizing.rs`, and `layout_golden.rs`.
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-example-counter -p velox-example-todo -p velox-example-showcase` passed: all example proof suites green. The existing proof output remained valid for the current examples.
- `git diff --check` passed.

## Concerns / follow-up limitation

- The existing flex pass can overwrite a pre-laid item's used width/height for flex-grow, align-self stretch, and related cases. Descendant wrapping can therefore still be based on the pre-flex dimensions. This task intentionally fixes coordinates only and does not perform a second-pass content relayout.
- The existing untracked documentation files were left untouched:
  - `docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md`
  - `docs/superpowers/plans/2026-09-24-velox-html-css-parity-fix.md`

## Fix round 1

### Finding addressed

Descendants of a flex item did not follow the item's later relative or sticky movement. The flex item root was moved by `apply_relative_position` or `apply_sticky_position` after the initial subtree translation, but only the root moved; descendants and descendant clips remained at the pre-offset position.

### What changed

- Added private `translate_layout_descendants`, which applies the existing subtree translation to each child without translating the flex item root.
- After relative and sticky positioning, computed `descendant_dx` and `descendant_dy` from the final item root minus the resolved flex position and translated only descendants by that delta. This also translates descendant clip `x/y` while leaving clip dimensions unchanged.
- Added regression test `repro_positioned_flex_item_descendants_follow_item_position`, asserting the positioned item root is offset by `left: 10px` and `top: 6px` and its text child remains centered in the item's final box.

### TDD command and results

Exact command:

```text
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-dom --test flex_completeness_repro
```

Pre-fix output:

```text
running 11 tests
...
test repro_positioned_flex_item_descendants_follow_item_position ... FAILED

---- repro_positioned_flex_item_descendants_follow_item_position stdout ----
thread 'repro_positioned_flex_item_descendants_follow_item_position' panicked at velox-dom/tests/flex_completeness_repro.rs:88:5:
assertion `left == right` failed
  left: 96
 right: 106

failures:
    repro_positioned_flex_item_descendants_follow_item_position

test result: FAILED. 10 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

Post-fix output:

```text
running 11 tests
...
test repro_positioned_flex_item_descendants_follow_item_position ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Verification

- `cargo fmt` passed.
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-dom` passed fully green, including the existing flex, box-sizing, layout, golden, and regression suites.
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-example-counter -p velox-example-todo -p velox-example-showcase` passed: 2 counter proof tests, 2 showcase proof tests, and 3 todo proof tests.
- `git diff --check` passed.
