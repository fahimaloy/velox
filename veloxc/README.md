# veloxc

The Velox CLI and dev server: scaffold, lint, build, run and watch `.vx` apps.

[![Crates.io](https://img.shields.io/crates/v/veloxc.svg)](https://crates.io/crates/veloxc) [![Downloads](https://img.shields.io/crates/d/veloxc.svg)](https://crates.io/crates/veloxc) [![docs.rs](https://img.shields.io/docsrs/veloxc)](https://docs.rs/veloxc) [![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/veloxc.svg)](https://crates.io/crates/veloxc) [![CI](https://img.shields.io/github/actions/workflow/status/fahimaloy/velox/ci.yml?branch=main)](https://github.com/fahimaloy/velox/actions/workflows/ci.yml)

## Overview

`veloxc` is the tooling front of the Velox workspace: a single binary that scaffolds new projects, lints `.vx` files, compiles single-file components to Rust (via `velox-sfc`), resolves CSS (via `velox-style`), links compiled apps (via `velox-renderer`), and runs a watch-and-rebuild dev server with an HMR control channel. Scaffolds wire your `Cargo.toml` to the six published workspace crates, and every command is `veloxc <subcommand>` — `veloxc --help` lists them, and `veloxc <subcommand> --help` is the authoritative flag reference.

## Features

- **`veloxc init`** — scaffolds a working todo app: `Cargo.toml` wired to Velox, a `build.rs` that compiles `.vx` files, assets, and a component tree
- **`veloxc dev`** — watch-and-rebuild dev server: inotify-driven (no polling), debounced, with an HMR control channel on `127.0.0.1:31313`
- **`veloxc build`** — project build, or compile one `.vx` file — and every component it imports — to Rust for inspection
- **`veloxc lint`** — parse errors, reactive-idiom advisories in `<script>`, and every CSS property that parses but paints nothing; `--fix` repairs trailing whitespace and missing final newlines
- **`veloxc add component`** — PascalCase component scaffolds with named slots
- **Dependency wiring** — scaffolds use relative `path` deps when built from a clone (or `--local <path>`), crates.io `git` pins otherwise

## Installation

Requires Rust 1.85+ and Git.

```bash
cargo install veloxc
```

This installs the `veloxc` binary and pulls the three Velox crates it drives from crates.io: `velox-sfc` (`.vx` parsing and template code generation), `velox-style` (CSS), and `velox-renderer` (painting, events, HMR).

`cargo add veloxc` instead adds it as a build dependency — that is how scaffolded projects use it: their `build.rs` calls `veloxc::build_cmd` on `src/App.vx` on every `cargo build` (emitting `cargo:rerun-if-changed` for each `.vx` it reads), and `src/main.rs` `include!`s the generated module.

Working from a checkout:

```bash
git clone https://github.com/fahimaloy/velox.git
cd velox
cargo install --path veloxc --force
```

## Quick start

```bash
# 1. Scaffold
veloxc init my-app
cd my-app

# 2. Watch, build, run — reload with `r`, quit with `q`
veloxc dev

# 3. One-shot commands
veloxc lint src/            # check every .vx file
veloxc run                  # build + run once
veloxc build --release      # optimized build
```

`veloxc dev` starts the dev server:

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

## Documentation

- API reference: [docs.rs/veloxc](https://docs.rs/veloxc)
- Book: [CLI reference](https://fahimaloy.github.io/velox/features/cli.html) — every subcommand and flag
- Full CLI reference in the repository README: [`README.md`](https://github.com/fahimaloy/velox/blob/main/README.md#cli-reference)

## Workspace crates

Velox is a workspace of six published crates. This crate is marked **(this crate)**.

| Crate | What it provides | Documentation |
|:---|:---|:---|
| [velox-core](https://docs.rs/velox-core) | Reactive primitives: signals, effects, lifecycle hooks | [docs.rs](https://docs.rs/velox-core) |
| [velox-dom](https://docs.rs/velox-dom) | `VNode` types, block + flex layout, positioning, overflow | [docs.rs](https://docs.rs/velox-dom) |
| [velox-style](https://docs.rs/velox-style) | CSS parsing, selectors, cascade; styles applied to `VNode` | [docs.rs](https://docs.rs/velox-style) |
| [velox-renderer](https://docs.rs/velox-renderer) | Rendering backends, event dispatch, hit testing | [docs.rs](https://docs.rs/velox-renderer) |
| [velox-sfc](https://docs.rs/velox-sfc) | Single-file component compiler (pest grammar) → Rust | [docs.rs](https://docs.rs/velox-sfc) |
| **[veloxc](https://docs.rs/veloxc)** · this crate | The CLI and dev server | [docs.rs](https://docs.rs/veloxc) |

## Status

Published on [crates.io](https://crates.io/crates/veloxc) at **0.1.1**.

All seven subcommands work: `init`, `build`, `run`, `dev`, `lint`, `add component`, `version`. Known limitations, stated plainly:

- **`veloxc dev` is a watch-and-rebuild loop, not hot reload.** Module-level `HotReload` exists in the HMR protocol but is not implemented — every save is a full rebuild followed by an app restart. The HMR port is fixed at 31313 (there is no port flag).
- **The crates.io scaffold's git pin falls back to `rev = "unknown"`** and Cargo fails with ``revspec 'unknown' not found``. Until the fallback is fixed, scaffold from a clone (`git clone https://github.com/fahimaloy/velox && cargo install --path veloxc`), pass `--local <path>`, or drop the `git = …` / `rev = …` keys from every `velox-*` line in the generated `Cargo.toml`.
- **`veloxc`'s dev-dependencies never reach a scaffolded app.** They exist so integration tests can drive the real layout pass and run the pixel-level scoping proof in the default `cargo test` pass; the scaffolded `Cargo.toml` is generated independently and lists only the runtime crates.
- `veloxc version` prints a hardcoded `Edition: 2021` informational line (`veloxc/src/bin/main.rs:196`) — the workspace itself builds with edition 2024.

## License

Licensed under **MIT OR Apache-2.0**.

Copyright © FAHIM AHMED <fahimaloy@tutamail.com>
