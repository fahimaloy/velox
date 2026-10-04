# Introduction

**Velox** is a modern UI framework for Rust that turns single-file `.vx` components into fast, native desktop applications — rendered by Skia, driven by reactive signals, and shipped as a single native binary.

Write a component once in a familiar, HTML-like template syntax with an inline `<script setup>` block, and Velox compiles it into Rust. During development you get a hot-reload loop that re-renders as you edit; for release you get a self-contained executable with no runtime, no browser, and no JavaScript engine.

```html
<template>
  <div class="app">
    <h1>Count: {{ count }}</h1>
    <button @click="increment">Add one</button>
  </div>
</template>

<script setup>
let count = signal(0);

fn increment() {
  count.set(count.get() + 1);
}
</script>
```

## Why Velox

- **Single-file components** — templates, styles, and logic live together in one `.vx` file. The Velox compiler parses them and generates idiomatic Rust you can read, debug, and ship.
- **Reactive by default** — fine-grained signals and effects power state management. Change a signal, and your view recomputes. No diffing headaches, no manual wiring.
- **Skia rendering** — the same graphics engine behind Chrome and Flutter draws every frame, with a flexbox layout engine and a fixed viewport contract that keeps your UI responsive at any window size, DPI, or scale factor.
- **A real dev loop** — Hot Module Replacement (HMR) reloads your components in a running window as you type, so iteration feels like web development without leaving Rust.

## Who it's for

Velox is for Rust developers who want to build desktop UIs without reaching for a browser shell, and for teams coming from Vue or React who want the single-file component workflow with native performance. If you can write HTML and Rust, you can build a Velox app.

## Next steps

Head to [Getting Started](getting-started.md) to install the CLI and build your first component, then follow the [Counter tutorial](tutorials/counter.md) for a guided tour.

---

*Velox is built and maintained by **FAHIM AHMED** · [fahimaloy@tutamail.com](mailto:fahimaloy@tutamail.com) · [github.com/fahimaloy/velox](https://github.com/fahimaloy/velox)*
