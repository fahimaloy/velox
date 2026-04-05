# Velox Framework

Velox is a modular Rust UI framework for building reactive, component-driven desktop apps.

It provides:
- **SFC compiler** for `.vx` components (`<template>`, `<script setup>`, `<style>`)
- **Virtual DOM + layout engine**
- **CSS parser/cascade engine**
- **Renderer backends** (`wgpu`, `skia-native`)
- **CLI** for scaffolding, building, linting, and running apps

---

## Quick Start

1. [Prerequisites](#prerequisites)
2. [Build Everything](#build-everything)
3. [Install Velox CLI](#install-velox-cli)
4. [Verify Installation](#verify-installation)
5. [Next Steps](#next-steps)

---

## Prerequisites

Before starting, ensure you have:

- **Rust 1.70+** — Install from [rustup.rs](https://rustup.rs)
  ```bash
  rustup --version
  cargo --version
  ```
- **Git** — For cloning the repository
- **Build tools** — Required by Rust:
  - Linux: `build-essential`, `pkg-config`, `libssl-dev`
  - macOS: Xcode Command Line Tools (`xcode-select --install`)
  - Windows: Microsoft C++ build tools

---

## Workspace Structure

The Velox repository is organized into modular crates:

- **`velox-core`** — Reactive primitives (signals/effects/lifecycle)
- **`velox-sfc`** — SFC parsing + template codegen + component resolver
- **`velox-dom`** — VNode types, diffing, layout
- **`velox-style`** — CSS parsing, selectors, style application
- **`velox-renderer`** — Rendering + events (`wgpu`, `skia-native` backends)
- **`velox-cli`** — Developer workflow commands

Plus examples:
- `examples/counter-app`
- `examples/todo`
- `examples/gallery`
- `examples/myapp`
- `examples/vx_demo`
- `examples/interactive_skia`

---

## Build Everything

### Step 1: Clone the Repository

```bash
git clone https://github.com/fahimaloy/velox.git
cd velox
```

### Step 2: Build All Crates

Build the entire workspace with all tests:

```bash
# Build everything (debug mode - faster compile)
cargo build --workspace

# Build everything in release mode (slower compile, faster runtime)
cargo build --workspace --release
```

### Step 3: Run Tests (Optional)

Ensure everything compiles and tests pass:

```bash
# Run full test suite across all crates
cargo test --workspace --all-targets
```

### Build Specific Backends (Optional)

The renderer supports multiple backends:

```bash
# Build with wgpu backend (GPU rendering)
cargo build -p velox-renderer --features wgpu

# Build with native skia backend
cargo build -p velox-renderer --features skia-native

# Build both
cargo build -p velox-renderer --all-features
```

---

## Install Velox CLI

After building, install the Velox CLI globally:

### Option 1: Install from Local Source (Recommended for Development)

Install from your cloned repository:

```bash
cd /path/to/velox
cargo install --path velox-cli --force
```

This installs the `velox` binary to your Cargo bin directory. The binary path is:

```
~/.cargo/bin/velox
```

The `~/.cargo/bin` directory should already be in your `$PATH`. If not, add it:

```bash
# Add to ~/.bashrc, ~/.zshrc, or similar:
export PATH="$HOME/.cargo/bin:$PATH"
```

### Option 2: Run Without Global Install

If you prefer not to install globally, you can run the CLI directly from the repository:

```bash
cd /path/to/velox

# Instead of: velox init myapp
# Use:        cargo run -p velox-cli -- init myapp

# Instead of: velox build src/App.vx
# Use:        cargo run -p velox-cli -- build src/App.vx
```

Add this alias to your shell config for convenience:

```bash
# Add to ~/.bashrc or ~/.zshrc
alias velox='cd /path/to/velox && cargo run -p velox-cli --'
```

---

## Verify Installation

### Check CLI is Available

Run one of these commands (depending on your installation method):

```bash
# If installed globally:
velox version

# If using cargo run alias:
velox version

# Output should be:
# velox 0.1.0
# Edition: 2021
# Platform: Linux (or your OS)
```

### Verify Help Works

```bash
velox --help
```

You should see a list of available commands.

### Quick Smoke Test

Create a test project to ensure everything works:

```bash
velox init test-velox-app
cd test-velox-app

# Try building the scaffolded app
velox build src/App.vx

# Try linting
velox lint src/App.vx
```

If all commands succeed, your installation is complete!

---

## Next Steps

### Path to Use Velox Commands

Depending on your installation method, use:

**If installed globally** (`cargo install`):
```bash
velox init myapp
velox build src/App.vx
velox dev
```

**If using without global install** (using alias recommendation):
```bash
velox init myapp
velox build src/App.vx
velox dev
```

In both cases, you reference paths **relative to where you run the command**, not relative to the Velox repository.

### Your First App

```bash
# 1. Create a new Velox app (runs from anywhere)
velox init my-first-app
cd my-first-app

# View the generated structure:
ls -la
# Cargo.toml, src/App.vx, src/main.rs, build.rs, assets/, etc.

# 2. Lint the scaffolded component
velox lint src/App.vx

# 3. Build the component (generates Rust code)
velox build src/App.vx

# 4. Run the app
cargo run

# Or use the shorthand:
velox run
```

### For Development Workflow

```bash
cd my-first-app

# Start dev server with hot reload (watches src/ by default)
velox dev

# In another terminal, edit src/App.vx and save
# → App reloads automatically

# Press 'r' in the dev terminal to manually reload
# Press 'q' to quit
```

### Run Examples

```bash
# From anywhere, test the counter app example:
velox init temp-counter
cd temp-counter

# Or build an example from the Velox repository:
cd /path/to/velox
velox build examples/counter-app/src/App.vx
cargo run --example counter-app
```

---

## Using the Velox CLI

### Complete Command Reference

#### 1. `velox init <name|path>`

Create a new Velox project with scaffolding.

**Syntax:**
```bash
velox init <project-name>
velox init /path/to/project
velox init ../relative/path
```

**Examples:**
```bash
# Create project in current directory
velox init myapp

# Create at absolute path
velox init /tmp/myapp

# Create at relative path
velox init ../projects/myapp
```

**Generated files:**
- `Cargo.toml` with Velox dependencies
- `src/App.vx` — main component (SFC format)
- `src/main.rs` — Rust entry point
- `build.rs` — build script
- `README.md` — project instructions
- `assets/` — directory for images/resources

**After init:**
```bash
cd myapp
velox dev         # Start with hot reload
velox run         # Build and run once
velox run --release  # Release build
```

---

#### 2. `velox build [input] [options]`

Build the current project by default, or compile a specific `.vx` file to generated Rust source.

**Syntax:**
```bash
velox build
velox build --release
velox build <path-to-file.vx> -o <output-dir>
```

**Options:**
- `--release` — Release build when building the current project
- `-o, --out-dir <PATH>` — Output directory when compiling a single `.vx` file (default: `target/velox-gen`)

**Examples:**
```bash
# Build the entire current project (like vite build / flutter build)
velox build

# Build current project in release mode
velox build --release

# Compile a single component to generated Rust source
velox build src/App.vx

# Build to custom directory
velox build src/App.vx -o my_generated

# Build from examples
velox build examples/counter-app/src/App.vx
```

**Output:**
- `velox build` produces a full Cargo project build (binary in `target/debug` or `target/release`).
- `velox build src/App.vx` generates Rust source (e.g., `App.rs`) for inspection/integration and is not directly executable.

---

#### 3. `velox dev [options]`

Start a development server with hot reload.

**Syntax:**
```bash
velox dev [--watch <directory>]
```

**Options:**
- `-w, --watch <PATH>` — Watch directory for changes (default: `src`)

**Examples:**
```bash
# Watch src/ with auto-reload
velox dev

# Watch custom directory
velox dev --watch assets

# Watch multiple or specific paths
velox dev --watch src --watch assets
```

**Workflow:**
- File changes auto-detected
- App rebuilds and restarts
- Press `r` to manually reload
- Press `q` to quit

---

#### 4. `velox run [options]`

Build and run a Velox project.

**Syntax:**
```bash
velox run [--release]
```

**Options:**
- `--release` — Optimized build (slower compile, faster runtime)

**Examples:**
```bash
# Debug build (faster compile)
velox run

# Release build (faster runtime)
velox run --release

# Watch and rebuild (equivalent to velox dev)
velox dev
```

---

#### 5. `velox lint [target]`

Check `.vx` files for syntax errors and issues.

**Syntax:**
```bash
velox lint [<file-or-directory>]
```

**Examples:**
```bash
# Lint all .vx in current src/
velox lint

# Lint specific file
velox lint src/App.vx

# Lint custom directory
velox lint components/
```

**Output:**
Reports parse errors, component issues, and style problems.

---

#### 6. `velox version`

Display CLI version, edition, and platform.

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

## Workflow Examples

### Complete: Create, Develop, and Release an App

```bash
# 1. Create a new app
velox init myapp
cd myapp

# 2. Start development server (watches src/ by default)
velox dev

# 3. In another terminal, edit src/App.vx
#    Save the file and watch it reload automatically

# 4. When ready, build for distribution
velox run --release

# 5. Your built app is ready in target/release/
./target/release/myapp
```

### Create a Component Library

```bash
# 1. Create a new app
velox init component-lib
cd component-lib

# 2. Create components in src/
cat > src/Button.vx << 'EOF'
<template>
  <div class="button">
    <p>{{ label }}</p>
  </div>
</template>

<script setup>
pub fn new() {
    // Button logic here
}
</script>

<style>
.button {
  padding: 10px 20px;
  border: 1px solid #ccc;
  border-radius: 4px;
}
</style>
EOF

# 3. Lint your components
velox lint src/Button.vx
velox lint src/        # Lint all .vx files

# 4. Build components to Rust
velox build src/Button.vx -o target/components

# 5. Use in your app
# Include the generated Rust files in your project
```

### Development Loop

```bash
# 1. Start your app development
velox init myapp
cd myapp

# 2. In terminal 1: Start dev server
velox dev

# 3. In terminal 2: Edit and test
$ velox lint src/App.vx          # Check syntax
$ velox build src/App.vx         # Rebuild

# 4. Watch terminal 1 for app reload
#    Changes appear automatically

# 5. Kill dev server when done
# (In terminal 1, press q)
```

---

## Development and Testing

### Building the Velox Framework from Source

If you're contributing to Velox or want to modify the framework:

```bash
# From the Velox repository root
cd /path/to/velox

# Build all crates
cargo build --workspace

# Build with optimizations
cargo build --workspace --release

# Run all tests
cargo test --workspace --all-targets

# Build specific renderer backend
cargo build -p velox-renderer --features wgpu
cargo build -p velox-renderer --features skia-native

# Run tests for specific crate
cargo test -p velox-core
cargo test -p velox-sfc
cargo test -p velox-renderer
```

### Running Examples

```bash
cd /path/to/velox

# Build and run the counter app
cargo run --example counter-app

# Build and run the todo app
cargo run --example todo

# Run the gallery
cargo run --example gallery

# Run the demo
cargo run --example vx_demo
```

---

## Supported Features (v0.1 line)

### SFC / Compiler
- `.vx` single-file components with `<template>`, `<script setup>`, `<style>`
- Rust logic and state in component setup
- Template interpolation: `{{ expr }}`
- Conditionals: `v-if`, `v-else-if`, `v-else`
- Component import resolution and composition

### Styling
- CSS parsing with style application to VNode tree
- Text styling: `font-size`, `font-weight`, `line-height`, `text-decoration`, `font-style`
- Visual effects: `border-radius`, `box-shadow`, `opacity`
- CSS selectors and inline style synthesis

### DOM & Layout
- Virtual DOM diffing and efficient patching
- Block layout engine with sizing, margins, padding
- Positioning: `static | relative | absolute | fixed | sticky`
- Z-index and stacking context
- Overflow clipping and scroll offsets

### Rendering & Events
- Event binding for common UI events: `click`, `input`, `change`, keyboard, mouse events
- Hit testing aligned with render order and stacking
- Multiple renderer backends:
  - **wgpu** — GPU-accelerated rendering
  - **skia-native** — Native Skia rendering

---

## Template Syntax Reference

Templates use canonical directives:

```html
<template>
  <div>
    <p v-if="count > 0">Count is positive</p>
    <p v-else-if="count == 0">Count is zero</p>
    <p v-else>Count is negative</p>
    
    <span>{{ label }}</span>
  </div>
</template>
```

**Supported directives:**
- `v-if` — Conditional rendering (true/false)
- `v-else-if` — Additional condition
- `v-else` — Fallback when all conditions false
- `{{ expr }}` — Template interpolation

---

## Release Checklist

Before publishing a new version, verify:

```bash
# 1. Run full test suite
cargo test --workspace --all-targets

# 2. Build everything in release mode
cargo build --workspace --release

# 3. Test CLI scaffolding
velox init test-app
cd test-app
velox lint src/
velox build src/App.vx
cd ..

# 4. Build and test examples
cd examples/counter-app
velox lint src/
cargo run

# 5. Final manual review
# - Verify documentation accuracy
# - Check README examples work
# - Confirm all examples run cleanly
```

---

## Contributing

This repository is currently in active stabilization for the first stable release. When contributing:

- Ensure all tests pass: `cargo test --workspace --all-targets`
- Build in release mode to catch optimizations: `cargo build --workspace --release`
- Manual verification of examples recommended before PRs
- Update relevant documentation when changing behavior

---

## Notes

- Velox is a modular framework with independent crates that can be used separately
- The CLI (`velox-cli`) is the primary developer-facing tool
- Multiple renderer backends allow flexibility in deployment targets
- The SFC compiler generates optimized Rust code from `.vx` components
