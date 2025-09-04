# Repository Guidelines

## Project Structure & Module Organization
- velox-core: reactive primitives (signals, effects, lifecycle). Tests in `velox-core/tests`.
- velox-sfc: parses `.vx/.vue` Single File Components and codegens Rust. Tests in `velox-sfc/tests`.
- velox-dom: minimal DOM/runtime types.
- velox-renderer: optional rendering backends (features: `wgpu`, `skia`).
- velox-style: CSS parsing/selectors.
- velox-cli: command-line tool to compile SFCs.
- examples/: runnable crates (e.g., `todo`, `gallery`).

## Build, Test, and Development Commands
- Build workspace: `cargo build --workspace`
- Run tests (all): `cargo test --workspace`
- Run crate tests: `cargo test -p velox-sfc`
- Format code: `cargo fmt` (check CI-style: `cargo fmt -- --check`)
- Lint: `cargo clippy --workspace -- -D warnings`
- Run examples: `cargo run -p todo` or `cargo run -p gallery`
- Use CLI to compile SFC:
  - Stub: `cargo run -p velox-cli -- build examples/todo/src/App.vx --emit stub`
  - Render: `cargo run -p velox-cli -- build examples/todo/src/App.vx --emit render --out-dir target/velox-gen`

## Coding Style & Naming Conventions
- Rust edition: 2021/2024 per-crate; use stable toolchain.
- Indentation: 4 spaces; keep lines concise.
- Naming: `snake_case` for funcs/modules, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for consts.
- Modules: one logical unit per file; prefer small, focused modules.
- SFC files end with `.vx` (or `.vue`); component filenames use `PascalCase` (e.g., `App.vx`).

## Testing Guidelines
- Use Rust’s built-in test framework (`#[test]`, `tests/` integration tests).
- Place crate-specific tests under `cratename/tests` and focused unit tests next to code.
- Add tests for new parsing rules, codegen, and reactive behavior; avoid flaky timing-based tests.
- Run: `cargo test -p <crate>` and `cargo test --workspace` before opening a PR.

## Commit & Pull Request Guidelines
- Commit style: Conventional Commits with scopes (e.g., `feat(sfc): add dynamic attrs`, `fix(core): re-entrancy in signals`).
- PRs must include: clear description, rationale, linked issues, and tests updated/added. For renderer changes, include screenshots or notes on backend/features used.
- Ensure green checks: `cargo fmt -- --check`, `cargo clippy -- -D warnings`, and full test pass.
- Prohibited terms: Do not include the words "AI" or "Codex" in commit messages, PR titles/descriptions, code comments, or documentation. Use neutral phrasing like "automation" or the specific tool name instead.
 - Local enforcement: Enable repository hooks to reject messages containing these terms:
   - `git config core.hooksPath .githooks`

## Security & Configuration Tips (Optional)
- Do not commit secrets. Heavy renderer deps are feature-gated; enable explicitly: `cargo build -p velox-renderer --features wgpu`.
- Generated code defaults to `target/velox-gen/`; add that path to tools/scripts, not to VCS.
