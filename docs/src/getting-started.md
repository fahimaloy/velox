# Getting Started

Velox is a Rust UI framework with Vue-like single-file components: you write `.vx` files — a `<template>`, a `<script setup>` block and a `<style scoped>` block in one file — and they compile to Rust that renders natively with Skia. This guide takes you from a clean machine to a running app in a few minutes.

---

## Prerequisites

- **Rust 1.91 or later.** Install with [rustup](https://rustup.rs) and check with `rustc --version`.
- **A desktop platform.** Linux, macOS, and Windows are all supported.

| Platform | Notes |
| --- | --- |
| Linux | File watching is backed by inotify, a finite kernel resource. If your watch budget is exhausted the dev server tells you and keeps running — see [Troubleshooting](#troubleshooting). |
| macOS | No extra setup. |
| Windows | The native renderer build is not target-gated; GPU-backed rendering is Unix-only. |

---

## Install veloxc

`veloxc` is both the compiler and the CLI: it turns `.vx` files into Rust at build time and drives the dev server.

### From crates.io

```bash
cargo install veloxc
```

Verify the install:

```bash
veloxc version
```

This prints the version, the edition, and the platform.

> Note: the installed binary is `veloxc`; the CLI's own help text says `velox`, so every `velox …` command in this guide is written as `veloxc …` to make copy-paste work.

### From source

```bash
git clone https://github.com/fahimaloy/velox
cd velox
cargo install --path veloxc
```

---

## Create a project

```bash
veloxc init my-app
cd my-app
```

`veloxc init` scaffolds a complete, working project — not a stub. The generated app is a todo list with a composer, filters, a light/dark theme toggle, and two dialogs, so everything worth learning from is on screen from the first run.

```text
my-app/
├── Cargo.toml
├── build.rs
├── README.md
├── assets/
│   ├── velox-logo.svg
│   └── velox-logo.png
└── src/
    ├── main.rs
    ├── App.vx
    └── components/
        ├── Todos.vx
        ├── TodoInput.vx
        ├── TodoItem.vx
        ├── Confirm.vx
        └── Modal.vx
```

| File | Role |
| --- | --- |
| `Cargo.toml` | Dependencies on the Velox crates, `serde_json`, and `veloxc` as a build dependency |
| `build.rs` | Compiles `src/App.vx` — and every component it imports — into Rust in `OUT_DIR` |
| `src/main.rs` | Entry point: builds state, hands the render and event closures to the window |
| `src/App.vx` | Root component: page chrome, theme toggle, dialogs |
| `src/components/*.vx` | The list, the input, a single row, and two reusable dialogs |
| `assets/` | Static assets; `<img src="…">` paths resolve against the project root |

> Warning: a **crates.io-installed** `veloxc` has no git revision recorded — it falls back to `"unknown"` and the generated `Cargo.toml` gets `rev = "unknown"`, which Cargo rejects with ``revspec 'unknown' not found``. Until that fallback is fixed, scaffold from a clone (`git clone https://github.com/fahimaloy/velox && cargo install --path veloxc`), pass `veloxc init my-app --local <path>`, or drop the `git = …` / `rev = …` keys from every `velox-*` line in the generated `Cargo.toml` and keep the `version` key.

> Note: `veloxc init` writes **path** dependencies when it can find a Velox workspace — the `VELOX_PATH` environment variable, a workspace above the current directory, or the checkout the CLI itself was built in. Otherwise it writes **git** dependencies pinned to the commit the CLI was built at, so a scaffold always compiles against tested code.

> Tip: Package names must start with a letter or `_`. Dots and spaces in the name are normalized to dashes: `veloxc init my.app` creates `my-app`.

---

## Run the dev server

```bash
veloxc dev
```

The first run compiles the project (a full cargo build — it takes a minute), prints a banner, and opens the app window:

```text
  ⚡ Velox dev server v0.1.1
  ➤ Project: my-app
  ➤ Watching: /home/you/my-app/src
  ➤ Watching: /home/you/my-app/assets
  ➤ HMR: port 31313 (auto-reload on save)
  ➤ Build: debug

    r: reload   c: clear   q: quit
```

Every line is a statement about the run you are in. The `HMR` line says `unavailable: <reason>` if the port could not be bound, rather than advertising a reload that cannot happen. `Watching` lists one directory per line — the dev server watches `src/` and `assets/`, not the project root, so `target/` writes never burn the file watcher's budget.

While the server runs, type one of:

| Key | Action |
| --- | --- |
| `r` | Rebuild and restart the app by hand |
| `c` | Clear the terminal and redraw the banner |
| `q` | Stop the dev server (and the app) |

Now edit `src/App.vx` and save. The loop does the rest: the save is debounced, classified by which block of the file changed (`<style>`, `<template>`, or script), and answered with a rebuild and an app restart. A compile error does not stop the server — the diagnostics print, the watcher stays alive, and the next save retries. If the app itself exits, the server tells you and keeps watching: press `r` to restart, or save a file to rebuild.

![The veloxc dev workflow — the dev server in the terminal, the app window beside it](assets/velox-dev-window.png)

---

## Build for release

```bash
veloxc build --release
```

This runs `cargo build --release` for the current project. Start the built app with:

```bash
veloxc run --release
```

`veloxc run` builds first when `--release` is passed, then opens the window.

`veloxc build` also compiles a single `.vx` file to Rust source — useful when you want to read what the compiler generates from your component:

```bash
veloxc build src/App.vx --out-dir gen
```

This writes `gen/app.rs`, the compiled component as a Rust module. With no `--out-dir`, the output lands in `target/velox-gen`.

> Note: The generated file is Rust **source**, not an executable — the CLI says so itself when you run this form.

---

## Troubleshooting

### Files are no longer detected on save (Linux)

inotify is a finite kernel resource. When `fs.inotify.max_user_watches` is exhausted, the dev server reports the error and keeps running, but saves may no longer be detected. Raise the limit:

```bash
sudo sysctl -w fs.inotify.max_user_watches=524288
sudo sysctl -w fs.inotify.max_user_instances=1024
```

To persist it, write those two lines to a file in `/etc/sysctl.d/` and run `sudo sysctl --system`. The dev server also honors `r` to rebuild by hand while watching is degraded.

---

## Next steps

| Tutorial | You build |
| --- | --- |
| [Counter](tutorials/counter.md) | A reactive counter — state, events, `v-model`, and the dev loop |
| [Todo App](tutorials/todo.md) | Component composition, computed state, list rendering, and events |
| [Showcase](tutorials/showcase.md) | A guided tour of the layout engine — centering, flex, scroll, and wrap |

For the full command surface — `lint`, `add component`, and every flag — see the [CLI Reference](features/cli.md).
