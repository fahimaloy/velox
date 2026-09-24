# Task 17 Report — Suppress collapsible whitespace-only line boxes in block flow

## Summary

Removed collapsible whitespace-only `Text` nodes from the block-flow child loop when they occur at a block formatting boundary. Whitespace between inline-level participants remains in the layout tree so a future inline formatting-context implementation can retain the collapsed space. No parser or text-wrapping changes were made, and the existing flex path was left unchanged.

## Root cause confirmation

The SFC parser normalizes nested whitespace runs to a text node, and the flex child collector already skips those nodes. The block child loop did not skip them, so `Text(" ")` entered the text layout branch. The text wrapper's empty-line fallback then produced a line box with normal line height, which offset the following block child.

The added boundary tests confirmed the defect before the implementation: the block boundary case had three children instead of two, while the inline negative-control case already passed.

## Changes

- Added private formatting-context helpers in `velox-dom/src/layout.rs`:
  - classify explicit inline-level displays (`inline`, `inline-block`, `inline-flex`, and `inline-grid`);
  - recognize the default inline-level HTML element tags used by this engine, including `span` and `a`;
  - ignore out-of-flow (`absolute`/`fixed`) and `display:none` elements when looking for neighboring formatting participants;
  - treat only `white-space: normal` and `white-space: nowrap` as collapsible for this boundary decision.
- Updated only the block-flow child loop. A whitespace-only text node is skipped only when it is collapsible and at the start/end of the block formatting context or between non-inline participants. The node is omitted entirely rather than emitted with a zero-height box.
- Left the flex whitespace guard, `velox-dom/src/text_wrap.rs`, and the SFC parser unchanged.
- Added tests in `velox-dom/tests/layout_tests.rs`:
  - `block_boundary_collapses_whitespace_only_text`;
  - `block_boundary_preserves_whitespace_between_inline_participants`.

## Tests

### TDD pre-fix failure

Command:

```text
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-dom --test layout_tests
```

Output before the implementation:

```text
running 25 tests
...
test block_boundary_collapses_whitespace_only_text ... FAILED

---- block_boundary_collapses_whitespace_only_text stdout ----
thread 'block_boundary_collapses_whitespace_only_text' panicked at velox-dom/tests/layout_tests.rs:55:5:
assertion `left == right` failed
  left: 3
 right: 2

failures:
    block_boundary_collapses_whitespace_only_text

test result: FAILED. 24 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

The negative-control test `block_boundary_preserves_whitespace_between_inline_participants` passed before the implementation, proving the regression was isolated to block-boundary whitespace suppression.

### Post-fix targeted pass

The same command after implementation produced:

```text
running 25 tests
test block_boundary_collapses_whitespace_only_text ... ok
test block_boundary_preserves_whitespace_between_inline_participants ... ok
...
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Verification

- Command: `cargo fmt`
  - Result: passed.
- Command: `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-dom`
  - Result: passed completely: 143 tests across the unit, integration, and doc-test runs, with 0 failures and 0 ignored. Existing flex, box-sizing, layout, golden, overflow, margin-collapse, and text-wrap tests remained green.
- Command: `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p velox-example-counter -p velox-example-todo -p velox-example-showcase`
  - Result: passed. Counter: 2/2 proof tests; showcase: 2/2 proof tests; todo: 3/3 proof tests. Existing proof outputs remain green and unchanged.
- Command: `git diff --check`
  - Result: passed.

## Concerns and deferred items

- This task intentionally does not implement an inline formatting context. Inline-level elements and retained whitespace still use the engine's existing block-loop layout behavior until that separate slice is implemented.
- The boundary classifier uses the engine's existing display/tag inputs. A fully computed CSS display value, including all UA and author cascade sources, is not expanded in this DOM-only change.
- `velox-dom/src/text_wrap.rs` and `velox-sfc/src/template_parse.rs` were not modified, so non-collapsible `white-space` handling remains owned by the existing text wrapper.
- No public API, dependency, MSRV, parser, text-wrapper, or flex behavior changes were made.
