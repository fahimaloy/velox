# velox-core

Reactive signals, effects and lifecycle primitives for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-core.svg)](https://crates.io/crates/velox-core) [![Downloads](https://img.shields.io/crates/d/velox-core.svg)](https://crates.io/crates/velox-core) [![docs.rs](https://img.shields.io/docsrs/velox-core)](https://docs.rs/velox-core) [![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/velox-core.svg)](https://crates.io/crates/velox-core) [![CI](https://img.shields.io/github/actions/workflow/status/fahimaloy/velox/ci.yml?branch=main)](https://github.com/fahimaloy/velox/actions/workflows/ci.yml)

## Overview

`velox-core` is the floor of the Velox stack: single-threaded reactive state (`Signal`), effects, computed values, Vue-style `ref`/`watch`/`computed` ergonomics, component lifecycle hooks, dependency injection, and a `next_tick` queue. It knows nothing about elements, CSS or pixels — `velox-dom`, `velox-style` and `velox-renderer` all sit on top of it. Everything is `Rc`/`RefCell`/thread-local by construction and deliberately `!Send`; the crate's own doctests pin that property with `compile_fail` guards.

## Features

- **Signals** — `Signal<T>` with `get`/`set`/`update`/`set_if_changed`; subscriber notification is deferred inside `batch`
- **Effects** — run once to collect dependencies, then again on every change; stoppable via `EffectHandle`
- **Computed values** — derived signals that recompute only when their dependencies change
- **Vue-style ergonomics** — `ref_value`/`shallow_ref`, `computed_ref`, and `watch`/`watch_ref`/`watch_effect` with `(new, old)` callbacks
- **Batching** — `batch` collapses many `set`s into a single subscriber flush
- **Lifecycle hooks** — `on_mounted`/`on_updated`/`on_unmounted`/`on_resize` and per-component cleanup
- **Provide/inject** — typed dependency injection down the component tree, with fallbacks
- **`next_tick` queue** — callbacks after the current reactive flush, drained synchronously with `flush_sync`
- **Two-way binding** — the `VModel` trait, implemented for `Cell<T>`, `RefCell<String>`, `Signal<T>` and `Ref<T>`
- **`!Send` by design** — everything is `Rc`/`RefCell`/thread-local; no data races by construction

## Installation

```bash
cargo add velox-core
```

No feature flags. The only dependency is `log 0.4`.

## Quick start

```rust
use velox_core::{computed_ref, ref_value, watch_ref};
use velox_core::signal::effect;

let count = ref_value(0);

// Derived state: re-runs whenever `count` changes.
let doubled = computed_ref({
    let count = count.clone();
    move || count.get() * 2
});

// Effects run once to collect dependencies, then on every change.
let printed = count.clone();
let handle = effect(move || println!("count = {}", printed.get()));

// Watchers fire only on change, with (new, old).
watch_ref(&count, |new, old| println!("{old} -> {new}"));

count.set(41);
assert_eq!(doubled.get(), 82);

handle.stop();
```

Interpolations and event handlers in generated components take `&self` — state lives in interior-mutable `Ref<T>` signals, not behind `&mut self`.

## Documentation

- API reference: [docs.rs/velox-core](https://docs.rs/velox-core)
- Book: [the Counter tutorial](https://fahimaloy.github.io/velox/tutorials/counter.html) — signals, effects and reactivity in practice

## Workspace crates

Velox is a workspace of six published crates. This crate is marked **(this crate)**.

| Crate | What it provides | Documentation |
|:---|:---|:---|
| **[velox-core](https://docs.rs/velox-core)** · this crate | Reactive primitives: signals, effects, lifecycle hooks | [docs.rs](https://docs.rs/velox-core) |
| [velox-dom](https://docs.rs/velox-dom) | `VNode` types, block + flex layout, positioning, overflow | [docs.rs](https://docs.rs/velox-dom) |
| [velox-style](https://docs.rs/velox-style) | CSS parsing, selectors, cascade; styles applied to `VNode` | [docs.rs](https://docs.rs/velox-style) |
| [velox-renderer](https://docs.rs/velox-renderer) | Rendering backends, event dispatch, hit testing | [docs.rs](https://docs.rs/velox-renderer) |
| [velox-sfc](https://docs.rs/velox-sfc) | Single-file component compiler (pest grammar) → Rust | [docs.rs](https://docs.rs/velox-sfc) |
| [veloxc](https://docs.rs/veloxc) | The CLI and dev server | [docs.rs](https://docs.rs/veloxc) |

## Status

Published on [crates.io](https://crates.io/crates/velox-core) — the badge above tracks the latest release.

The reactive core is complete and in production use across the stack: signals, effects, computed values, watchers, lifecycle hooks, provide/inject and `next_tick` all work today. `emit`/`emit_void` are intentional no-op placeholders until codegen supplies a real event sink. Everything is deliberately `!Send` — the crate is single-threaded by design, and its doctests enforce that with `compile_fail` guards.

## License

Licensed under **MIT OR Apache-2.0**.

Copyright © FAHIM AHMED <fahimaloy@tutamail.com>
