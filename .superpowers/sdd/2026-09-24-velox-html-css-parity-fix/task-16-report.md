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
