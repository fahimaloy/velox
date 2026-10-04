# Tutorial: Build a Counter

The classic first app: a number, three buttons, and a label that reacts to both. You will build it as a single `.vx` component — a `<template>`, a `<script setup>` block and a `<style scoped>` block in one file — rendered natively with Skia.

Along the way you will meet the three ideas every Velox app is built on: the single-file component, reactive state, and the dev loop.

> Prerequisite: `veloxc` installed and Rust 1.91+. See [Getting Started](../getting-started.md) if you have not set up yet.

By the end of this tutorial you will have written:

- a `.vx` component with interpolation, `v-model`, and `@click` handlers
- reactive state driven by `Ref<T>`
- a `build.rs` that compiles `.vx` files at build time
- an entry point that wires the component to a native window

---

## Step 1 — Create the project

```bash
cargo new counter --bin
cd counter
```

Replace the generated `Cargo.toml` with:

```toml
[package]
name = "counter"
version = "0.1.0"
edition = "2024"

[workspace]

[dependencies]
velox-core = { git = "https://github.com/fahimaloy/velox" }
velox-dom = { git = "https://github.com/fahimaloy/velox" }
velox-style = { git = "https://github.com/fahimaloy/velox" }
velox-renderer = { git = "https://github.com/fahimaloy/velox", features = ["skia-native"] }

[build-dependencies]
veloxc = { git = "https://github.com/fahimaloy/velox" }
```

Four runtime crates and one build dependency:

| Crate | Role |
| --- | --- |
| `velox-core` | Reactive state: `Signal`, `Ref`, `computed` |
| `velox-dom` | The virtual DOM — the `VNode` tree your template compiles to |
| `velox-style` | The stylesheet parser |
| `velox-renderer` | The native window and Skia renderer (`skia-native` pulls the heavy dependency) |
| `veloxc` | The compiler, as a build dependency |

> Note: The empty `[workspace]` table makes this project its own workspace root. It is required if you create the project inside a Velox checkout and harmless anywhere else.

> Tip: `veloxc init` pins its git dependencies to a `rev` — the commit the CLI was built at — so a scaffold compiles against tested code. A crates.io-installed `veloxc` has no commit and falls back to `rev = "unknown"`, which Cargo rejects; scaffold from a clone or with `--local <path>` instead. Pinning a `rev` yourself is optional but recommended for anything you care about.

---

## Step 2 — Write the component

Create `src/App.vx`. The template first:

```html
<template>
  <div class="app">
    <h1 class="title">{{ title }}</h1>
    <div class="card">
      <p class="count">{{ count }}</p>
      <p class="status">{{ status }}</p>
      <div class="label-row">
        <span class="label-text">Label</span>
        <input class="label-input" v-model="label"/>
      </div>
      <div class="actions">
        <button class="btn inc" @click="increment">+1</button>
        <button class="btn dec" @click="decrement">-1</button>
        <button class="btn reset" @click="reset">Reset</button>
      </div>
    </div>
  </div>
</template>
```

Three pieces of syntax carry the whole page:

| Syntax | What it does |
| --- | --- |
| `{{ title }}` | Interpolation — the name inside the braces is a method on your `State`, called each time the tree is built |
| `v-model="label"` | Two-way binding — edits write through to the `label` ref, and re-renders read it back |
| `@click="increment"` | Event binding — the named method is called when the element is clicked |

Now the script block:

```rust
<script setup>
use velox_core::ergonomics::Ref;

pub struct State {
    // A single reactive integer.
    counter: Ref<i32>,
    // The text field's value, bound with `v-model="label"`.
    label: Ref<String>,
}

impl State {
    pub fn new() -> Self {
        Self {
            counter: velox_core::r#ref!(0),
            label: velox_core::r#ref!(String::from("counter")),
        }
    }

    pub fn title(&self) -> String {
        String::from("Velox Counter")
    }

    pub fn count(&self) -> i32 {
        self.counter.get()
    }

    pub fn label(&self) -> String {
        self.label.get()
    }

    // Derived: reads the same signal as `count`, so it updates on the
    // next frame after an increment.
    pub fn status(&self) -> String {
        if self.counter.get() > 0 {
            String::from("positive")
        } else {
            String::from("not positive")
        }
    }

    pub fn increment(&self) {
        self.counter.set(self.counter.get() + 1);
    }

    pub fn decrement(&self) {
        self.counter.set(self.counter.get() - 1);
    }

    pub fn reset(&self) {
        self.counter.set(0);
    }
}
</script>
```

