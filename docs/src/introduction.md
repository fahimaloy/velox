# Introduction

**Velox** is a modern UI framework for Rust that turns single-file `.vx` components into fast, native desktop applications — rendered by Skia, driven by reactive signals, and shipped as a single native binary.

Write a component once in a familiar, HTML-like template syntax with an inline `<script setup>` block, and Velox compiles it into Rust. During development you get a dev-server loop that classifies each edit by SFC block — style, template, or script — and rebuilds and relaunches the native window, with an HMR control channel on `127.0.0.1:31313`; for release you get a self-contained executable with no runtime, no browser, and no JavaScript engine.

```html
<template>
  <div class="app">
    <h1>Count: {{ count }}</h1>
    <button @click="increment">Add one</button>
  </div>
</template>

<script setup>
use velox_core::ergonomics::Ref;

pub struct State {
    count: Ref<i32>,
}

impl State {
    pub fn new() -> Self {
        Self { count: velox_core::r#ref!(0) }
    }

    pub fn count(&self) -> i32 {
        self.count.get()
    }

    pub fn increment(&self) {
        self.count.set(self.count.get() + 1);
    }
}
</script>
```

## Why Velox

- **Single-file components** — templates, styles, and logic live together in one `.vx` file. The Velox compiler parses them and generates idiomatic Rust you can read, debug, and ship.
- **Reactive by default** — fine-grained signals and effects power state management. Change a signal, and your view recomputes; no manual wiring. Velox's render loop is **immediate-mode**, and `:key` today is a plain string attribute with no identity or reconciliation semantics — see [Reconciler](advanced/reconciler.md) for the honest picture, including the `:key` reorder caveat.
- **Skia rendering** — the same graphics engine behind Chrome and Flutter draws every frame, with a flexbox layout engine and a fixed viewport contract that keeps your UI responsive at any window size, DPI, or scale factor.
- **A real dev loop** — save a `.vx` file and the dev server classifies the edit by SFC block (style, template, or script), rebuilds, and relaunches the native window; an HMR control channel on `127.0.0.1:31313` coordinates the running app. Module-level hot reload is not implemented yet — today every save is a full rebuild and restart.

## Who it's for

Velox is for Rust developers who want to build desktop UIs without reaching for a browser shell, and for teams coming from Vue or React who want the single-file component workflow with native performance. If you can write HTML and Rust, you can build a Velox app.

## Next steps

Head to [Getting Started](getting-started.md) to install the CLI and build your first component, then follow the [Counter tutorial](tutorials/counter.md) for a guided tour.

---

*Velox is built and maintained by **FAHIM AHMED** · [fahimaloy@tutamail.com](mailto:fahimaloy@tutamail.com) · [github.com/fahimaloy/velox](https://github.com/fahimaloy/velox)*
