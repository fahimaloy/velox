Velox Framework (v0.1.1)

Overview
- Modular Rust workspace for building reactive, component‑driven desktop apps with GPU rendering.
- Single File Components (SFCs) in `.vx` (or `.vue`) compile to Rust VNode trees and styles.
- Renderer backends are feature‑gated; default build is a fast stub, `wgpu` enables a GPU window.

Workspace
- velox-core: reactive primitives (signals, effects, lifecycle)
- velox-sfc: SFC parsing + template/codegen
- velox-dom: VNode tree + diffing + layout
- velox-style: CSS parsing, cascading, selectors, inline style synthesis
- velox-renderer: render VNode trees; backends: `wgpu` (and a stub)
- velox-cli: CLI for compiling SFCs and scaffolding/running apps
- examples/: example apps scaffolded via CLI

What’s New in 0.1.1
- Layout: `%` width/height, margin/padding per side, full‑window root sizing.
- Styles: cascaded/inherited text props (font-size, font-weight 100–1000, line-height, text-decoration underline/overline/line-through, italic).
- Renderer: draws text from VNode + stylesheet (no placeholders); correct text‑decoration placement; clipping inside element boxes; extended vertical bounds for non‑button text; basic text-align and font-family selection.
- Events: multiple `@click` targets with proper hit testing.
- CLI: init template with hex colors and demo text styles; includes increment/decrement handlers.

Build
- Build workspace: `cargo build --workspace`
- Enable GPU renderer: `cargo build -p velox-renderer --features wgpu`
- Lint/format: `cargo clippy --workspace -- -D warnings`, `cargo fmt`

Tests
- All crates: `cargo test --workspace`
- Specific crate: `cargo test -p velox-sfc`

CLI
- Compile SFC to Rust (stub):
  `cargo run -p velox-cli -- build examples/todo/src/App.vx --emit stub`
- Compile SFC and render:
  `cargo run -p velox-cli -- build examples/todo/src/App.vx --emit render --out-dir target/velox-gen`
- Scaffold new app:
  `cargo run -p velox-cli -- init myapp`
- Dev server (restarts on file changes):
  `cargo run -p velox-cli -- dev myapp`

Example Run
- After `init myapp`:
  - `cargo run -p velox-cli -- dev myapp`
  - Edit `examples/myapp/src/App.vx` to experiment with styles (`font-size`, `text-decoration`, `line-height`, `:hover`), and events (`@click`).

Design Notes
- SFC `<template>` becomes a VNode tree; `<style>` is parsed and cascaded into inline styles during render (with hover predicate support); `<script setup>` holds Rust state/logic.
- Layout is a simple block model to support rapid iteration; it can be extended toward flex/grid if needed.
- Renderer consumes the VNode + inline styles only; no hardcoded UI.

Contributing
- Keep changes small and focused; add tests under the crate you modify.
- Ensure `cargo fmt`, `cargo clippy` (no warnings), and tests pass before opening a PR.

Directive Normalization (SFC templates)
- Template directive attributes must start with `v-` (for example `v-if`, `v-else`, `v-for`).
- The parser normalizes directive names by stripping the leading `v-` and converting the remainder to kebab-case before storing it in the AST. That means the following variants are accepted and normalized:
  - `v-if` -> `if`
  - `v-else` -> `else`
  - `v-else-if`, `v-elseIf`, `v_elseif` -> `else-if`
  - `v-elseif` -> `elseif` (kept as-is after normalization)
- Normalization rules: underscores become `-`, uppercase letters are converted to lowercase and prefixed with `-` when appropriate, consecutive dashes are collapsed, and any surrounding dashes are trimmed. The AST stores directive names without the `v-` prefix (e.g. `if`, `else-if`).
- Codegen currently supports `v-if` with chained `v-else-if` and a final `v-else` sibling. Use these variants interchangeably in templates; the parser will normalize them before codegen.

If you want the docs to prefer a canonical style, use `v-if`, `v-else-if`, and `v-else` in examples — these map directly to the normalized directive names used by the compiler.

