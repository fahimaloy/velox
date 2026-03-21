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

## Installing Velox CLI

### Option 1: From Source (Recommended for Development)

Clone the repository and build the CLI:

```bash
git clone https://github.com/fahimaloy/velox.git
cd velox
cargo install --path velox-cli --force
```

This installs the `velox` binary globally to your Cargo bin directory (`~/.cargo/bin`).

Verify installation:

```bash
velox version
```

You should see output like:
```
velox 0.1.0
Edition: 2021
Platform: Linux
```

### Option 2: Using `cargo` Directly (Without Global Install)

If you want to run the CLI without installing it globally, use:

```bash
cd velox
cargo run -p velox-cli -- <command>
```

For example:
```bash
cargo run -p velox-cli -- init myapp
cargo run -p velox-cli -- version
```

### Verifying Your Installation

After either installation method, verify the CLI is available and functional:

```bash
# Show help
velox --help

# Show version
velox version

# Create a test project
velox init test-project
```

---

## Using the Velox CLI

The CLI provides the following commands:

### 1. `velox init <name|path>`

Create a new Velox project with scaffolding.

**Syntax:**
```bash
velox init <project-name>
velox init /path/to/project
```

**Examples:**
```bash
# Create project in current directory
velox init myapp

# Create project at absolute path
velox init /tmp/myapp

# Create project at relative path
velox init ../projects/myapp
```

**What it generates:**
- `Cargo.toml` with Velox dependencies
- `src/App.vx` — the main component (SFC format)
- `src/main.rs` — Rust entry point
- `build.rs` — build script
- `README.md` — project instructions
- `assets/` directory for images/resources

**Next steps after init:**
```bash
cd myapp
velox dev      # Start development server
cargo run      # Build and run
```

---

### 2. `velox build <input> [options]`

Compile a `.vx` Single File Component to Rust code.

**Syntax:**
```bash
velox build <path-to-file.vx> -o <output-dir>
```

**Options:**
- `-o, --out-dir <PATH>` — Output directory (default: `target/velox-gen`)

**Examples:**
```bash
# Build a component to default output
velox build src/App.vx

# Build with custom output directory
velox build src/App.vx -o my_generated

# Build from scaffolded example
velox build examples/counter-app/src/App.vx
```

**Output:**
Generates Rust code (e.g., `App.rs`) that can be `include!` macro'd into your project.

---

### 3. `velox dev [options]`

Start a development server with hot reload.

**Syntax:**
```bash
velox dev [--watch <directory>]
```

**Options:**
- `-w, --watch <PATH>` — Watch directory for changes (default: `src`)

**Examples:**
```bash
# Watch src/ and reload on changes
velox dev

# Watch a custom directory
velox dev --watch assets
```

**Workflow:**
- File changes are detected automatically
- App rebuilds and restarts
- Press `r` to manually reload
- Press `q` to quit

---

### 4. `velox run [options]`

Build and run a Velox project.

**Syntax:**
```bash
velox run [--release]
```

**Options:**
- `--release` — Build in release mode (optimized, slower compile)

**Examples:**
```bash
# Debug build (faster compile, slower runtime)
velox run

# Release build (slower compile, faster runtime)
velox run --release
```

---

### 5. `velox lint [target]`

Check `.vx` files for syntax errors and issues.

**Syntax:**
```bash
velox lint [<file-or-directory>]
```

**Examples:**
```bash
# Lint all .vx files in src/
velox lint

# Lint a specific file
velox lint src/App.vx

# Lint a custom directory
velox lint components/
```

**Output:**
Reports parse errors, component issues, and style problems.

---

### 6. `velox version`

Display CLI version, edition, and platform information.

**Examples:**
```bash
velox version
```

**Output:**
```
velox 0.1.0
Edition: 2021
Platform: Linux
```

---

## Complete Workflow Example

### Create and Run a New App

```bash
# 1. Create project
velox init myapp
cd myapp

# 2. Start dev server (watches src/)
velox dev

# 3. In another terminal, edit src/App.vx
# Changes auto-reload in the running app

# 4. To build for distribution
velox run --release
```

### Working with Components

```bash
# 1. Create a new component
echo '
<template>
  <div class="button">
    <p>{{ label }}</p>
  </div>
</template>

<script setup>
pub fn new() {
    // logic here
}
</script>

<style>
.button {
  padding: 10px 20px;
  border: 1px solid #ccc;
}
</style>
' > src/Button.vx

# 2. Lint the component
velox lint src/Button.vx

# 3. Build to Rust
velox build src/Button.vx -o target/components

# 4. Dev server auto-picks up changes
velox dev
```

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