What is happening here:

- **`State` is the single source of truth.** Every name the template references — `title`, `count`, `status`, `label`, `increment`, `decrement`, `reset` — resolves to a method on this struct.
- **`r#ref!(value)` creates reactive state.** It expands to `Ref::new(value)`, the ergonomic wrapper around `Rc<Signal<T>>` for scalar state.
- **Reads go through `.get()`, writes through `.set()`.** `count()` reads the signal so the rendered number is always what the ref holds; `increment()` writes to it.

Finally the style. `<style scoped>` rules apply only to this component — the compiler rewrites every selector with a per-component scope id:

```css
<style scoped>
.app {
    width: 100%;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 6px 14px 20px;
    background: #0f172a;
    color: #f1f5f9;
    font-family: system-ui, -apple-system, sans-serif;
}
/* A fixed width plus `margin: 0 auto` centers the card in the viewport. */
.card {
    display: block;
    width: 420px;
    margin: 0 auto;
    padding: 12px 14px;
    background: #1e293b;
    border-radius: 12px;
    text-align: center;
}
.count {
    display: block;
    margin: 0 0 4px 0;
    font-size: 34px;
    font-weight: 700;
    color: #38bdf8;
}
/* ... .title, .status, .label-row, .label-input, .actions, .btn and the
   button color variants follow the same pattern — see examples/counter
   for the complete stylesheet. */
</style>
```

---

## Step 3 — State and reactivity

Velox state is a `Signal<T>`: a value that tracks reads and notifies on writes. `Ref<T>` wraps one in an ergonomic cell — that is what you hold in `State`.

| API | What it does |
| --- | --- |
| `velox_core::r#ref!(value)` | Create a `Ref<T>` — the ergonomic wrapper around `Rc<Signal<T>>` |
| `velox_core::signal!(name = value)` | Create an `Rc<Signal<T>>` directly |
| `.get()` | Read the current value |
| `.set(new)` | Write a new value |
| `.update(f)` | Apply a closure to the current value and write the result back |
| `velox_core::signal::computed(f)` | A derived signal that re-runs when any signal it reads changes |

The reactivity model is deliberately simple:

1. Each time the tree is built, the generated resolver calls your `State` methods. `count()` calls `self.counter.get()`, so the rendered number is whatever the ref holds right now.
2. An event handler calls `.set(...)`.
3. The renderer picks the new value up on the next frame — `status()` reads the same signal, so it updates with it.

There is no dependency graph to wire up by hand: a method that reads a signal is live, and a write is visible on the next frame.

`v-model` is the same machinery in both directions. `VModel` is implemented for `Ref<T>`, so the generated setter is a fully-qualified `VModel::vmodel_set(&self.label, payload)` call — and fully-qualified calls do not auto-deref, which is why the `VModel` impl lives on `Ref<T>` itself rather than being inherited from `Signal` through `Deref`. Editing the input writes to the ref; the next re-render reads it back.

---

## Step 4 — Compile at build time and wire the window

### `build.rs`

The `.vx` file is not Rust — the compiler runs as a build dependency and generates Rust into `OUT_DIR` before your crate compiles:

```rust
// build.rs
fn main() {
    // Re-run if the .vx file changes.
    println!("cargo:rerun-if-changed=src/App.vx");

    let vx_path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("src/App.vx");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    veloxc::build_cmd(&vx_path, Some(&out_dir), veloxc::EmitMode::Render)
        .expect("failed to compile .vx file");
}
```

`EmitMode::Render` recursively compiles the input file and everything it imports, and emits `cargo:rerun-if-changed` directives for every `.vx` file it reads — so the generated code is always in sync with the source.

### `src/main.rs`

