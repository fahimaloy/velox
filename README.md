# Velox Framework

Velox is a modular Rust UI framework for building reactive, component-driven desktop apps.

It provides:
- **SFC compiler** for `.vx` components (`<template>`, `<script setup>`, `<style>`)
- **Virtual DOM + layout engine**
- **CSS parser/cascade engine**
- **Renderer backends** (`wgpu`, `skia-native`)
- **CLI** for scaffolding, building, linting, and running apps

---

## Workspace Crates

- `velox-core` — reactive primitives (signals/effects/lifecycle)
- `velox-sfc` — SFC parsing + template codegen + component resolver
- `velox-dom` — VNode types, diffing, layout
- `velox-style` — CSS parsing, selectors, style application
- `velox-renderer` — rendering + events (`wgpu`, `skia-native`)
- `velox-cli` — developer workflow commands

Examples:
- `examples/counter-app`
- `examples/todo`
- `examples/gallery`
- `examples/myapp`
- `examples/vx_demo`
- `examples/interactive_skia`

---

## Current Feature Coverage (v0.1 line)

### SFC / Compiler
- `.vx` single-file components
- `<script setup>` Rust logic and state
- Template interpolation: `{{ expr }}`
- Conditionals: `v-if`, `v-else-if`, `v-else`
- Component import resolution

### Styling
- CSS parsing + application to VNode tree
- Text styling: `font-size`, `font-weight`, `line-height`, `text-decoration`, `font-style`
- Visual effects: border radius, shadows, opacity
- Basic selector support and inline style synthesis

### DOM / Layout
- VNode diffing and patching
- Block layout with sizing/margins/padding
- Positioning support: `static | relative | absolute | fixed | sticky`
- Z-index / stacking context behavior
- Overflow clipping and scroll offsets

### Renderer / Events
- Event binding for common UI events (`click`, `input`, `change`, keyboard/mouse variants)
- Hit testing aligned with stacking/render order
- Backend support:
  - `wgpu` (GPU path)
  - `skia-native` (native Skia path)

### CLI
- `velox init <name|path>`
- `velox build`
- `velox dev`
- `velox run`
- `velox lint`
- `velox version`

---

## Exact Use Cases

### 1) Create a New Velox App

```bash
velox init myapp
cd myapp
cargo build
cargo run
```

Also supports absolute/relative paths:

```bash
velox init /tmp/myapp
```

### 2) Build a `.vx` Component to Rust

```bash
velox build src/App.vx --out-dir target/velox-gen
```

### 3) Lint `.vx` Files During Development

```bash
velox lint src
```

### 4) Run Development Mode (watch/reload workflow)

```bash
velox dev
```

### 5) Run the Counter App Example

```bash
cd examples/counter-app
cargo run
```

### 6) Validate Whole Workspace Before Shipping

```bash
cargo test --workspace --all-targets
cargo build --workspace --release
```

---

## Build & Test Commands (Project Root)

```bash
# Build everything
cargo build --workspace

# Full test suite
cargo test --workspace --all-targets

# Build renderer with wgpu backend
cargo build -p velox-renderer --features wgpu

# Build renderer with native skia backend
cargo build -p velox-renderer --features skia-native
```

---

## Template Syntax (Canonical)

Use canonical directives in templates:
- `v-if`
- `v-else-if`
- `v-else`

Example:

```html
<template>
  <div>
    <p v-if="count > 0">Positive</p>
    <p v-else-if="count == 0">Zero</p>
    <p v-else>Negative</p>
  </div>
</template>
```

---

## CLI Installation

### From this workspace

```bash
cargo install --path velox-cli --force
```

Then verify:

```bash
velox version
```

---

## Recommended Release Gate (before publishing)

Run this checklist:

1. `cargo test --workspace --all-targets`
2. `cargo build --workspace --release`
3. `velox init <tmp-app>` smoke test
4. `velox lint src` in scaffolded app
5. Build/run `examples/counter-app`
6. Final manual review of docs/examples

---

## Notes

- This repository is currently in active stabilization for the first stable release line.
- Manual verification and targeted debug passes are recommended before tagging a release.
