# Velox

Vue SFC-syntax Rust GUI framework with Skia rendering.

## Architecture

```
.vx SFC File
    │
    ▼
velox-sfc (compiler)
    ├── grammar.pest → pest parser
    ├── parse_sfc() → SFC AST
    ├── template_codegen.rs → template → VNode
    └── codegen.rs → SFC → Rust code
    │
    ▼
Generated Rust in OUT_DIR/
    │
    ▼
velox-renderer (Skia + softbuffer)
    ├── lib.rs: run_window_vnode_skia()
    ├── skia_render.rs → RGBA pixels
    └── presenter.rs → SoftbufferPresenter
```

## Crates

| Crate | Purpose |
|-------|---------|
| velox-core | Reactive primitives (ref, signal, lifecycle hooks) |
| velox-sfc | .vx file parsing and code generation |
| velox-dom | Layout computation (compute_layout) |
| velox-style | CSS processing and style application |
| velox-renderer | Skia rendering and window management |
| velox-cli | Build CLI and dev server |

## Key Commands

```bash
cd /home/fahimaloy/Projects/personal/velox

# Build all
cargo build

# Run tests
cargo test

# Run specific crate tests
cargo test -p velox-sfc

# Build example
cargo build --example todo

# Run (requires display/compositor)
cargo run --example todo
```

## Key Files

- `velox-sfc/src/codegen.rs` - SFC to Rust codegen
- `velox-sfc/src/template_codegen.rs` - Template to VNode codegen
- `velox-renderer/src/lib.rs` - Window + Skia rendering loop
- `velox-renderer/src/presenter.rs` - Softbuffer presenter
- `velox-core/src/signal.rs` - Reactive primitives
- `velox-core/src/lifecycle.rs` - Lifecycle hooks

## .vx Syntax

```vue
<script>
  let count = ref!(0);

  fn on_increment() {
    count.set(*count + 1);
  }
</script>

<template>
  <button @click={on_increment}>
    Count: {count}
  </button>
</template>

<style>
  button {
    padding: 8px 16px;
    background: #4a90d9;
    color: white;
    border-radius: 4px;
  }
</style>
```

## Coding Conventions

- Reactive primitives: `ref!()`, `signal!()`, `define_emits!()`, `on_mounted!()`
- Template interpolation: `{variable}`
- Event handlers: `@click={handler}`
- Attribute binding: `:src={expr}`
- Conditional: `v-if`, `v-else`
- Lists: `v-for` with `:key`

## Known Issues

- EPIPE crash when no compositor available (headless)
- CSS scoping not fully implemented
- Some CSS properties missing (box-shadow, text-shadow)
