# Contributing to Velox

Thank you for your interest in contributing to Velox! This document covers everything you need to get started.

## Dev Setup

**Requirements:**

- Rust 1.85+ (2024 edition) — install via [rustup](https://rustup.rs)
- cargo (bundled with Rust)
- Linux recommended for the full renderer feature set (the native Skia GPU path is unix-only)

**Getting started:**

```bash
git clone https://github.com/fahimaloy/velox.git
cd velox
cargo build          # build the workspace
cargo test           # run the test suite
```

## Workspace Layout

Velox is a Cargo workspace with the following crates at the repo root:

| Crate | Purpose |
| --- | --- |
| `velox-core` | Core reactive primitives (signals, effects, reactivity) |
| `velox-sfc` | `.vx` single-file component compiler |
| `velox-dom` | DOM abstraction and tree types |
| `velox-renderer` | Rendering backends (Skia) |
| `velox-style` | CSS parsing and style resolution |
| `veloxc` | The `veloxc` CLI binary |

Examples live under `examples/` (`counter`, `todo`, `showcase`). Project documentation is built with mdbook from `docs/`.

## Pull Request Flow

1. Fork the repo and create a feature branch from `main`.
2. Make your changes with tests.
3. Run the checks locally (see below).
4. Open a pull request against `main`.
5. CI must pass before a PR can be merged.

### CI Checks

CI runs on every push and every pull request. The gate jobs are:

- **build-test** — format check (`cargo fmt`), clippy with `-D warnings`, and the workspace test suite.
- **coverage** — line coverage via `cargo llvm-cov` with a minimum floor.
- **dependency-audit** — `cargo audit` against a recorded advisory baseline.
- **property-tests** — property-based tests (auto-enabled when a `proptest!` block exists in the workspace).

Additional jobs cover renderer feature configurations, the non-unix build, and the Docker test image.

### Local Checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Commit Conventions

- Use clear, imperative commit messages (e.g. "Add style-resolution caching", "Fix SFC parser offset bug").
- Keep commits focused; one logical change per commit.
- Squash or merge commits are both accepted on merge; rebase merges are disabled.

## Reporting Issues

Please use the issue templates (bug report or feature request). Blank issues are disabled so reports stay actionable.

## Contact

- Maintainer: FAHIM AHMED <fahimaloy@tutamail.com>
- Security issues: see [SECURITY.md](SECURITY.md) — please do not open public issues for security reports.
