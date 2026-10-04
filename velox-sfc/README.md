# velox-sfc

Single-file component (`.vx`) compiler for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-sfc.svg)](https://crates.io/crates/velox-sfc) [![Downloads](https://img.shields.io/crates/d/velox-sfc.svg)](https://crates.io/crates/velox-sfc) [![docs.rs](https://img.shields.io/docsrs/velox-sfc)](https://docs.rs/velox-sfc) [![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/velox-sfc.svg)](https://crates.io/crates/velox-sfc) [![CI](https://img.shields.io/github/actions/workflow/status/fahimaloy/velox/ci.yml?branch=main)](https://github.com/fahimaloy/velox/actions/workflows/ci.yml)

## Overview

`velox-sfc` turns a `.vx` single-file component — `<template>`, `<script setup>`, `<script>`, `<style>` — into Rust source: it splits the blocks, parses the template into an AST, resolves component imports, and emits the render function, event dispatcher, `v-model` setters and scoped CSS that the generated `State` runs on. It is a pure parser/compiler crate: its only dependencies are `pest` and `pest_derive`, so it can run inside a build script without dragging in the rest of the framework. The `veloxc` CLI drives it, and its own tests — which execute the generated code — are what pull `velox-core`, `velox-dom` and `velox-renderer` in as dev-dependencies.

## Features

- **`.vx` splitting** — `<template>`, `<script setup>`, `<script>` and `<style>` blocks via a pest grammar, with caret-style parse errors and non-fatal warnings collected into `Sfc::warnings`
- **Template parsing** — iterative HTML-ish parser handling nesting, self-closing tags, `:bind`, `@event` and `{{ interpolation }}`; warnings returned via `TemplateDiag`
- **Rust codegen** — emits `render()`, the event dispatcher, `v-model` setters and scoped CSS; `RenderMode::State` (what `veloxc` uses) and `Resolve` (reports unresolvable loop bindings)
- **Component imports** — `ComponentResolver` resolves PascalCase tags to their file imports, recursively
- **Scoped CSS** — `<style scoped>` rewritten to per-component `data-v-*` selectors
- **Stub generation** — `to_stub_rs` and variants for scaffolding a component
- **Script linting** — `lint_script` warns about `Cell`/`RefCell` state that will not trigger a redraw

## Installation

```bash
cargo add velox-sfc
```

No feature flags. Runtime dependencies: `pest 2.8` and `pest_derive 2.8` only — the crate's normal dependency surface is a pure parser.

## Quick start

Compile a minimal component — parse the `.vx`, then generate the render function:

```rust
use velox_sfc::{compile_template_to_rs, parse_sfc, to_stub_rs};

let src = r#"<template>
  <div>
    <button @click="inc">Inc</button>
    <span>{{ count }}</span>
  </div>
</template>
<script setup>
use std::cell::Cell;
pub struct State { pub count: Cell<i32> }
impl State {
    pub fn new() -> Self { Self { count: Cell::new(0) } }
    pub fn inc(&self) { self.count.set(self.count.get() + 1); }
}
</script>"#;

let sfc = parse_sfc(src).expect("parse ok");
assert!(sfc.template.is_some());
assert!(sfc.script_setup.is_some());

let template = sfc.template.as_ref().unwrap().content.as_str();
let render_fn = compile_template_to_rs(template, "Counter", None).expect("compiles");

// The generated body carries the event wiring and the render functions.
assert!(render_fn.contains("make_on_event"));
assert!(render_fn.contains("inc"));
assert!(render_fn.contains("render()"));

let stub = to_stub_rs(&sfc, "Counter");
println!("{stub}");
```

## Documentation

- API reference: [docs.rs/velox-sfc](https://docs.rs/velox-sfc)
- Book: [Template syntax](https://fahimaloy.github.io/velox/features/template-syntax.html) — the `.vx` format the crate compiles

## Workspace crates

Velox is a workspace of six published crates. This crate is marked **(this crate)**.

| Crate | What it provides | Documentation |
|:---|:---|:---|
| [velox-core](https://docs.rs/velox-core) | Reactive primitives: signals, effects, lifecycle hooks | [docs.rs](https://docs.rs/velox-core) |
| [velox-dom](https://docs.rs/velox-dom) | `VNode` types, block + flex layout, positioning, overflow | [docs.rs](https://docs.rs/velox-dom) |
| [velox-style](https://docs.rs/velox-style) | CSS parsing, selectors, cascade; styles applied to `VNode` | [docs.rs](https://docs.rs/velox-style) |
| [velox-renderer](https://docs.rs/velox-renderer) | Rendering backends, event dispatch, hit testing | [docs.rs](https://docs.rs/velox-renderer) |
| **[velox-sfc](https://docs.rs/velox-sfc)** · this crate | Single-file component compiler (pest grammar) → Rust | [docs.rs](https://docs.rs/velox-sfc) |
| [veloxc](https://docs.rs/veloxc) | The CLI and dev server | [docs.rs](https://docs.rs/veloxc) |

## Status

Published on [crates.io](https://crates.io/crates/velox-sfc) — the badge above tracks the latest release.

Working today: `.vx` parsing, `{{ expr }}` interpolation, `v-if`/`v-else-if`/`v-else`, and component imports. `parse_template_to_ast` preserves its historical behaviour of printing warnings to stderr; prefer `parse_template`, which returns them in a `TemplateDiag`. The crate is pure-parser at runtime — `velox-core`, `velox-dom` and `velox-renderer` are dev-dependencies used only by the tests that execute generated code.

## License

Licensed under **MIT OR Apache-2.0**.

Copyright © FAHIM AHMED <fahimaloy@tutamail.com>
