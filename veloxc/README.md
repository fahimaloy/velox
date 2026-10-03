# veloxc

CLI tools for building Velox applications — scaffold, lint, build, and watch `.vx` single-file-component projects.

[![Crates.io](https://img.shields.io/crates/v/veloxc.svg)](https://crates.io/crates/veloxc) [![Downloads](https://img.shields.io/crates/d/veloxc.svg)](https://crates.io/crates/veloxc) [![License](https://img.shields.io/crates/l/veloxc.svg)](https://crates.io/crates/veloxc) [![Docs](https://img.shields.io/docsrs/veloxc)](https://docs.rs/veloxc)

## Installation

Requires Rust 1.85+ and Git.

```bash
cargo install veloxc
```

This installs the `veloxc` binary and pulls the three Velox crates it drives from crates.io: `velox-sfc` (`.vx` parsing and template code generation), `velox-style` (CSS), and `velox-renderer` (painting, events, HMR).

Verify:

```text
$ veloxc version
velox 0.1.1
Edition: 2021
Platform: Linux
```

### From source

```bash
cargo install --git https://github.com/fahimaloy/velox veloxc
```

`veloxc` is the `veloxc/` member of the repository's Cargo workspace; cargo resolves it by package name from the git checkout, together with the sibling crates it depends on by path.

To work on the CLI itself:

```bash
git clone https://github.com/fahimaloy/velox.git
cd velox
cargo install --path veloxc --force
```

## Quick start

```bash
veloxc init my-app
cd my-app
veloxc dev
```

`veloxc init` scaffolds a working todo app (see [Scaffolded project layout](#scaffolded-project-layout)). Its real output:

```text
✅ Created Velox project: my-app
📦 To get started:
   cd my-app
   velox dev
   velox build
   velox run
✅ Created Velox project at: my-app
📖 Next steps:
   cd my-app
   velox dev
```

The printed hints say `velox …`; the installed binary is named `veloxc`.

`veloxc dev` then starts the dev server:

```text
[velox] HMR dev server listening on 127.0.0.1:31313
  ⚡ Velox dev server v0.1.1
  ➤ Project: my-app
  ➤ Watching: ./src
  ➤ Watching: ./assets
  ➤ HMR: port 31313 (auto-reload on save)
  ➤ Build: debug
    r: reload   c: clear   q: quit

⏳ Compiling...
```

It builds the project, runs the app, and rebuilds on every save. This is a watch-and-rebuild loop: each change is a full `cargo build` followed by an app restart, so in-process state does not survive a save. Treat rebuild time as the cost of every edit.

Next steps in a scaffolded project:

```bash
veloxc lint src/            # check every .vx file
veloxc run                  # build + run once
veloxc build --release      # optimized build
```

## CLI reference

Usage: `veloxc <COMMAND>`. `veloxc --help` and `veloxc <COMMAND> --help` print the generated help.

| Command | Purpose |
|---|---|
| `veloxc init <name>` | Scaffold a new Velox project |
| `veloxc build [file.vx]` | Build the current project, or compile a `.vx` tree to Rust |
| `veloxc run` | Build and run the current project |
| `veloxc dev` | Dev server: watch, rebuild, restart |
| `veloxc lint [target]` | Check `.vx` files for errors; `--fix` repairs what it can |
| `veloxc add component <name>` | Generate a component `.vx` file |
| `veloxc version` | Version and platform info |

### `veloxc init`

```text
veloxc init <name> [-t <template>] [--local <path>]
```

| Argument / flag | Description |
|---|---|
| `<name>` | Project name or path (`my-app`, `../projects/my-app`, `/tmp/my-app`). Normalized to a valid Cargo package name: dots and whitespace become hyphens, and the first character must be a letter or `_`. |
| `-t, --template <name>` | Template variant (default: `default`). Accepted for compatibility; every variant currently scaffolds the same built-in project. |
| `--local <path>` | Generate a project wired to a local Velox checkout at `<path>` instead of the default dependency source (see below). |

**Where the generated dependencies come from.** The scaffolded `Cargo.toml` is produced by one of three modes, checked in this order:

| Condition | Generated dependencies |
|---|---|
| `--local <path>` (or `VELOX_PATH` in the environment) and `<path>/velox-core/Cargo.toml` exists | Relative `path` dependencies into that checkout: `velox-core`, `velox-dom`, `velox-style`, `velox-renderer` (with `skia-native`), plus `veloxc` as a build-dependency — each with `version` set to the CLI's own version |
| A Velox workspace is found — walking up from the current directory to a `velox-core/Cargo.toml`, or a binary that was built inside a checkout | The same relative `path` dependencies, into that workspace |
| Neither | `git = "https://github.com/fahimaloy/velox"` dependencies pinned to `rev`, the commit the CLI was built from (`VELOX_GIT_REV`; `unknown` when built outside a git checkout) |

The generated manifest also declares an empty `[workspace]`, so the new app is its own workspace root.

### `veloxc build`

```text
veloxc build [--release]              # build the current project
veloxc build <file.vx> [-o <dir>]     # compile a .vx component tree to Rust
```

| Flag | Applies to | Description |
|---|---|---|
| `--release` | project mode | Runs `cargo build --release` |
| `-o, --out-dir <dir>` | `.vx` mode | Where generated `.rs` files are written (default: `target/velox-gen`) |

With no input, this is `cargo build`. With a `.vx` file, it compiles that component **and every component it imports**, recursively, into one `.rs` file per component, printing `[velox] Generated: …` for each. The output is source code for inspection, not an executable — the scaffolded `build.rs` runs the same compilation into `OUT_DIR` on every `cargo build`.

### `veloxc run`

```text
veloxc run [--release]
```

Builds and runs the current project (`cargo run`, with `cargo build --release` first when `--release` is given).

### `veloxc dev`

```text
veloxc dev [-w <path>] [--release]
```

| Flag | Default | Description |
|---|---|---|
| `-w, --watch <path>` | `.` | Project root to build and watch |
| `--release` | — | Build in release mode |

Behavior, as implemented:

- Watches `./src` and `./assets` (or the whole project root if there is no `src/`). `target/`, `.git/`, `.vscode/`, `.idea/` and every dot-directory are excluded — `cargo build` writes thousands of files into `target/`, and watching it burns the kernel's inotify watch budget.
- Change reports come from the OS (inotify on Linux, via `notify`), not polling. A burst of saves from one editor write is debounced into a single rebuild, and a change recorded while a build is running becomes one follow-up build.
- Every detected change costs a `cargo build` and an app restart, including style-only edits (the cheaper paths are classified but not yet wired). A compile error prints diagnostics and keeps the watcher alive; the next save retries.
- The HMR listener binds `127.0.0.1:31313` (fixed — there is no port flag) before the banner is drawn, and the app is spawned with `VELOX_HMR` set. If the port is taken, the banner says `unavailable` with the reason instead of claiming auto-reload.
- Keys: `r` reload, `c` clear, `q` quit.
- If the watcher fails (typically an exhausted inotify limit), the error is reported with its remedy and the server keeps running — it never goes silently blind:

```bash
sudo sysctl -w fs.inotify.max_user_watches=524288
sudo sysctl -w fs.inotify.max_user_instances=1024
```

### `veloxc lint`

```text
veloxc lint [target] [--fix]
```

| Target | Effect |
|---|---|
| *(none)* | Lint every `.vx` file under `src/` |
| a `.vx` file | Lint that file |
| a directory | Lint every `.vx` file under it, recursively |

Parse errors fail the command (`Lint failed with N errors`). Everything else is advisory and never fails a run:

- Reactive-idiom warnings for `Cell`/`RefCell` usage in script blocks.
- CSS declarations that Velox **parses but never paints**. These survive the cascade and produce no visual change today: `box-shadow`, `font-style`, `letter-spacing`, `visibility`, `overflow-x`, `overflow-y`, `background-image`, `transition`, `border-style`, `border-color`, and `transform` (stacking only). `visibility: hidden` hides nothing — authoring it looks correct and does nothing.

`--fix` repairs what it can — trailing whitespace and a missing final newline. Parse errors are not auto-fixable and leave the file untouched. The summary line reads `Lint results: N files, M errors`.

### `veloxc add component`

```text
veloxc add component <name> [-s <NAME,NAME>]
```

| Argument / flag | Description |
|---|---|
| `<name>` | Component name, either style — `Counter` or `side-bar` — scaffolded as PascalCase (`src/components/SideBar.vx`) with a kebab-case CSS class |
| `-s, --slots <NAME,NAME>` | Named slots to declare, comma-separated. The implicit `default` slot is always generated; `default` itself cannot be requested |

Finds the project root by walking up to a `src/App.vx`, refuses to overwrite an existing file, and prints the fill hint:

```text
✅ Created component: src/components/Counter.vx
   Slots: default, header
   Fill one from a parent: <Counter><template v-slot:header>…</template></Counter>
   (shorthand: <Counter><template #header>…</template></Counter>)
   Import it from a parent component, e.g.:
   import Counter from './components/Counter.vx'
```

### `veloxc version`

```text
$ veloxc version
velox 0.1.1
Edition: 2021
Platform: Linux
```

`Platform` reports `Linux`, `macOS`, or `Windows`.

## How veloxc fits the Velox stack

| Crate | Role in a veloxc project |
|---|---|
| `veloxc` | This CLI: scaffolds projects, lints `.vx`, compiles `.vx` to Rust, runs the dev server |
| `velox-sfc` | Compiles `.vx` single-file components — parses the blocks, resolves component imports, generates Rust from the template |
| `velox-style` | Parses and resolves the CSS — cascade and selectors — into the stylesheet the app applies |
| `velox-renderer` | Paints the laid-out tree with Skia, dispatches events, and speaks the HMR protocol the dev server uses |

A scaffolded app wires this together: its `build.rs` calls `veloxc::build_cmd` on `src/App.vx` in render mode on every `cargo build` (emitting `cargo:rerun-if-changed` for each `.vx` it reads), and `src/main.rs` `include!`s the generated module. The app itself depends on `velox-core`, `velox-dom`, `velox-style`, and `velox-renderer` (with `skia-native`), with `veloxc` as a build-dependency.

One nuance worth preserving: **`veloxc`'s dev-dependencies never reach a scaffolded app.** `velox-dom` is a dev-dependency because integration tests assert on styled vnodes and drive the real layout pass directly, and `velox-renderer` is declared there *with* the `skia-native` feature so the pixel-level scoping proof runs in the default `cargo test` pass rather than behind `#[ignore]`. Both are test-only; the scaffolded `Cargo.toml` is generated independently and lists only the runtime crates.

## Scaffolded project layout

`veloxc init my-app` creates:

```text
my-app/
├── Cargo.toml            # generated; dependency source chosen as described under `init`
├── build.rs              # compiles src/App.vx (and its imports) into OUT_DIR
├── README.md             # project README with structure and feature notes
├── assets/
│   ├── velox-logo.svg    # referenced by App.vx <img src="assets/…">
│   └── velox-logo.png    # referenced by Modal.vx
└── src/
    ├── main.rs           # entry point; include!s the generated module
    ├── App.vx            # root component: page chrome, theme toggle, dialogs
    └── components/
        ├── Todos.vx      # todo list, draft text, theme flag
        ├── TodoInput.vx  # text input
        ├── TodoItem.vx   # a single todo row
        ├── Modal.vx      # reusable dialog: title, message, cancel/confirm
        └── Confirm.vx    # reusable destructive question: cancel/accept
```

The result is a working todo app with a light/dark theme and two reusable dialog components.

## Template syntax

`.vx` files are Velox's single-file component format: a `<template>` block, a `<script setup>` block of Rust state, and a `<style>` (or `<style scoped>`) block, in one file.

```html
<template>
  <div class="container">
    <h1>{{ title }}</h1>
    <p v-if="count > 0">Count is positive</p>
    <p v-else>Count is zero or negative</p>
    <button @click="increment">Increment</button>
  </div>
</template>

<script setup>
pub struct State { /* Rust state */ }
</script>

<style scoped>
.container { padding: 24px; font-size: 18px; }
</style>
```

| Syntax | Meaning |
|---|---|
| `{{ expr }}` | Text interpolation |
| `v-if="cond"` / `v-else-if="cond"` / `v-else` | Conditional rendering; the chain must be adjacent siblings |
| `v-for="(item, i) in items"` | List rendering (`item in items` also works) |
| `@event="handler"` | Event binding — `click`, `input`, `change`, keyboard, mouse |
| `:prop="expr"` | Dynamic attribute/prop binding — `:class`, component props such as `:title` |
| `:key` | Keyed child inside a `v-for` (order-preservation hint) |
| `<slot>` | Implicit `default` slot; fill named slots with `<template v-slot:name>…</template>` or the `#name` shorthand |
| `import Child from './components/Child.vx'` | Component import, written in the script block |

Styling notes, so you do not discover them by failure:

- `<style scoped>` rewrites the block's selectors to a per-component `data-v-*` attribute; a bare `<style>` stays global. Scoped CSS cannot cross a component boundary — themed elements need a carrier class inside each component.
- There is no `var()` and no `@media`.
- There are no HTML comments inside `<template>` — the parser has no comment syntax for them. Put comments in the script or style block.
- `transform` is parsed (it participates in stacking) but does not visually transform anything, and properties such as `box-shadow` and `visibility` parse without painting — `veloxc lint` reports each one.

## Links

| | |
|---|---|
| Documentation | <https://velox.dev/docs/cli> |
| Repository | <https://github.com/fahimaloy/velox> |
| Changelog | <https://github.com/fahimaloy/velox/blob/main/CHANGELOG.md> |
| License (MIT) | <https://github.com/fahimaloy/velox/blob/main/LICENSE> |
| API docs | <https://docs.rs/veloxc> |
