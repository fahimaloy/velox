# velox-core

Core reactive primitives for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-core.svg)](https://crates.io/crates/velox-core) [![Downloads](https://img.shields.io/crates/d/velox-core.svg)](https://crates.io/crates/velox-core) [![License](https://img.shields.io/crates/l/velox-core.svg)](https://crates.io/crates/velox-core) [![Documentation](https://img.shields.io/docsrs/velox-core)](https://docs.rs/velox-core)

## Overview

`velox-core` is the floor of the Velox stack: single-threaded reactive state (`Signal`), effects, computed values, Vue-style `ref`/`watch`/`computed` ergonomics, component lifecycle hooks, dependency injection, and a `next_tick` queue. It knows nothing about elements, CSS or pixels — `velox-dom`, `velox-style` and `velox-renderer` all sit on top of it. Everything is `Rc`/`RefCell`/thread-local by construction and deliberately `!Send`; the crate's own doctests pin that property with `compile_fail` guards.

## Installation

```bash
cargo add velox-core
```

No feature flags. The only dependency is `log 0.4`.

## API reference

Signatures below are copied from the source; see [docs.rs/velox-core](https://docs.rs/velox-core) for the full list.

### Types

| Signature | Description |
|---|---|
| `pub struct Signal<T>` | A reactive value. `Signal<T>` is `!Send` by design; methods below exist on `impl<T: Clone> Signal<T>`. |
| `Signal::new(initial: T) -> Self` | Create a signal holding `initial`. |
| `Signal::get(&self) -> T` | Read the value and register the caller as a subscriber. |
| `Signal::set(&self, new: T)` | Write the value and notify subscribers (deferred inside `batch`). |
| `Signal::update<F>(&self, f: F) where F: FnOnce(T) -> T` | Functional update, e.g. `count.update(\|v\| v + 1)`. |
| `Signal::set_if_changed(&self, new: T) where T: PartialEq` | Write only when the value actually differs. |
| `pub struct EffectHandle` | Handle for a running effect. |
| `EffectHandle::stop(&self)` | Stop the effect from ever running again. |
| `EffectHandle::is_active(&self) -> bool` | Whether the effect is still scheduled. |
| `pub struct Ref<T>(Rc<Signal<T>>)` | Vue-like reactive reference; `Clone`, `Deref<Target = Rc<Signal<T>>>`. |
| `Ref::new(value: T) -> Self` | Create a `Ref` (same as `ref_value`). |
| `Ref::get(&self) -> T` / `Ref::set(&self, value: T)` | Read / write through the inner `Signal`. |
| `Ref::update<F>(&self, f: F) where F: FnOnce(T) -> T` | Apply a closure to the current value. |
| `Ref::set_if_changed(&self, value: T) where T: PartialEq` | Write only when the value differs. |
| `Ref::as_signal(&self) -> &Rc<Signal<T>>` / `Ref::into_signal(self) -> Rc<Signal<T>>` | Escape hatch to the underlying `Signal`. |
| `pub struct ShallowRef<T: Copy>(Rc<Cell<T>>)` | Lighter `Cell`-backed ref for `Copy` types; no reactivity, no subscribers. |
| `ShallowRef::new(value: T) -> Self`, `get(&self) -> T`, `set(&self, value: T)`, `update<F>(&self, f: F) where F: FnOnce(T) -> T` | Plain copy in, copy out. |
| `pub struct WatchOptions { pub deep: bool, pub immediate: bool }` | Options for `watch` / `watch_effect`. |
| `pub struct WatchHandle` | Stop button for a watcher (`stop()`, `is_active()`). |
| `pub struct LifecycleHandle` | Owns one component id and fires its mounted hooks exactly once. |
| `LifecycleHandle::new() -> Self`, `with_id(id) -> Self`, `id(&self) -> usize`, `mount(&self)`, `is_mounted(&self) -> bool` | Allocate/enter a component context, run mounted hooks once. |
| `pub struct TemplateRef<T>` | Interior-mutable ref-cell slot for template state (`new`, `with_value`, `get`, `set`, `clear`). |
| `pub trait VModel` | Two-way binding: `fn vmodel_set(&self, payload: &str)`. Implemented for `Cell<T>`, `RefCell<String>`, `Signal<T>` and `Ref<T>`. |

### Functions

| Signature | Description |
|---|---|
| `pub fn effect<F>(f: F) -> EffectHandle where F: FnMut() + 'static` | Run `f` now to collect dependencies, then again on every `set` of anything it read (`velox_core::signal::effect`). |
| `pub fn computed<T, F>(compute: F) -> Rc<Signal<T>> where T: Clone + PartialEq + 'static, F: Fn() -> T + 'static` | Derived signal that recomputes when its dependencies change (`velox_core::signal::computed`). |
| `pub fn batch(f: impl FnOnce())` | Defer subscriber flushes until `f` returns — a hundred `set`s cause one flush (`velox_core::signal::batch`). |
| `pub fn ref_value<T: Clone + 'static>(value: T) -> Ref<T>` | Vue-like `ref()`. |
| `pub fn from_signal<T: Clone + 'static>(signal: Rc<Signal<T>>) -> Ref<T>` | Wrap an existing `Signal` in a `Ref`. |
| `pub fn cell_ref<T: Clone + 'static>(signal: Rc<Signal<T>>) -> Ref<T>` | Alias of `from_signal`. |
| `pub fn to_signal<T: Clone + 'static>(r: &Ref<T>) -> &Rc<Signal<T>>` | Borrow the inner `Signal` from a `Ref`. |
| `pub fn shallow_ref<T: Copy>(value: T) -> ShallowRef<T>` | `Cell`-backed ref for `Copy` types. |
| `pub fn computed_ref<T: Clone + PartialEq + 'static, F: Fn() -> T + 'static>(compute: F) -> Ref<T>` | `computed`, returned as a `Ref`. |
| `pub fn readonly<T: Clone + PartialEq + 'static, F: Fn() -> T + 'static>(compute: F) -> Ref<T>` | Same as `computed_ref`, named to mark intent. |
| `pub fn watch_ref<T: Clone + PartialEq + 'static>(source: &Ref<T>, callback: impl FnMut(T, T) + 'static)` | Call `callback(new, old)` when a `Ref` changes; no callback on the first run. |
| `pub fn watch_effect<F: FnMut() + 'static>(f: F) -> EffectHandle` | Root-level `watch_effect`: run now, re-run on dependency change (`velox_core::watch_effect`). |
| `pub fn watch<T, S, F>(source: S, callback: F, options: WatchOptions) -> WatchHandle where T: PartialEq + Clone + 'static, S: FnMut() -> T + 'static, F: FnMut(T, T) + 'static` | General watcher (`velox_core::watch::watch`); fires only when `new != old`. |
| `pub fn watch_effect<F>(f: F, _options: WatchOptions) -> WatchHandle where F: FnMut() + 'static` | The options-taking form in `velox_core::watch`. |
| `pub fn next_tick<F>(callback: F) where F: FnOnce() + 'static` | Queue a callback for after the current reactive flush (`velox_core::next_tick::next_tick`). |
| `pub fn flush_sync()` | Synchronously drain the `next_tick` queue. |
| `pub fn provide<T: 'static>(key: &str, value: T)` | Publish a value to descendants (`velox_core::provide_inject`). |
| `pub fn inject<T: 'static>(key: &str) -> Option<Rc<T>>` | Read it back, searching up the context stack. |
| `pub fn inject_or<T: 'static>(key: &str, default: T) -> Rc<T>` | `inject` with a fallback. |
| `pub fn with_injection_context<F, R>(f: F) -> R where F: FnOnce() -> R` | Run `f` inside a fresh injection context. |
| `pub fn apply_str(target: &impl VModel, payload: &str)` | Sugar for `target.vmodel_set(payload)`. |
| `pub fn emit(_event: &str, _payload: &str)` / `pub fn emit_void(event: &str)` | Component event emission; a no-op placeholder until codegen supplies a real sink. |

### Lifecycle

All re-exported at the crate root from `velox_core::lifecycle`.

| Signature | Description |
|---|---|
| `pub fn generate_component_id() -> usize` | Allocate a fresh component id (returned through the private `ComponentId` alias). |
| `pub fn set_current_component(id: usize)` / `clear_current_component()` | Enter/leave a registration context. |
| `pub fn current_component_id() -> Option<usize>` | Read the active context. |
| `pub fn on_mounted(f: impl FnOnce() + 'static)` | Register a mount hook for the current component. |
| `pub fn on_updated(f: impl FnMut() + 'static)` | Register a re-render hook. |
| `pub fn on_unmounted(f: impl FnOnce() + 'static)` | Register an unmount hook. |
| `pub fn before_destroy(f: impl FnOnce() + 'static)` | Register a destroy hook. |
| `pub fn on_resize(f: impl FnMut(u32, u32) + 'static)` | Register a viewport-resize hook (module path: `velox_core::lifecycle::on_resize`). |
| `pub fn run_all_mounted_hooks()` / `run_all_updated_hooks()` / `run_all_destroy_hooks()` | Flush the corresponding hook registry. |
| `pub fn cleanup_component(id: usize)` | Drop every hook registered for `id`. |

### Macros

All exported at the crate root.

| Macro | Expands to |
|---|---|
| `signal!(count = 0)` | `Rc<Signal<T>>` — the name is documentary; the value carries the type. |
| `ref!(0)` written `r#ref!(0)` | `ref_value(0)` — a raw identifier, since `ref` is a keyword. |
| `shallow!(0)` | `shallow_ref(0)` — a `ShallowRef<T>` for `Copy` types. |
| `reactive! { count: i32 = 0, name: String = String::from("Hello") }` | An anonymous struct whose fields are `Ref<T>`. |
| `reactive_struct! { count: Ref<i32> = ref_value(0) }` | Same shape, initialisers supplied as full expressions. |
| `define_emits!(change, reset)` | `pub const EMIT_EVENTS: &[&str] = &["change", "reset"];` |
| `on_mounted! { … }`, `on_updated! { … }`, `on_unmounted! { … }`, `on_resize!(\|w, h\| …)` | Block-or-closure forms of the lifecycle registrars. |

## Example

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

## How it relates

```mermaid
graph TD
    core["velox-core<br/>(leaf)"]
    dom["velox-dom<br/>(leaf)"]
    style["velox-style"]
    renderer["velox-renderer"]
    sfc["velox-sfc"]
    ex["examples/*"]

    renderer -->|depends on| core
    renderer -->|depends on| dom
    renderer -->|depends on| style
    style -->|depends on| dom
    sfc -.->|dev-dependency only| core
    renderer -.->|dev-dependency only| sfc
    ex --> core
    ex --> dom
    ex --> style
    ex --> renderer
```

`velox-core` has no internal edges of its own: it is the leaf every other Velox crate reaches for when it needs state, effects or lifecycle.

## License

MIT
