# Changelog Policy

- Use Keep a Changelog style with semantic versioning when publishing.
- Every user-visible change must have an entry under Added, Changed, Fixed, or Removed.
- Link PRs/issues when possible; avoid prohibited terms in entries.
- Unreleased section accumulates changes until the next version bump.

## Unreleased

### Fixed
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

### Changed
- Removed `syn` and `quote` dependencies from `velox-sfc` (were unused)
- Removed `examples/` directory (outdated code); replaced with clean templates
- Simplified project template `App.vx` to self-contained counter example
- Simplified `Counter.vx` component template
- Updated `test_project/src/App.vx` with clean stable syntax
- Added `rustfmt.toml` with project formatting settings
- Added `clippy.toml` with project linting thresholds

### Added
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
- Total test count: 263 tests (up from ~98)

### Removed
- Removed `examples/` directory with outdated example projects (`todo`, `gallery`, `myapp`, `interactive_skia`, `vx_demo`)

