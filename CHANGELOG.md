# Changelog Policy

- Use Keep a Changelog style with semantic versioning when publishing.
- Every user-visible change must have an entry under Added, Changed, Fixed, or Removed.
- Link PRs/issues when possible; avoid prohibited terms in entries.
- Unreleased section accumulates changes until the next version bump.

## Unreleased

### Fixed
- `velox dev` no longer quantises saves to a 400 ms floor: the watcher is
  `notify`/inotify instead of a full-tree `read_dir` scan on a timer, so a save is
  seen in single-digit milliseconds with no tree walk in the path
- `font-weight: bolder` now renders bold. The text-style parser matched only the
  literal `"bold"` or a numeric weight `>= 700`, so the relative-bold keyword
  fell through and the declaration was silently dropped. `lighter` is
  deliberately still non-bold: the render path carries a single `bold: bool`, so
  mapping it to bold would be a new lie rather than a fix
- `font-style` is no longer dropped twice. `velox-style` lists it in
  `INHERITABLE`, so the cascade propagated it to children, and then
  `ComputedStyle::set_property` threw it away for want of a match arm
- `border: 1px` no longer fabricates a `solid` style. CSS 2.1 §8.5.2 makes `none`
  the initial `border-style`, so a width with no style keyword paints nothing.
  This changes no pixels — the renderer never called this function — but the two
  halves of the same grammar had disagreed, and the DOM was the wrong half
- The renderer's `border` shorthand parser now delegates to the DOM's own grammar
  instead of re-implementing a subset of it. `border: 1px dashed` and
  `border: 2em solid red` previously painted **no border at all**, because the
  local parser honoured only a literal `px` suffix and a literal `solid`. Lengths
  now resolve `px`/unitless/`em`/`rem`/`vw`/`vh`, all eight border-style keywords
  survive (with a real Skia dash path effect for `dashed`/`dotted`), and colour
  parsing picks up 7 named colours and 3-digit hex
- `font-size` resolves relative units. `2em`, `1.5rem`, `150%` and `5vw` all
  failed every parser and were dropped with no warning, so the UA stylesheet's own
  `h1 { font-size: 2em }` laid the box out at 32px while painting the glyphs at
  the inherited size — the whole `h1`–`h6` hierarchy rendered flat. Sizing now
  goes through the same unit table as `border`, with the relative basis taken
  from the parent's font size per CSS 2.1 §6.7
- Fixed `watch_tests.rs` compilation errors: updated test API to match new `watch()` signature (added `WatchOptions`, callback receives owned `T` instead of `&T`)
- Removed all 36 compiler warnings across all crates:
  - `velox-renderer`: removed unused imports (`Arc`, `Mutex`, `Receiver`, `thread`, `EventLoop`, `Stylesheet`), prefixed unused vars with underscore
  - `velox-cli`: removed unused `Arc`, `Mutex` imports; prefixed unused `app_name` field
  - `velox-dom`: prefixed unused variables (`wrap_reverse`, `explicit_w`, `half_gap`), removed unnecessary `mut` from `ln` variable
  - `velox-core`: removed unnecessary `mut` from `watch_effect` parameter
  - `velox-sfc`: added `#[allow(dead_code)]` to utility functions used only by tests
- Fixed Rust 2024 edition match ergonomics: replaced `ref`/`ref mut` patterns with compatible alternatives in `template_parse.rs`
- Fixed clippy `manual_strip` warnings: used `strip_prefix()` instead of manual slicing
- Fixed clippy `collapsible_match`: collapsed nested `if let` in style parsing
- Fixed clippy `ptr_arg`: changed `&Vec<VNode>` to `&[VNode]` in `reconcile_keyed_children`
- Fixed clippy `should_implement_trait`: added `#[allow]` for intentional `from_str` methods
- Fixed clippy `too_many_arguments`: added `#[allow]` for internal layout functions
- Fixed clippy `type_complexity`: added type aliases for effect closures in `velox-core`
- Fixed clippy `needless_borrow`: removed unnecessary borrows in template parser
- Updated build tests to use template files instead of removed examples directory

### Removed
- Removed `velox_renderer::reconcile_keyed_children`, which had no production callers. On a key match it pushed the previous-frame node and discarded the incoming content, so a keyed child whose text or attributes changed kept stale content. `:key` semantics and what would revisit them are now written down in `docs/RECONCILER.md`

### Changed
- `velox dev` watching is now constructible from a test and non-fatal on failure.
  `DirWatcher::start` returns a handle even when it cannot watch, reporting via
  `DevCmd::WatchError` instead of failing: a dev server that exits on a watcher
  problem is worse than one that says so and keeps running. Saves are
  coalesced by a `ChangeDebouncer` deadline that blocks for exactly the remaining
  window and still wakes early for a keypress or a finished build — the change
  path never sleeps