```rust
use std::sync::Arc;

use velox_dom::VNode;
use velox_style::Stylesheet;

// Include the generated code from build.rs.
include!(concat!(env!("OUT_DIR"), "/app.rs"));

fn main() {
    let state = Arc::new(app::script_rs::State::new());

    // make_view: build the tree. Called with the current viewport size
    // (w, h) so the layout reflows on resize.
    let make_view = {
        let state = Arc::clone(&state);
        move |w: u32, h: u32| -> (VNode, Stylesheet) {
            let _viewport = (w, h);
            let vnode =
                app::render_with_state(Arc::clone(&state), app::make_resolve(Arc::clone(&state)));
            let sheet = Stylesheet::parse(app::STYLE);
            (vnode, sheet)
        }
    };

    let on_event = app::make_on_event(Arc::clone(&state));

    let get_title = || "Velox Counter".to_string();

    // HMR when the dev server is running; otherwise a normal window.
    if let Some(port) = velox_renderer::hmr_config() {
        let (hmr_tx, hmr_rx) = std::sync::mpsc::channel::<velox_renderer::HmrMessage>();
        let hmr_rx = Arc::new(std::sync::Mutex::new(hmr_rx));
        velox_renderer::run_hmr_client(port, hmr_tx);
        let _ = velox_renderer::run_window_vnode_skia_with_hmr(
            "Velox Counter",
            make_view,
            on_event,
            get_title,
            hmr_rx,
        );
    } else {
        let _ =
            velox_renderer::run_window_vnode_skia("Velox Counter", make_view, on_event, get_title);
    }
}
```

The four pieces the window needs:

| Piece | Role |
| --- | --- |
| `make_view` | Builds the tree: calls the generated `render_with_state` with the generated resolver, and parses the generated stylesheet |
| `on_event` | The generated event dispatcher — routes clicks to your `State` methods |
| `get_title` | The window title |
| The `hmr_config()` branch | When `VELOX_HMR=1` is set (the dev server does this), the app connects to the dev server and reloads on demand; otherwise it opens a normal window |

> Note: `State` holds `Rc<Signal<T>>`, so it is `!Send` by construction — Velox is one thread per window, by design. The `Arc` is not a cross-thread choice: it is the parameter type the codegen emits, and `Arc::clone` only hands the same state to several of the generated calls.

---

## Step 5 — The dev loop

```bash
veloxc dev
```

The dev server compiles the project, opens the window, and watches `src/`. Now change something and save:

- change the button label from `+1` to `Plus one` — the window comes back with the new label
- change `.count` color from `#38bdf8` to `#4ade80` — same loop

Each save is debounced, classified by which block of the file changed, and answered with a rebuild and an app restart:

```text
↻ src/App.vx changed (TemplateOnly) — rebuilding
⏳ Compiling...
✓ Compiled in 0.4s
```

You do not touch the terminal. Edit, save, look at the window. `r` rebuilds by hand, `c` clears the terminal, `q` quits.

![The counter dev loop — save, rebuild, the window comes back](../assets/counter-hmr.gif)

---

## Step 6 — Run and build

For development, keep `veloxc dev` running. For a release build:

```bash
veloxc build --release
veloxc run --release
```

The finished project also lives in the Velox repository at `examples/counter`, with path dependencies instead of git ones:

```bash
git clone https://github.com/fahimaloy/velox
cd velox
cargo run -p velox-example-counter --bin counter
```

---

## Exercises

1. **Change the starting count.** Replace `r#ref!(0)` with `r#ref!(10)` and confirm the dev loop picks it up.
2. **Make the status smarter.** Change `status()` to return `"even"` / `"odd"` instead of `"positive"` / `"not positive"`.
3. **Add a step.** Add a `step: Ref<i32>` field and a second input bound with `v-model`, then make `increment` add `step.get()` instead of `1`.
4. **Inspect the generated code.** Run `veloxc build src/App.vx --out-dir gen` and read `gen/app.rs` — find the resolver that calls your `State` methods and the event dispatcher that routes `@click`.

Then continue to the [Todo App](todo.md) tutorial, where the same ideas scale to components, computed state, and list rendering.
