<div align="center">

# ⚡ Velox

### A Modern Rust UI Framework

**Reactive components · Instant rebuilds · Skia rendering**

[![Crates.io](https://img.shields.io/crates/v/velox-cli.svg?style=for-the-badge&color=blue)](https://crates.io/crates/velox-cli)
[![Docs](https://img.shields.io/badge/docs-velox.dev?style=for-the-badge&color=lightblue)](https://velox.dev/docs/cli)
[![License: MIT](https://img.shields.io/badge/license-MIT?style=for-the-badge&color=green)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B?style=for-the-badge&color=orange)](https://rustup.rs)
[![CI](https://img.shields.io/github/actions/deployment/fahimaloy/velox?label=ci&style=for-the-badge)](https://github.com/fahimaloy/velox/actions)

</div>

---

<div align="center">

| | |
|---|---|
| **⚡ Immediate-mode** | No diffing overhead — full tree recompute per frame |
| **🧩 SFC compiler** | `.vx` components with `<template>` · `<script setup>` · `<style>` |
| **🎨 CSS engine** | Full cascade, selectors, block + flex layout |
| **🖼️ Skia native** | GPU-accelerated rendering via `skia-safe` |
| **🛠️ CLI first** | Scaffold, lint, build, watch — zero config |
| **📦 Modular crates** | Use any combination of the 6 published crates |

</div>

---

<div align="center">

**[Quick Start](#quick-start)** · **[CLI Reference](#cli-reference)** · **[Architecture](#architecture)** · **[Templates](#template-syntax)** · **[Examples](#examples)** · **[Contributing](#contributing)**

</div>

---

## 🚀 Quick Start

### Prerequisites

| Requirement | Check |
|:---|:---|
| Rust **1.85+** | `rustup --version` |
| Git | `git --version` |
| C build tools | See [Rust install guide](https://rustup.rs) |

<details>
<summary>Platform-specific build tools</summary>

```bash
# Linux (Debian/Ubuntu)
sudo apt-get install build-essential pkg-config libssl-dev

# macOS
xcode-select --install

# Windows
# Microsoft Visual C++ Build Tools: https://visualstudio.microsoft.com/visual-cpp-build-tools/
```
</details>

---

### Install the CLI

```bash
cargo install velox-cli
```

Verify:

```bash
$ velox version
velox 0.1.0
Edition: 2024
Platform: Linux
```

> **Dev tip:** Install from a cloned repo with `cargo install --path velox-cli --force` to get the latest `main` branch without waiting for a crates.io release.

---

### Your First App — 30 seconds

```bash
# 1. Scaffold a new project
velox init my-first-app
cd my-first-app

# 2. Lint the generated component
velox lint src/App.vx

# 3. Build and run
velox run

# 4. Watch mode — rebuilds on every save
velox dev
# Press 'r' to force rebuild · Press 'q' to quit
```

**Generated structure:**

```
my-first-app/
├── Cargo.toml          ← auto-wired velox dependencies
├── build.rs            ← invokes velox-sfc build script
├── src/
│   ├── App.vx          ← your component (SFC)
│   └── main.rs         ← Rust entry point
├── assets/             ← images and resources
└── README.md
```

---

## 🖥️ CLI Reference

Complete command reference. All commands work from any directory — paths are resolved relative to the **current working directory**, not the Velox source.

---

### `velox init`

```
velox init <project-name> [options]
velox init <absolute-path>
velox init <relative/path>
```

| Option | Description |
|:---|:---|
| `-t, --template <name>` | Template variant (default: `default`) |
| `--local <path>` | Use a local template directory instead of built-in |

```bash
velox init myapp                    # in current dir
velox init /tmp/myapp               # absolute path
velox init ../projects/myapp        # relative path
```

---

### `velox build`

Two modes:

```bash
# Mode 1 — build the whole project
velox build [--release]

# Mode 2 — compile a single .vx file to Rust source
velox build <file.vx> [-o <output-dir>]
```

| Flag | Applies to |
|:---|:---|
| `--release` | Full-project build — optimized binary in `target/release/` |
| `-o, --out-dir` | Single-file mode — where generated `.rs` files are written (default: `target/velox-gen`) |

> **Note:** `velox build src/App.vx` emits generated Rust source for inspection. It is **not** a directly executable binary.

---

### `velox dev`

```
velox dev [options]
```

| Flag | Default | Description |
|:---|:---|:---|
| `-w, --watch <path>` | `.` | Project root to build and watch |
| `--release` | — | Build in release mode |

**What happens on every save:**

```
File change detected (inotify / notify)
        ↓
Debounce burst of saves
        ↓
cargo build  ← full rebuild, no hot reload
        ↓
App process killed and re-spawned
```

> ⚠️ `velox dev` is a **watch-and-rebuild** loop, not hot reload. In-process state does not survive a restart. Treat the rebuild time as the cost of every save.

**Inotify watch limit (Linux):**

```bash
# Raise kernel watch budget (add to /etc/sysctl.d/99-inotify.conf to persist)
sudo sysctl -w fs.inotify.max_user_watches=524288
sudo sysctl -w fs.inotify.max_user_instances=1024
```

---

### `velox run`

```
velox run [--release]
```

Build and run in one step. Equivalent to `cargo build && cargo run`.

---

### `velox lint`

```
velox lint [target]
```

| Target | Effect |
|:---|:---|
| *(none)* | Lint all `.vx` files in `src/` |
| `src/App.vx` | Lint a specific file |
| `components/` | Lint all `.vx` in a directory |

Reports parse errors, component issues, and unrecognized CSS properties that parse but are not painted.

---

### `velox version`

```
velox version
# velox 0.1.0
# Edition: 2024
# Platform: Linux
```

---

## 📐 Architecture

Velox is a **modular workspace** of 6 published crates. Use any combination:

```
┌─────────────────────────────────────────────────────────────────┐
│  velox-cli  (velox binary)                                     │
│  init · build · dev · run · lint · version                    │
├─────────────────────────────────────────────────────────────────┤
│  velox-renderer   skia-native · skia · svg                     │
│  event dispatch · hit test · immediate-mode paint             │
├─────────────────────────────────────────────────────────────────┤
│  velox-style     cssparser · selectors                         │
│  cascade resolution · style application                        │
├─────────────────────────────────────────────────────────────────┤
│  velox-dom       VNode · block layout · flex layout            │
│  positioning · z-index · overflow clipping                     │
├─────────────────────────────────────────────────────────────────┤
│  velox-sfc       pest · pest_derive                            │
│  .vx parsing · template codegen · component resolver           │
├─────────────────────────────────────────────────────────────────┤
│  velox-core      log                                           │
│  signals · effects · lifecycle                                 │
└─────────────────────────────────────────────────────────────────┘
```

| Crate | Dependencies | Description |
|:---|:---|:---|
| `velox-core` | `log` | Reactive primitives: signals, effects, lifecycle hooks |
| `velox-sfc` | `pest`, `pest_derive` | SFC parser, template codegen, component import resolver |
| `velox-dom` | — | `VNode` types, block/flex layout engine, style resolution |
| `velox-style` | `cssparser`, `selectors`, `velox-dom` | CSS parsing, cascade, style application to VNode |
| `velox-renderer` | `velox-core`, `velox-dom`, `velox-style`, `skia-safe`* | Skia rendering backend, event dispatch, hit test |
| `velox-cli` | `clap`, `anyhow`, `velox-sfc`, `velox-style`, `velox-renderer` | All CLI subcommands |

\* optional — gated behind `skia-native` feature

---

## 🎨 Template Syntax

`.vx` files are Velox's single-file component format:

```html
<!-- src/App.vx -->
<template>
  <div class="container">
    <h1>{{ title }}</h1>
    <p v-if="count > 0">Count is positive</p>
    <p v-else>Count is zero or negative</p>
    <button @click="increment">Increment</button>
  </div>
</template>

<script setup>
pub fn new() {
    let count = velox_core::signal(0);
    count.subscribe(|v| println!("count is now {v}"));
}
</script>

<style>
.container {
  padding: 24px;
  font-size: 18px;
  border-radius: 8px;
  opacity: 0.9;
}
</style>
```

### Supported directives

| Directive | Usage |
|:---|:---|
| `{{ expr }}` | Template interpolation |
| `v-if="cond"` | Conditional rendering |
| `v-else-if="cond"` | Additional condition |
| `v-else` | Fallback |
| `@event="handler"` | Event binding |
| `:key` | Keyed child (order-preservation hint) |

---

## 🌊 Supported Features (v0.1)

### ✅ Fully functional

| Category | Features |
|:---|:---|
| **SFC** | `.vx` parsing · `{{ expr }}` · `v-if`/`v-else-if`/`v-else` · component imports |
| **Layout** | Block layout · Flex layout · `static`/`relative`/`absolute`/`fixed`/`sticky` · z-index · overflow clipping |
| **CSS** | Selectors · cascade · `font-size` · `font-weight` · `line-height` · `text-decoration` · `border-radius` · `opacity` |
| **Rendering** | Skia immediate-mode paint · event hit-testing · `skia-native` backend |
| **Events** | `click` · `input` · `change` · keyboard · mouse |

### ⚠️ Parsed but not painted

These properties parse successfully, survive the cascade, and produce **no visual change**. `velox lint` reports each when used.

`box-shadow` · `font-style` · `letter-spacing` · `visibility` · `overflow-x` · `overflow-y` · `background-image` · `transition` · `border-style` · `border-color` · `transform` (stacking only, no visual transform)

<details>
<summary>Why is the reconciler not in production? (read before asking)</summary>

The render loop is **immediate-mode**. `run_window_vnode_skia` takes the `&VNode`, runs `compute_layout` over the whole tree, and paints it. There is no retained tree, no previous frame to compare, and no patch applier.

`velox-dom` ships a complete, duplicate-key-safe keyed reconciler (`diff::diff`) but it has **zero production callers** — only tests invoke it.

The renderer's `reconcile_keyed_children` was deleted: it had no production callers and had a stale-content bug on key match.

`v-for` reordering: correctness comes from `VNode` child order, not from `:key`. An `<input>` inside a reordered `v-for` loses focus/caret to whichever item occupies its index. This is a known limitation, not an oversight.

Full decision record: [`docs/RECONCILER.md`](docs/RECONCILER.md)
</details>

---

## 📚 Examples

Three example apps ship in `examples/`:

| App | What it demonstrates | Run |
|:---|:---|:---|
| `examples/counter` | Signals, event handlers, immediate reactivity | `cargo run -p velox-example-counter` |
| `examples/todo` | Components, `v-for`, computed filters, scrolling lists | `cargo run -p velox-example-todo` |
| `examples/showcase` | Layout gallery: spacing, flex, centering, wrapping | `cargo run -p velox-example-showcase` |

All three are also runnable via `velox`:

```bash
velox build examples/counter/src/App.vx
cargo run -p velox-example-counter
```

---

## 🔧 Building from Source

```bash
git clone https://github.com/fahimaloy/velox.git
cd velox

# Build all crates
cargo build --workspace

# Release build
cargo build --workspace --release

# Run full test suite
cargo test --workspace --all-targets

# Build Skia backend only
cargo build -p velox-renderer --features skia-native

# Test a specific crate
cargo test -p velox-core
cargo test -p velox-sfc
cargo test -p velox-renderer
```

---

## 🏁 Workflow Examples

### Full app lifecycle

```bash
velox init myapp
cd myapp

velox dev                # watch mode — rebuilds on save

# When ready:
velox run --release      # optimized build
./target/release/myapp   # the binary
```

### Component library

```bash
velox init my-lib
cd my-lib

# Create a component
mkdir -p src
cat > src/Button.vx << 'EOF'
<template>
  <div class="btn">
    <span>{{ label }}</span>
  </div>
</template>

<style>
.btn {
  padding: 10px 20px;
  border-radius: 4px;
  font-size: 14px;
}
</style>
EOF

velox lint src/Button.vx
velox build src/Button.vx -o target/components
```

---

## 📦 Crates on crates.io

All crates are published and installable:

| Crate | Install | Use case |
|:---|:---|:---|
| [`velox-core`](https://crates.io/crates/velox-core) | `cargo add velox-core` | Signal/effect primitives |
| [`velox-sfc`](https://crates.io/crates/velox-sfc) | `cargo add velox-sfc` | SFC parsing in custom tooling |
| [`velox-dom`](https://crates.io/crates/velox-dom) | `cargo add velox-dom` | VNode + layout engine |
| [`velox-style`](https://crates.io/crates/velox-style) | `cargo add velox-style` | CSS cascade engine |
| [`velox-renderer`](https://crates.io/crates/velox-renderer) | `cargo add velox-renderer` | Skia rendering |
| [`velox-cli`](https://crates.io/crates/velox-cli) | `cargo install velox-cli` | The `velox` CLI binary |

---

## 🤝 Contributing

CI runs on **every push and pull request, on every branch**. The blocking job is `build-test`:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
```

**Before submitting a PR:**

```bash
# Full test suite + ignored compile/measurement tests
cargo test --workspace --all-targets
cargo test -p velox-sfc --test integration_compile -- --ignored
cargo test -p velox-renderer --test frame_cost_bench -- --ignored --nocapture

# Release build (not gated by CI)
cargo build --workspace --release

# Smoke test CLI scaffolding
velox init test-app && cd test-app
velox lint src/ && velox build src/App.vx
cd ..
```

Update this README when behavior changes.

---

## 📜 License & Trademark

**Code:** Everything in this repository is MIT licensed — see [`LICENSE`](LICENSE).

**"Velox" is a trademark of this project.** MIT grants the right to copy, modify, and redistribute the code and the logo *as part of a Velox project or derivative*. It does not grant the right to use the name or logo in a way that suggests Velox project endorsement of a third-party product.

If you want the logo in a project that is **not** derived from Velox, ask first — that is a trademark question, not a license one.