- `velox dev --watch` documentation corrected: the default root is the current
  directory, not `src/`; `--watch` is single-valued, so `--watch src --watch
  assets` was a clap error rather than merely inaccurate; and both inotify
  `sysctl` limits, plus the step to persist them, are now documented
- CI now runs on **every push and every pull request, on every branch**. The
  previous trigger was `branches: [ main, master, alpha ]`, so no feature branch
  ran a single check — including the branch all of this cycle's work landed on
- CI gains three jobs: a coverage run (`cargo llvm-cov --workspace
  --summary-only`), a `cargo audit` dependency-vulnerability job with five
  recorded RUSTSEC waivers, and a proptest job. The proptest job probes for a
  `proptest!` macro and installs `cargo-proptest` only if it finds one — this
  repository has no `proptest` dependency in any `Cargo.toml`, in this tree or
  anywhere in its history, so that job is currently a no-op. Its probe now also
  searches `examples/`, which it previously omitted, so a `proptest!` added to
  an example crate's tests actually turns the job on
- The coverage job's `--fail-under 0` was a green box enforcing nothing. A
  baseline has now been measured (`cargo llvm-cov --workspace --summary-only`:
  **77.52% lines**, 77.21% regions) and the gate set to `--fail-under 76`,
  deliberately below the measured value so ordinary changes do not redden CI.
  The run also takes `--no-fail-fast`, because a target whose tests fail is
  excluded from the total — a red build silently lowers the denominator
- Two claims in this repository's own documentation were corrected against the
  code: the README described a virtual DOM with diffing and efficient patching,
  which the render loop does not do (it is immediate-mode, with a complete but
  caller-less keyed reconciler in `velox-dom`), and it described `velox dev` as
  hot reload, which it is not (every change kind, including a `<style>` edit,
  takes a full `cargo build` and a process restart)
- Removed `syn` and `quote` dependencies from `velox-sfc` (were unused)
- Simplified project template `App.vx` to self-contained counter example
- Simplified `Counter.vx` component template
- Updated `test_project/src/App.vx` with clean stable syntax
- Added `rustfmt.toml` with project formatting settings
- Added `clippy.toml` with project linting thresholds

### Added
- `velox lint` reports every CSS declaration that is parsed and then dropped
  before it reaches a pixel — `box-shadow`, `font-style`, `letter-spacing`,
  `visibility`, `overflow-x`, `overflow-y`, `background-image`, `transition`,
  `border-style`, `border-color`, and `transform` (read only to force a stacking
  context, never applied as a visual transform). It reports *these* declarations
  and not "anything unmatched", because the cascade filters genuinely-unknown
  properties out silently and flagging those would fire on every vendor prefix,
  custom property, and deliberately-declined property. The table is kept honest
  by two tests: every member must still have a `set_property` arm, and every
  cited arm line must still point at it
- VS Code extension: syntax highlighting for `.vx` files (TextMate grammar)
- Zed extension: language support configuration and highlight queries
- Neovim syntax: syntax highlighting and file type detection for `.vx` files
- `rustfmt.toml`: project-wide formatting configuration
- `clippy.toml`: project-wide clippy configuration (type complexity threshold, argument threshold)
- 165 new test cases across 9 new test files:
  - `velox-sfc`: v-for parsing/codegen, nested templates, error cases, integration pipeline tests
  - `velox-core`: provide/inject, next_tick, signal edge cases (multiple effects, type variants)
  - `velox-dom`: diff edge cases, keyed children, large tree performance, props diff
  - `velox-renderer`: event lifecycle, dispatch, reconciliation integration
- Total test count: 205 tests across all crates

### Removed
- Removed outdated example projects (`gallery`, `myapp`, `interactive_skia`, `vx_demo`); retained `counter` and `todo`

## 0.1.1 - 2026-10-04

### Added
- Per-crate `readme = "README.md"` in the `[package]` section of every published
  crate (`velox-core`, `velox-dom`, `velox-style`, `velox-renderer`, `velox-sfc`,
  `veloxc`), so crates.io renders the README on each crate's version page

### Changed
- `velox-cli` → `veloxc` rename completed across the workspace (c11dd60): crate,
  binary, CI, docs, examples and scaffold output all use the single name `veloxc`

### Fixed
- Corrected the logo glyph to a single Rust-style mark

### Removed
- `.opencode/` is no longer tracked by git and is gitignored (6092341)
