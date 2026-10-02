# Velox Framework

Velox is a modular Rust UI framework for building reactive, component-driven desktop apps.

It provides:
- **SFC compiler** for `.vx` components (`<template>`, `<script setup>`, `<style>`)
- **Layout engine** over a `VNode` tree (block + flex), driven by an
  immediate-mode render loop
- **CSS parser/cascade engine**
- **Renderer backends** (`skia-native`)
- **CLI** for scaffolding, building, linting, and running apps

Not provided: there is no reconciler on the render path. See
[DOM & Layout](#dom--layout) for exactly what exists and what does not.

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

- **Rust 1.85+** — Install from [rustup.rs](https://rustup.rs)
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
- **`velox-dom`** — `VNode` types, block/flex layout, CSS style resolution, and a
  keyed-diff module that is complete but has no production caller
- **`velox-style`** — CSS parsing, selectors, style application
- **`velox-renderer`** — Rendering + events (`skia-native` backend)
- **`velox-cli`** — Developer workflow commands

Plus examples:
- `examples/counter` — reactive counter (signals, event handlers)
- `examples/todo` — todo list (components, `v-for`, filters, scrolling)
- `examples/showcase` — layout gallery (spacing, flex, centering, scrolling)

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

The renderer supports two feature levels:

```bash
# Skia API surface only (no native dependencies)
cargo build -p velox-renderer --features skia

# Native Skia backend
cargo build -p velox-renderer --features skia-native
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
# Edition: 2024
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

# Start the dev server: it watches the current directory by default,
# and rebuilds + restarts the app when a file changes
velox dev

# In another terminal, edit src/App.vx and save
# → the app is recompiled from scratch and restarted

# Press 'r' in the dev terminal to rebuild and restart by hand
# Press 'q' to quit
```

`velox dev` is a **watch-and-rebuild loop, not hot reload.** The change is
detected quickly (the watcher is `notify`/inotify, not a poll), but what happens
next is a full `cargo build` followed by killing and re-spawning the app. Nothing
is patched into a running process, and no in-process state survives the
restart. Treat the rebuild time as the cost of every save. The mechanism is
described precisely under [`velox dev`](#3-velox-dev-options).

### Run Examples

```bash
# From anywhere, test the counter example:
velox init temp-counter
cd temp-counter

# Or build an example from the Velox repository:
cd /path/to/velox
velox build examples/counter/src/App.vx
cargo run -p velox-example-counter
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
velox dev         # Watch, rebuild and restart on every change
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
velox build examples/counter/src/App.vx
```

**Output:**
- `velox build` produces a full Cargo project build (binary in `target/debug` or `target/release`).
- `velox build src/App.vx` generates Rust source (e.g., `App.rs`) for inspection/integration and is not directly executable.

---

#### 3. `velox dev [options]`

Start the development server: watch the project, and on every change run a full
`cargo build` and restart the app.

**Syntax:**
```bash
velox dev [--watch <directory>] [--release]
```

**Options:**
- `-w, --watch <PATH>` — Project directory to build and watch for changes (default: the current directory)
- `--release` — Build in release mode

`--watch` names the **project** directory, not an extra directory to watch alongside
the current one. It may be given only once.

**Examples:**
```bash
# Watch the current project directory, rebuilding and restarting on change
velox dev

# Build and watch a project rooted at another path
velox dev --watch ../my-app
```

**What actually happens on a save — there is no hot reload here.**
- File changes are reported by the OS (`notify`/inotify on Linux), not by
  polling, so the *change* is noticed within milliseconds
- `target/`, `.git`, `.vscode`, `.idea` and dot-directories are excluded —
  `cargo build` writes thousands of files into `target/`, and watching it
  exhausts the kernel's watch budget
- A burst of saves is coalesced into one rebuild
- The app is told `HmrMessage::FullReload` over the HMR channel, killed and
  reaped, then `cargo build` runs to completion and the app is re-spawned
- **Every kind of change takes that same path.** A `<style>` edit is classified
  as a stylesheet swap by `react_to_change`, but
  `ChangeReaction::needs_rebuild()` returns `true` for every change kind today
  (`velox-cli/src/commands/dev.rs`), so the swap is not taken: a CSS edit costs
  a full rebuild and a restart exactly like a `<script>` edit. The classifier is
  correct and tested; the boolean that would skip the rebuild is a deliberate
  stop pending an `HmrMessage::StyleUpdate` that does not exist yet.
- `HmrMessage::HotReload` exists in the protocol but is explicitly documented as
  "module-level HMR not yet implemented" and is treated as a `FullReload`
- No in-process state survives a restart — component state, focus, caret,
  selection and scroll offsets are all rebuilt from scratch
- Press `r` to rebuild and restart by hand
- Press `q` to quit

**Inotify watch limit (Linux):** watching is backed by inotify, a finite kernel
resource. If it is exhausted the dev server reports the error and keeps running, but
files may no longer be detected — press `r` to rebuild by hand. Raise the limit with:

```bash
sudo sysctl -w fs.inotify.max_user_watches=524288
sudo sysctl -w fs.inotify.max_user_instances=1024
```

To persist it, write those two lines to a file in `/etc/sysctl.d/` and run
`sudo sysctl --system`.

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
Edition: 2024
Platform: Linux
```

---

## Workflow Examples

### Complete: Create, Develop, and Release an App

```bash
# 1. Create a new app
velox init myapp
cd myapp

# 2. Start development server (watches the current directory by default)
velox dev

# 3. In another terminal, edit src/App.vx
#    Save the file: the app is rebuilt from scratch and restarted

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

# 4. Watch terminal 1 for the rebuild and restart to finish
#    (a full cargo build, then the app restarts)

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
cargo run -p velox-example-counter

# Build and run the todo app
cargo run -p velox-example-todo

# Build and run the layout showcase
cargo run -p velox-example-showcase
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
- Text styling that is actually painted: `font-size`, `font-weight`,
  `line-height`, `text-decoration`
- Visual effects that are actually painted: `border-radius`, `opacity`
- CSS selectors and inline style synthesis

**Parsed but not painted.** These declarations parse, survive the cascade, and
produce no visual change. `velox lint` reports each one when a `.vx` file
declares it, so a project using one gets a warning rather than a silent no-op.
The list is authoritative in `velox-dom`'s `ComputedStyle::PARSED_BUT_UNRENDERED`,
and it currently includes `box-shadow`, `font-style`, `letter-spacing`,
`visibility`, `overflow-x`, `overflow-y`, `background-image`, `transition`,
`border-style`, `border-color`, and `transform` (read only to force a stacking
context, never applied as a visual transform). Two worth calling out:

- **`box-shadow`** — `set_property` stores a parsed `BoxShadow` value, and the
  value is correct, but no painter looks it up. The string `"box-shadow"` does
  not appear anywhere in `velox-renderer`. Declaring it changes nothing.
- **`font-style`** — `set_property` arms it and the cascade inherits it, but the
  renderer's `TextRenderConfig.font_style` field is only ever written by
  `Default::default()`. No builder method sets it and nothing copies it from the
  computed style, so `font-style: italic` renders upright.

### DOM & Layout
- Block and flex layout over the current `VNode` tree, recomputed from the tree
  each frame
- Block layout engine with sizing, margins, padding
- Positioning: `static | relative | absolute | fixed | sticky`
- Z-index and stacking context
- Overflow clipping and scroll offsets

**There is no diffing or patching on the render path.** This is the one place
where the shape of the architecture is worth stating plainly, because "virtual
DOM" usually implies the opposite:

- The render loop is **immediate-mode**. `run_window_vnode_skia` takes the
  `&VNode` it is given, runs `compute_layout` over the whole tree, and paints
  it. There is no retained tree, no previous frame to compare against, and no
  patch applier.
- `velox-dom` does contain a complete, duplicate-key-safe keyed reconciler
  (`diff::diff`, emitting `Patch::MoveChild` for a reorder rather than an
  insert/remove pair). **It has zero production callers.** Only tests call it.
- The renderer's own keyed reconciler, `reconcile_keyed_children`, was
  **deleted**: it had no production callers, and on a key match it pushed the
  previous node and discarded the incoming one, so a keyed child whose text
  changed kept stale content.
- `:key` is a deliberate **non-goal**, not an oversight. It compiles and lands as
  a plain runtime `key` string attribute; it does not reorder, diff, or preserve
  identity. Reorder correctness comes from `VNode` child order, because
  `compute_layout` lays children out in `VNode` order.
- The price, paid today: an `<input>` inside a reordered `v-for` loses its focus
  and caret to whichever item now occupies its index, because all cross-frame
  state is keyed by structural path, never by `key`.

The full decision record, including what would reopen it, is in
[`docs/RECONCILER.md`](docs/RECONCILER.md).

### Rendering & Events
- Event binding for common UI events: `click`, `input`, `change`, keyboard, mouse events
- Hit testing aligned with render order and stacking
- Multiple renderer backends:
  - **skia** — Skia API surface, no native dependencies
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

### What CI actually gates

CI runs on **every push and every pull request, on every branch**
(`.github/workflows/ci.yml`). The previous trigger was
`branches: [ main, master, alpha ]`, so feature branches ran no checks at all —
including the branch this cycle's work landed on. Nothing in the repository
records whether a pull request is *required* to pass these checks before merge;
that is a GitHub branch-protection setting and is not visible from the source
tree.

The blocking jobs are:

| Job | Blocks on |
|---|---|
| `build-test` | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast` |
| `renderer-features` | `cargo test -p velox-renderer --features skia` |
| `docker` | `docker build --target test`, which re-runs `cargo test --workspace --no-fail-fast` inside the image |
| `coverage` | An instrumented line-coverage run must succeed. See the caveat below |
| `dependency-audit` | `cargo audit`, minus five recorded waivers |
| `property-tests` | Nothing — it is a no-op today. See the caveat below |

Note that `build-test` is not feature-minimal: `velox-cli` declares
`velox-renderer` with `features = ["skia-native"]` as a **dev-dependency**, and
the example crates build-depend on `velox-cli`, so feature unification turns
`skia-native` on for any workspace-wide cargo command. `cargo test --workspace`
therefore already compiles Skia natively. The `continue-on-error: true` on the
`--features skia-native` build step in `renderer-features` is not what keeps a
Skia compile failure from blocking a merge — `build-test` catches it first.

### What CI does **not** gate — read this before cutting a release

- **Coverage is measured, not enforced.** The `coverage` job runs
  `cargo llvm-cov --workspace --summary-only --fail-under "${COVERAGE_MIN}"` with
  `COVERAGE_MIN: 0` in the workflow's `env`. No instrumented baseline has been
  measured on this workspace, so there is no floor to enforce; the job fails only
  if coverage cannot be collected at all. Setting that one variable to a number
  below a locally measured baseline turns it into a real gate.
- **Dependency advisories are partially waived.** `cargo audit` currently reports
  5 vulnerabilities in this graph; the job ignores five recorded RUSTSEC IDs
  (documented inline in the workflow) so that it is green today and red on
  anything new. It also treats `unmaintained` and `unsound` as non-blocking, so
  9 further advisories are reported without failing the job.
- **Property tests are not gated, because none exist.** The `property-tests` job
  probes for a `proptest!` macro and only installs `cargo-proptest` if it finds
  one. There is no `proptest` dependency in any `Cargo.toml`, in this repository
  or anywhere in its history, so the job currently does nothing.
- **The GPU pixel proofs never run.** Twelve `velox-renderer/tests/skia_*.rs`
  files carry a file-level `#![ignore = "requires skia-native feature and GPU
  hardware"]` (so every test inside them is skipped), and
  `velox-renderer/src/skia_render.rs` ignores two more individually. GitHub's
  `ubuntu-latest` runners have no GPU, so all of them are skipped. Several are
  checksum-pinned render proofs, which means a change that silently alters pixels
  would **not** be caught by CI.
- **No release-profile build.** No job runs `cargo build --workspace --release`.
- **No scaffolding or example smoke test.** Nothing runs `velox init`, and no job
  runs an example app.
- **The slow external-`cargo` compile tests are skipped.** Three tests in
  `velox-sfc/tests/integration_compile.rs` shell out to a second `cargo`, and
  `velox-renderer/tests/frame_cost_bench.rs` is a measurement tool. All four are
  `#[ignore]`'d, so the default `cargo test` pass skips them. They matter when
  changing the codegen or the compile pipeline, and the commands are below.

### Manual steps still required

CI does not replace these. Run them before publishing:

```bash
# 1. Run the full test suite, then the ignored compile/measurement tests
cargo test --workspace --all-targets
cargo test -p velox-sfc --test integration_compile -- --ignored
cargo test -p velox-renderer --test frame_cost_bench -- --ignored --nocapture

# 2. Build everything in release mode
cargo build --workspace --release

# 3. Test CLI scaffolding
velox init test-app
cd test-app
velox lint src/
velox build src/App.vx
cd ..

# 4. Build and test examples
cargo run -p velox-cli -- lint examples   # lints every example .vx file
cargo run -p velox-example-counter
cargo run -p velox-example-todo
cargo run -p velox-example-showcase

# 5. Final manual review
# - Verify documentation accuracy
# - Check README examples work
# - Confirm all examples run cleanly
```

---

## Contributing

This repository is currently in active stabilization for the first stable release. When contributing:

- CI runs on every push and every pull request, on every branch. The command that
  actually blocks you is `build-test`, which runs `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace --no-fail-fast`.
- Build in release mode to catch optimizations: `cargo build --workspace --release`
  (not covered by CI)
- Manual verification of examples recommended before PRs
- Update relevant documentation when changing behavior — including this README,
  which is expected to describe the tree as it is

---

## Notes

- Velox is a modular framework with independent crates that can be used separately
- The CLI (`velox-cli`) is the primary developer-facing tool
- Multiple renderer backends allow flexibility in deployment targets
- The SFC compiler generates optimized Rust code from `.vx` components
