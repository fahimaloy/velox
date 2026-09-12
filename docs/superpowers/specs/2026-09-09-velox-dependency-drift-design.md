# Velox Dependency-Drift Design — 2026-09-09

## Context
- `velox init` outside workspace (e.g. /tmp/test_app) emits unpinned `git = "https://github.com/fahimaloy/velox"` deps (init.rs:267-288). Floats on HEAD, unreproducible.
- Path branch (init.rs:235-266) uses bare `path` without `version`, fragile relative paths via `compute_relative_path` + `canonicalize`.
- Dead `templates/project/Cargo.toml.template` (mustache `{{...}}`) vs live `format!()` + `include_str!` duplication guarantees drift.
- Working tree: 40 files, +4209/-999, all uncommitted; HEAD==origin/alpha==c93c459. `/tmp/test_app` matches NEW template, old rev breaks (missing serde_json/skia-native, hmr.rs, diagnostic.rs, build-dep velox-sfc→velox-cli switch, +1085 template_codegen).
- Case fix (App.rs/app.rs alias in lib.rs + build.rs) exists locally, not pushed.

Decision: Approach A — pinned git + path override now, crates.io later (user-approved).

## Architecture
- Single source of truth for scaffolding: `velox-cli/src/commands/init.rs` + `templates/project/{build.rs,src/main.rs,src/App.vx}`. Delete or generate `Cargo.toml.template` from code (no dual maintenance).
- Dependency resolution order for new projects:
  1. `--local <path>` flag or `VELOX_PATH` env if set → `path` + `version` bound.
  2. `find_velox_workspace()` hit → `path` + `version` bound (computed relative, verified exists).
  3. Fallback → `git` + `rev=<current CLI rev>` + `version="0.1.0"` bound.
- CLI embeds its own rev at compile time via `env!("CARGO_PKG_VERSION")` + `option_env!("VELOX_GIT_REV")` (build.rs writes git rev) so generated `Cargo.toml` pins what was actually used.

## Components
1. `init.rs: generate_cargo_toml()` — add `rev`, `version`, env/flag override, keep `[workspace]` + `skia-native` + `serde_json` + build-dep `velox-cli`.
2. `init.rs: find_velox_workspace()/compute_relative_path()` — harden: verify candidate files exist, handle non-canonicalizable paths, tests for /tmp vs in-workspace vs moved dir.
3. Template dedup — remove dead `.template` or make it generated; `generate_*` functions read from `templates/project/` only.
4. Codegen case-safety — keep sanitized `app.rs` primary + raw-case alias (already in lib.rs:84-97, build.rs:294-302); add unit test asserting both exist.
5. Release hygiene — commit + push 40-file drift on `alpha`; `velox --version` prints version+rev; README notes `cargo update -p velox-*` after push.

## Data flow
`velox init <name> [--local <path>]` → detect workspace/env → render Cargo.toml (pinned) → copy build.rs/main.rs/App.vx/components → `cargo build` in new project uses pinned CLI rev → build script writes `OUT_DIR/app.rs` (+ alias) → `include!` resolves on case-sensitive Linux.

## Error handling
- Missing `VELOX_PATH` dir → clear error, fall back to git pin (no silent float).
- Relative-path failure → absolute path fallback + warning, never broken `../../` string.
- Old pinned rev checked out → build script API mismatch surfaces as `velox-cli build_cmd not found`; README troubleshooting points to `cargo update`.

## Testing
- `cargo test -p velox-cli`: init matrix (in-workspace /tmp/outside, VELOX_PATH set/unset, --local), assert Cargo.toml contains `rev=` + `version=`, assert no `{{...}}` placeholders.
- Codegen alias test: compile `App.vx` via Render + Stub, assert `app.rs` + `App.rs` both exist with `pub mod app`.
- Manual: fresh `velox init` in /tmp from working-tree binary, `cargo build`, `timeout 20 velox run` → `Starting ...` with no `couldn't read .../app.rs`.
- Skip: GUI pixel QA, `cargo run --example` (needs display).

## Out of scope
- Crates.io publish (deferred to later per user).
- Changing `run_current` to do codegen (out of scope; build script owns it).
- Refactoring renderer/core/style (only template/init/build paths touched).
