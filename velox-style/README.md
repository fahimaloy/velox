# velox-style

CSS parsing and styling for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-style.svg)](https://crates.io/crates/velox-style) [![Downloads](https://img.shields.io/crates/d/velox-style.svg)](https://crates.io/crates/velox-style) [![License](https://img.shields.io/crates/l/velox-style.svg)](https://crates.io/crates/velox-style) [![Documentation](https://img.shields.io/docsrs/velox-style)](https://docs.rs/velox-style)

## Overview

`velox-style` is the CSS layer of the Velox framework: a cssparser-driven stylesheet parser, selector matching, a UA default sheet, and the cascade that resolves author styles, user-agent defaults and inline declarations into a `ComputedStyle`. It sits directly on top of `velox-dom` — selectors match `VNode` trees and the results are written back into each element's `style` attribute, so layout in `velox-dom` and painting in `velox-renderer` both read plain resolved CSS. The crate also re-exports the whole computed-style vocabulary (`Length`, `Color`, `Display`, flex properties, and friends) so downstream crates can `use velox_style::Length` in one import.

## Installation

```bash
cargo add velox-style
```

No feature flags. Dependencies: `cssparser 0.29`, `selectors 0.23`, and `velox-dom`.

## API reference

Signatures below are copied from the source; see [docs.rs/velox-style](https://docs.rs/velox-style) for the full list.

### Stylesheets and selectors

| Signature | Description |
|---|---|
| `pub struct Stylesheet { pub rules: Vec<Rule> }` | A parsed stylesheet; `Default` gives an empty sheet. |
| `Stylesheet::parse(css: &str) -> Self` | Parse with `cssparser`'s rule-list parser. Nested `@media` / `@supports` are recursed into; `@keyframes` and `@font-face` are skipped. |
| `pub struct Rule { pub selector: CompoundSelector, pub decls: HashMap<String, String> }` | One selector plus its raw declarations. |
| `pub struct CompoundSelector { pub parts: Vec<SelectorPart> }` | A compound selector as an ordered list of parts. |
| `pub struct SelectorPart { pub tag: String, pub class: String, pub hover: bool, pub placeholder: bool, pub attr_name: String, pub attr_value: Option<String>, pub combinator: Combinator }` | One matchable step: tag, class, `:hover`, `::placeholder`, `[attr]` / `[attr=value]`, and the combinator that joins it to the previous part. |
| `pub enum Combinator { #[default] None, Descendant, Child }` | The default, descendant (` `) and child (`>`) combinators. |
| `pub const PLACEHOLDER_STYLE_ATTR: &str = "style:placeholder";` | Attribute the cascade writes `::placeholder` declarations into; the renderer reads it only when the field is empty. |

Supported selector syntax: element (`div`), class (`.btn`), compound (`button.primary`), attribute (`[disabled]`, `[type=submit]`), pseudo-classes (`:hover`, `::placeholder`), universal (`*`), descendant and child combinators. Inline `style` attributes always win over author rules.

### Applying styles

| Signature | Description |
|---|---|
| `pub fn apply_styles(node: &VNode, sheet: &Stylesheet) -> VNode` | Match the sheet against one tree and return a styled copy with resolved `style` attributes. |
| `pub fn apply_with_cascade(node: &VNode, author: &Stylesheet) -> VNode` | The full cascade: UA defaults < author sheet < inline style. |
| `pub fn apply_with_cascade_with_hover<F>(node: &VNode, author: &Stylesheet, is_hovered: &F) -> VNode where F: Fn(&str, &Props) -> bool` | The cascade with `:hover` resolved through a caller-supplied predicate (element identifier, `Props`). |
| `pub fn apply_styles_with_hover<F>(node: &VNode, sheet: &Stylesheet, is_hovered: &F) -> VNode where F: Fn(&str, &Props) -> bool` | `apply_styles` with the same hover hook, UA defaults not consulted. |
| `pub fn compute_styles_for_node(node: &VNode, inline_style: Option<&str>, sheet: Option<&Stylesheet>, is_hovered: bool, ancestors: &[&VNode]) -> ComputedStyle` | Resolve one node against its ancestor chain into a typed `ComputedStyle` — the function the layout pass builds on. |
| `pub fn ua_sheet() -> &'static Stylesheet` | The built-in user-agent sheet (`velox_style::ua::ua_sheet`), parsed once from the embedded `ua.css` and cached in a `OnceLock`. |

### Re-exported computed-style vocabulary

`pub use velox_dom::style::*;` — the property types live in `velox-dom` and are re-exported here so a stylesheet consumer needs one crate in scope.

| Item | Description |
|---|---|
| `pub struct ComputedStyle` | The resolved property bag: `new()`, `set_property(&mut self, prop: &str, value: &str)`, `apply_inline_style(&mut self, style: &str)`, `creates_stacking_context(&self) -> bool`, `is_display_none(&self)`, `is_hidden(&self)`, plus public fields for display, box model, flex, colour, font, text, overflow, `transform` and `transitions`. |
| `pub enum Length { Px(f32), Percent(f32), Rem(f32), Em(f32), Vw(f32), Vh(f32), Dvh(f32), Dvw(f32), Auto, Zero }` | A CSS length with `parse(s: &str) -> Option<Self>` and `to_px(&self, parent_size: f32, root_size: f32, viewport: f32) -> f32`. |
| `pub struct Color` | `Color::parse`, `to_rgba()`, `to_normalized()`, and named constants. |
| `pub struct Sides<T>` | Four-sided margin/padding/border-radius values (`new`, `all`). |
| `pub struct Border`, `pub struct Transform`, `pub struct BoxShadow`, `pub struct Transition` | Parsed visual values, each with `parse(&str) -> Option<Self>`. |
| `pub enum Display`, `Position`, `FlexDirection`, `FlexWrap`, `JustifyContent`, `AlignItems`, `AlignSelf`, `Overflow`, `BoxSizing`, `Visibility`, `WhiteSpace`, `TextOverflow`, `TextAlign`, `VerticalAlign`, `FontWeight`, `FontStyle`, `TextDecoration`, `BorderStyle`, `TimingFunction`, `TransformOp` | Property vocabularies; each exposes `parse(s: &str) -> Option<Self>`. |
| `pub fn is_viewport_filling(style: &ComputedStyle, is_root_index: bool) -> bool` | Whether a box fills the logical viewport — the root-fill rule. |

### Fonts — `velox_style::fonts`, re-exported at the root

| Signature | Description |
|---|---|
| `pub struct FontDescriptor` | `new(family: impl Into<String>, size: f32)`, `with_weight(FontWeight)`, `with_style(FontStyle)` — a fully specified face request. |
| `pub struct FontFamily` | `new(family_str: &str)` splits a CSS `font-family` list; `families() -> &[String]`, `primary() -> &str`, `fallbacks() -> &[String]`. |
| `pub enum FontStyle` | `parse(s: &str) -> Option<Self>` for `normal` / `italic` / `oblique`. |
| `pub enum LineHeight` | `parse(s: &str) -> Option<Self>` and `to_pixels(&self, font_size: f32) -> f32`. |
| `pub enum GenericFamily` | `from_str(s: &str) -> Option<Self>` and `system_fonts(&self) -> &'static [&'static str]` for the CSS generic families. |

### Visual effects — `velox_style::visual_effects`, re-exported at the root

| Signature | Description |
|---|---|
| `pub struct BorderRadius` | `parse(value: &str) -> Option<Self>`, `all(radius: Length) -> Self`. Re-exported at the crate root as `velox_style::BorderRadius`. |
| `pub struct BoxShadow` | `new(offset_x: f32, offset_y: f32, blur: f32, color: (u8, u8, u8, u8))`, `with_spread(spread: f32)`, `inset()`, `parse(value: &str) -> Option<Self>`. Re-exported at the crate root as `velox_style::VisualBoxShadow` — the root name `BoxShadow` is taken by the `velox_dom::style` glob re-export. |
| `pub struct TextShadow` | `new(offset_x: f32, offset_y: f32, blur: f32, color: (u8, u8, u8, u8))`, `parse(value: &str) -> Option<Self>`. |

## Example

```rust
use velox_dom::{Props, VNode, h, text};
use velox_style::{Stylesheet, apply_styles, apply_with_cascade};

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

// Or the full cascade (UA defaults < author < inline).
let cascaded = apply_with_cascade(&node, &sheet);
let _ = cascaded;
```

## How it relates

```mermaid
graph TD
    dom["velox-dom<br/>(leaf)"]
    style["velox-style"]
    renderer["velox-renderer"]
    sfc["velox-sfc"]
    cli["veloxc"]
    ex["examples/*"]

    style -->|depends on| dom
    renderer -->|depends on| style
    cli -->|depends on| style
    ex --> style
    cli -.->|depends on| sfc
```

`velox-style` reads the `VNode` types that `velox-dom` defines and feeds `ComputedStyle` values back to them; `velox-renderer` consumes the resolved sheets when it paints, and `veloxc` drives the same cascade from generated code.

## License

MIT
