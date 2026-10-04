# velox-dom

Virtual DOM, flexbox layout and text engine for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-dom.svg)](https://crates.io/crates/velox-dom) [![Downloads](https://img.shields.io/crates/d/velox-dom.svg)](https://crates.io/crates/velox-dom) [![docs.rs](https://img.shields.io/docsrs/velox-dom)](https://docs.rs/velox-dom) [![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/velox-dom.svg)](https://crates.io/crates/velox-dom) [![CI](https://img.shields.io/github/actions/workflow/status/fahimaloy/velox/ci.yml?branch=main)](https://github.com/fahimaloy/velox/actions/workflows/ci.yml)

## Overview

`velox-dom` is the tree: a `VNode`/`Props` element model, a flexbox-plus-block layout engine, the computed-style vocabulary the whole framework shares, keyed diffing, and greedy text wrapping. It draws nothing and parses nothing — paint belongs to `velox-renderer`, CSS parsing to `velox-style`. The crate has zero dependencies, which is why it can be used headless in tests, in a build script, or as the geometry authority for a pixel-proof suite.

## Features

- **`VNode`/`Props` element model** — pure data, no state cell and no instance handle, built with `h()` and `text()`, with keyed children
- **Flexbox + block layout** — `compute_layout` against a viewport: block and flex layout, `static`/`relative`/`absolute`/`fixed`/`sticky` positioning, z-index, overflow clipping
- **Computed-style vocabulary** — `ComputedStyle`, `Length`, `Color`, the flex/position/overflow enums: the type language the whole framework shares
- **Keyed diffing** — `diff::diff` produces duplicate-key-safe patch lists
- **Text wrapping** — greedy wrap with `WhiteSpace`/ellipsis handling, measured line extents, DPI-scale awareness
- **Scroll state** — `ScrollState`, `max_scroll_y` and `is_scrollable` for scrollable boxes
- **Zero dependencies** — no feature flags; headless in tests, build scripts and pixel-proof suites

## Installation

```bash
cargo add velox-dom
```

No dependencies and no feature flags. The `[features]` section in `velox-dom/Cargo.toml` carries only comments — it documents how the opt-in headless pixel-proof tests in `tests/` are meant to be gated, but no feature is declared, so nothing is compiled out or in by feature selection.

## Quick start

```rust
use velox_dom::{Props, h, text, layout::compute_layout};

let node = h(
    "div",
    Props::new().set("style", "width: 300px; height: 200px;"),
    vec![text("hello"), h("span", (), vec![text("world")])],
);

let layout = compute_layout(&node, 800, 600);
assert_eq!(layout.rect.w, 300);
assert_eq!(layout.rect.h, 200);
assert_eq!(layout.children.len(), 2);
```

## Documentation

- API reference: [docs.rs/velox-dom](https://docs.rs/velox-dom)
- Book: [the Showcase tutorial](https://fahimaloy.github.io/velox/tutorials/showcase.html) — a layout gallery: spacing, flex, centering, wrapping

## Workspace crates

Velox is a workspace of six published crates. This crate is marked **(this crate)**.

| Crate | What it provides | Documentation |
|:---|:---|:---|
| [velox-core](https://docs.rs/velox-core) | Reactive primitives: signals, effects, lifecycle hooks | [docs.rs](https://docs.rs/velox-core) |
| **[velox-dom](https://docs.rs/velox-dom)** · this crate | `VNode` types, block + flex layout, positioning, overflow | [docs.rs](https://docs.rs/velox-dom) |
| [velox-style](https://docs.rs/velox-style) | CSS parsing, selectors, cascade; styles applied to `VNode` | [docs.rs](https://docs.rs/velox-style) |
| [velox-renderer](https://docs.rs/velox-renderer) | Rendering backends, event dispatch, hit testing | [docs.rs](https://docs.rs/velox-renderer) |
| [velox-sfc](https://docs.rs/velox-sfc) | Single-file component compiler (pest grammar) → Rust | [docs.rs](https://docs.rs/velox-sfc) |
| [veloxc](https://docs.rs/veloxc) | The CLI and dev server | [docs.rs](https://docs.rs/veloxc) |

## Status

Published on [crates.io](https://crates.io/crates/velox-dom) at **0.1.1**.

Working today: block layout, flex layout, `static`/`relative`/`absolute`/`fixed`/`sticky` positioning, z-index, overflow clipping, keyed diffing, and text wrapping. Two notes worth knowing before you rely on them:

- The keyed reconciler (`diff::diff`) ships complete but has **zero production callers** — Velox's render loop is immediate-mode, with no retained tree or patch applier. Full decision record: [`docs/internal/RECONCILER.md`](https://github.com/fahimaloy/velox/blob/main/docs/internal/RECONCILER.md).
- `v-for` reordering correctness comes from `VNode` child order, not from `:key`. An `<input>` inside a reordered `v-for` loses focus/caret to whichever item occupies its index. This is a known limitation, not an oversight.

## License

Licensed under **MIT OR Apache-2.0**.

Copyright © FAHIM AHMED <fahimaloy@tutamail.com>
