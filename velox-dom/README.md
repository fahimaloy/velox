# velox-dom

Virtual DOM and layout engine for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-dom.svg)](https://crates.io/crates/velox-dom) [![Downloads](https://img.shields.io/crates/d/velox-dom.svg)](https://crates.io/crates/velox-dom) [![License](https://img.shields.io/crates/l/velox-dom.svg)](https://crates.io/crates/velox-dom) [![Documentation](https://img.shields.io/docsrs/velox-dom)](https://docs.rs/velox-dom)

## Overview

`velox-dom` is the tree: a `VNode`/`Props` element model, a flexbox-plus-block layout engine, the computed-style vocabulary the whole framework shares, keyed diffing, and greedy text wrapping. It draws nothing and parses nothing — paint belongs to `velox-renderer`, CSS parsing to `velox-style`. The crate has zero dependencies, which is why it can be used headless in tests, in a build script, or as the geometry authority for a pixel-proof suite.

## Installation

```bash
cargo add velox-dom
```

No dependencies and no feature flags. The `[features]` section in `velox-dom/Cargo.toml` carries only comments — it documents how the opt-in headless pixel-proof tests in `tests/` are meant to be gated, but no feature is declared, so nothing is compiled out or in by feature selection.

## API reference

Signatures below are copied from the source; see [docs.rs/velox-dom](https://docs.rs/velox-dom) for the full list.

### Tree model

| Signature | Description |
|---|---|
| `pub enum VNode { Element { tag: String, props: Props, children: Vec<VNode> }, Text(String) }` | The virtual node. Pure data — no state cell, no instance handle. |
| `pub struct Props { pub attrs: HashMap<String, String> }` | Attribute map carried by an element. |
| `Props::new() -> Self` | Empty props. |
| `Props::set(mut self, k: impl Into<String>, v: impl Into<String>) -> Self` | Builder: insert one attribute. |
| `Props::from_inline(style: impl Into<String>) -> Self` | Props holding a single `style` attribute. |
| `Props::from_class(class: impl Into<String>) -> Self` | Props holding a single `class` attribute. |
| `impl From<()> for Props` / `impl From<Vec<(&str, &str)>> for Props` | Lets `h` take `()`, pairs, or `Props` directly. |
| `pub fn h(tag: impl Into<String>, props: impl Into<Props>, children: Vec<VNode>) -> VNode` | Element constructor. |
| `pub fn text(t: impl Into<String>) -> VNode` | Text node constructor. |
| `VNode::key(&self) -> Option<String>` / `VNode::key_str(&self) -> Option<&str>` | Read the `key` attribute, if any. |

### Errors

| Signature | Description |
|---|---|
| `pub enum VeloxError` | Ten variants spanning the framework: `SfcParse`, `CssParse`, `Codegen`, `Layout`, `Render`, `Window`, `Build`, `Io`, `FeatureNotEnabled`, `Internal`. |
| `pub type Result<T> = std::result::Result<T, VeloxError>` | The framework-wide result alias. |

### Layout — `velox_dom::layout`

| Signature | Description |
|---|---|
| `pub fn compute_layout(node: &VNode, viewport_w: i32, viewport_h: i32) -> LayoutNode` | The entry point: lay a tree out against a viewport. |
| `pub struct LayoutNode { rect: Rect, z_index: i32, display_none: bool, source_index: Option<usize>, scroll_x/scroll_y: i32, clip: Option<Rect>, stacking_context: bool, scroll_height: i32, max_scroll_y: i32, scrollable: bool, children: Vec<LayoutNode> }` | One laid-out box plus its children. |
| `pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }` | An integer box. |
| `pub struct ScrollState { pub offset_y: f32, pub max_y: f32 }` | Vertical scroll offset and its clamp (`new`, `scroll_by`, `on_wheel`). |
| `pub struct FontMetrics { char_width, line_height, ascent, descent, x_height }` | Strut metrics for the default face (`from_font_size`, `heuristic_vertical`). |
| `pub fn is_scrollable(overflow: &str, content_h: f32, rect_h: f32) -> bool` | `overflow` is `auto`/`scroll` and content exceeds the box. |
| `pub fn collapse_margins(a: f32, b: f32) -> f32` | CSS adjacent-sibling margin collapsing. |
| `pub fn explicit_display(node: &VNode) -> Option<String>` / `is_out_of_flow(&VNode) -> bool` / `is_inline_level_box(&VNode) -> bool` / `is_atomic_inline_box(&VNode) -> bool` | Display classification helpers. |
| `pub fn set_intrinsic_size_probe(f: IntrinsicSizeProbe)` with `pub type IntrinsicSizeProbe = fn(src: &str) -> Option<(i32, i32)>` | Register an intrinsic-size probe for replaced elements such as `<img>`. |
| `pub const DEFAULT_ROOT_FONT_SIZE: f32 = 16.0`, `DEFAULT_TEXT_FAMILY: &str = "system-ui"`, `DEFAULT_WHITE_SPACE`, `DEFAULT_POSITION`, `DEFAULT_BOX_SIZING`, `DEFAULT_TEXT_ALIGN`, `DEFAULT_FLEX_DIRECTION`, `DEFAULT_JUSTIFY_CONTENT`, `DEFAULT_ALIGN_ITEMS` … | The CSS defaults the engine assumes when a property is undeclared. |
| `pub const INLINE_BY_DEFAULT_TAGS: &[&str]` | Tags that are inline unless styled otherwise. |

### Computed style — `velox_dom::style`, re-exported at the crate root

| Signature | Description |
|---|---|
| `pub struct ComputedStyle` | Fully resolved per-element style: `display`, `position`, box model, flex, colour, font, text, overflow, `transform`, `transitions` and more. |
| `ComputedStyle::new() -> Self` | Default-filled. |
| `ComputedStyle::set_property(&mut self, prop: &str, value: &str)` | Parse and store one declaration. |
| `ComputedStyle::apply_inline_style(&mut self, style: &str)` | Apply a whole `style` attribute (highest precedence). |
| `ComputedStyle::creates_stacking_context(&self) -> bool` / `is_display_none(&self) -> bool` / `is_hidden(&self) -> bool` | Predicates the renderer and a11y tree ask. |
| `pub enum Length { Px(f32), Percent(f32), Rem(f32), Em(f32), Vw(f32), Vh(f32), Dvh(f32), Dvw(f32), Auto, Zero }` | A CSS length. `Length::parse(s: &str) -> Option<Self>`, `to_px(&self, parent_size, root_size, viewport) -> f32`, `is_auto(&self) -> bool`. |
| `pub struct Color { r, g, b, a }` | `Color::parse(s) -> Option<Self>`, `to_rgba()`, `to_normalized()`, plus named constants (`BLACK`, `ACCENT_BLUE`, …). |
| `pub struct Sides<T>` | Four-sided value (`new`, `all`). Backs margin/padding/border-radius. |
| `pub struct Border`, `pub struct Transform`, `pub struct BoxShadow`, `pub struct Transition` | Parsed visual values, each with a `parse(&str) -> Option<Self>`. |
| `pub enum Display`, `Position`, `FlexDirection`, `FlexWrap`, `JustifyContent`, `AlignItems`, `AlignSelf`, `Overflow`, `BoxSizing`, `Visibility`, `WhiteSpace`, `TextOverflow`, `TextAlign`, `VerticalAlign`, `FontWeight`, `FontStyle`, `TextDecoration`, `BorderStyle`, `TimingFunction`, `TransformOp` | The property vocabularies; each offers `parse(s: &str) -> Option<Self>` (and `FontWeight::to_number()`, `AlignSelf::resolve(parent)` where relevant). |
| `pub fn is_viewport_filling(style: &ComputedStyle, is_root_index: bool) -> bool` / `root_is_viewport_filling(index: usize) -> bool` | Whether a box fills the logical viewport — the root-fill rule. |
| `pub fn resolve_auto_margins(...)` | `margin: auto` resolution for centring. |

### Diffing — `velox_dom::diff`

| Signature | Description |
|---|---|
| `pub fn diff(old: &VNode, new: &VNode) -> Vec<Patch>` | Keyed diff of two trees. |
| `pub enum Patch { Replace(VNode), SetAttr(String, String), RemoveAttr(String), UpdateChild(usize, Vec<Patch>), InsertChild(usize, VNode), RemoveChild(usize), MoveChild(usize, usize) }` | Patch indices are read against the DOM as it exists after earlier patches in the same list. |

### Text — `velox_dom::text_wrap`

| Signature | Description |
|---|---|
| `pub fn wrap_text(text: &str, max_width: i32, font_size_px: f32) -> Vec<TextLine>` | Greedy wrap with `WhiteSpace`/ellipsis handling. |
| `pub fn wrap_text_with_style(text: &str, max_width: f32, font_size_px: f32, font_family: &str, scale: f32, white_space: WhiteSpace, text_overflow: TextOverflow) -> Vec<(String, f32)>` | The full-string wrap the rest of the family builds on. |
| `pub fn wrap_text_with_options(...) -> Vec<TextLine>` | `wrap_text_with_style` with explicit family, scale, white-space and overflow. |
| `pub fn wrap_text_measured(text: &str, max_width: f32, font_size_px: f32, font_family: &str, scale: f32) -> Vec<(String, f32)>` | Wrapping plus measured width per line. |
| `pub struct TextLine { pub text: String, pub width: i32, pub height: i32 }` | One wrapped line. |
| `pub struct MeasuredText { pub width: f32, pub ascent: f32, pub descent: f32 }` | One measured run (`line_extent()` = ascent + descent). |
| `pub type TextMeasurer = fn(&str, f32, &str, f32) -> MeasuredText` | Measurer signature a backend registers via `set_skia_measurer`. |
| `pub fn measure_text(text: &str, font_size_px: f32, font_family: &str, scale: f32) -> f32` | Width of one run. |
| `pub fn measure_text_metrics(text: &str, font_size_px: f32, font_family: &str, scale: f32) -> MeasuredText` | Width plus vertical extent. |
| `pub fn set_current_scale(scale: f32)` / `current_scale() -> f32` | The DPI scale layout measures at. |
| `pub fn create_text_nodes(text: &str, x: i32, y: i32, max_width: i32, font_size_px: f32, source_index: Option<usize>) -> Vec<LayoutNode>` | One `LayoutNode` per wrapped line. |

## Example

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

## How it relates

```mermaid
graph TD
    dom["velox-dom<br/>(leaf)"]
    style["velox-style"]
    renderer["velox-renderer"]
    sfc["velox-sfc"]
    ex["examples/*"]

    style -->|depends on| dom
    renderer -->|depends on| dom
    sfc -.->|dev-dependency only| dom
    ex --> dom
```

`velox-dom` depends on nothing inside the framework. `velox-style` parses CSS *into* the style vocabulary defined here; `velox-renderer` lays out and paints trees built from `h`/`text`.

## License

MIT
