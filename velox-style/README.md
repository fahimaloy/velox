# velox-style

CSS parsing, selectors and cascade for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-style.svg)](https://crates.io/crates/velox-style) [![Downloads](https://img.shields.io/crates/d/velox-style.svg)](https://crates.io/crates/velox-style) [![docs.rs](https://img.shields.io/docsrs/velox-style)](https://docs.rs/velox-style) [![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/velox-style.svg)](https://crates.io/crates/velox-style) [![CI](https://img.shields.io/github/actions/workflow/status/fahimaloy/velox/ci.yml?branch=main)](https://github.com/fahimaloy/velox/actions/workflows/ci.yml)

## Overview

`velox-style` is the CSS layer of the Velox framework: a cssparser-driven stylesheet parser, selector matching, a UA default sheet, and the cascade that resolves author styles, user-agent defaults and inline declarations into a `ComputedStyle`. It sits directly on top of `velox-dom` — selectors match `VNode` trees and the results are written back into each element's `style` attribute, so layout in `velox-dom` and painting in `velox-renderer` both read plain resolved CSS. The crate also re-exports the whole computed-style vocabulary (`Length`, `Color`, `Display`, flex properties, and friends) so downstream crates can `use velox_style::Length` in one import.

## Features

- **Stylesheet parsing** — `cssparser`'s rule-list parser; nested `@media`/`@supports` are recursed into, `@keyframes` and `@font-face` are skipped
- **Selector matching** — element, class, compound, attribute, `:hover`, `::placeholder`, universal, descendant and child combinators
- **Full cascade** — UA defaults < author sheet < inline style, via `apply_with_cascade`; hover-aware variants resolve `:hover` through a caller-supplied predicate
- **UA default sheet** — built-in `ua.css`, parsed once and cached in a `OnceLock`
- **Typed resolution** — `compute_styles_for_node` resolves one node against its ancestor chain into a typed `ComputedStyle`
- **Re-exported vocabulary** — the whole `velox-dom` style vocabulary (`Length`, `Color`, flex enums, …) in one import
- **Font handling** — `FontDescriptor`, `FontFamily` list splitting, `LineHeight`, and CSS generic families

## Installation

```bash
cargo add velox-style
```

No feature flags. Dependencies: `cssparser 0.29`, `selectors 0.23`, and `velox-dom`.

## Quick start

```rust
use velox_dom::{Props, VNode, h, text};
use velox_style::{Stylesheet, apply_styles};

let sheet = Stylesheet::parse(".btn { font-weight: bold; }");

let node = h(
    "div",
    Props::new().set("class", "btn container"),
    vec![text("Click")],
);

// Single-layer matching: declarations land in the element's `style` attribute.
let styled = apply_styles(&node, &sheet);
if let VNode::Element { props, .. } = &styled {
    let style = props.attrs.get("style").expect("style attr applied");
    assert!(style.contains("font-weight: bold;"));
}
```

For the full cascade (UA defaults < author < inline), use `apply_with_cascade(&node, &sheet)`.

## Documentation

- API reference: [docs.rs/velox-style](https://docs.rs/velox-style)
- Book: [Styling](https://fahimaloy.github.io/velox/features/styling.html) — cascade, selectors and what paints today

## Workspace crates

Velox is a workspace of six published crates. This crate is marked **(this crate)**.

| Crate | What it provides | Documentation |
|:---|:---|:---|
| [velox-core](https://docs.rs/velox-core) | Reactive primitives: signals, effects, lifecycle hooks | [docs.rs](https://docs.rs/velox-core) |
| [velox-dom](https://docs.rs/velox-dom) | `VNode` types, block + flex layout, positioning, overflow | [docs.rs](https://docs.rs/velox-dom) |
| **[velox-style](https://docs.rs/velox-style)** · this crate | CSS parsing, selectors, cascade; styles applied to `VNode` | [docs.rs](https://docs.rs/velox-style) |
| [velox-renderer](https://docs.rs/velox-renderer) | Rendering backends, event dispatch, hit testing | [docs.rs](https://docs.rs/velox-renderer) |
| [velox-sfc](https://docs.rs/velox-sfc) | Single-file component compiler (pest grammar) → Rust | [docs.rs](https://docs.rs/velox-sfc) |
| [veloxc](https://docs.rs/veloxc) | The CLI and dev server | [docs.rs](https://docs.rs/veloxc) |

## Status

Published on [crates.io](https://crates.io/crates/velox-style) at **0.1.1**.

Working today: selectors, the cascade, the UA sheet, and `:hover`/`::placeholder` handling. Inline `style` attributes always win over author rules.

CSS properties that are painted today: `font-size`, `font-weight`, `line-height`, `text-decoration`, `border-radius`, `opacity`. Properties that **parse but paint nothing** (they survive the cascade and produce no visual change; `veloxc lint` reports each one): `box-shadow`, `font-style`, `letter-spacing`, `visibility`, `overflow-x`, `overflow-y`, `background-image`, `transition`, `border-style`, `border-color`, and `transform` (stacking only, no visual transform).

## License

Licensed under **MIT OR Apache-2.0**.

Copyright © FAHIM AHMED <fahimaloy@tutamail.com>
