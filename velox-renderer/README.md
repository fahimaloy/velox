# velox-renderer

Rendering backends (Skia) for the Velox UI framework.

[![Crates.io](https://img.shields.io/crates/v/velox-renderer.svg)](https://crates.io/crates/velox-renderer) [![Downloads](https://img.shields.io/crates/d/velox-renderer.svg)](https://crates.io/crates/velox-renderer) [![License](https://img.shields.io/crates/l/velox-renderer.svg)](https://crates.io/crates/velox-renderer) [![Documentation](https://img.shields.io/docsrs/velox-renderer)](https://docs.rs/velox-renderer)

## Overview

`velox-renderer` owns everything between a styled `VNode` and pixels: backend selection, window and event loops, offscreen Skia rasterisation, hit testing and input editing, accessibility trees, viewport/DPI maths, and the HMR client. It depends on `velox-core`, `velox-dom` and `velox-style`, and every heavyweight dependency is behind a feature flag — with no features the crate compiles to a fast stub backend and pulls in no windowing or GPU stack at all. The feature set is the crate's real user-facing API.

## Installation

```bash
cargo add velox-renderer                # stub backend, no features
cargo add velox-renderer --features skia-native   # native Skia window
```

| Feature | What it turns on |
|---|---|
| `default` | Empty — the stub backend, no optional dependencies, fast to compile. |
| `skia` | Selects the Skia backend path: `BACKEND = "skia"`, `SelectedRenderer = skia_backend::SkiaRenderer`, and a compile-only `skia_backend` module with no native dependencies (so the API surface builds without Skia). |
| `svg` | `dep:resvg 0.48` — rasterisation of SVG images (`<img src="*.svg">`). |
| `skia-native` | The real backend: `dep:skia-safe 0.91.1`, `dep:winit 0.28`, `dep:raw-window-handle 0.5`, `dep:egl 0.2`, `dep:glow 0.12`, `dep:softbuffer 0.3`, plus `skia` and `svg`. Unlocks the window loops, GPU surface, offscreen PNG/RGBA rendering, and `skia_backend::SkiaRenderer` with a live `surface`. |

The always-on dependencies are `velox-core`, `velox-dom`, `velox-style`, `log 0.4`, `bytemuck 1`, `serde 1`, `serde_json 1.0`.

## API reference

Signatures below are copied from the source; see [docs.rs/velox-renderer](https://docs.rs/velox-renderer) for the full list. Rows marked **skia-native** only exist when that feature is enabled.

### Backend selection

| Signature | Description |
|---|---|
| `pub const BACKEND: &str` | `"skia"` when the `skia` feature is on, otherwise `"stub"`. |
| `pub fn backend_name() -> &'static str` | The active value of `BACKEND`. |
| `pub fn init() -> Result<(), String>` | Backend bootstrap; with no backend feature this is an empty `Ok(())`. The Skia equivalent is `skia_backend::init()`. |
| `pub type SelectedRenderer = skia_backend::SkiaRenderer` / `pub type SelectedRenderer = StubRenderer` | The feature-selected renderer type — `SkiaRenderer` under `skia` or `skia-native`, `StubRenderer` otherwise. |
| `pub fn new_selected_renderer() -> SelectedRenderer` | Construct it (`SkiaRenderer { surface: None, vnode: None }` under `skia-native`). |
| `pub struct StubRenderer` | No-feature backend; implements all three traits below and reports `"stub"`. |
| `pub fn skia_backend::init() -> Result<(), String>` | **skia-native** create the GL/DirectContext; under `skia` alone it is an `Ok(())` stand-in. |
| `pub struct SkiaRenderer { pub surface: Option<SkiaSurface>, pub vnode: Option<VNode> }` | **skia-native** `velox_renderer::skia_backend::SkiaRenderer`, with `with_window(window: &impl HasRawWindowHandle, width: i32, height: i32) -> Result<Self, String>`, `present(&mut self) -> Result<(), String>`, `resize(&mut self, width: i32, height: i32) -> Result<(), String>`. |

### Traits

| Signature | Description |
|---|---|
| `pub trait Renderer { fn backend_name(&self) -> &'static str; fn mount(&self, vnode: &VNode) -> Result<RenderTree, String>; }` | Minimal backend contract. |
| `pub trait VeloxRenderer { fn new() -> Result<Self, String> where Self: Sized; fn mount(&mut self, vnode: VNode) -> Result<(), String>; fn hot_update(&mut self, new_vnode: VNode) -> Result<(), String>; }` | Owned renderer with hot reload. |
| `pub trait HmrRenderer { fn init() -> Result<(), String> where Self: Sized; fn mount(&mut self, vnode: VNode) -> Result<(), String>; fn hot_update(&mut self, new_vnode: VNode) -> Result<(), String>; fn get_window_handle(&self) -> *mut std::ffi::c_void; }` | The HMR-capable contract the window loops drive. |

### Trees and accessibility

| Signature | Description |
|---|---|
| `pub struct RenderTree { pub root: VNode, pub node_count: usize, pub text_count: usize }` | What `Renderer::mount` returns — the laid-out tree plus counts. |
| `pub fn build_a11y_tree(vnode: &VNode, width: i32, height: i32) -> A11yTree` | Walks the tree (running its own `compute_layout`) into an accessibility tree. |
| `pub struct A11yTree { pub root: A11yNode }` | Root of the accessibility tree. |
| `pub struct A11yNode { pub id: usize, pub role: String, pub name: String, pub rect: velox_dom::layout::Rect, pub children: Vec<A11yNode> }` | One accessible element with its layout box. |
| `pub fn find_node_at_path<'a>(node: &'a VNode, path: &[usize]) -> Option<&'a VNode>` | Resolve an index path into a live node. |

### Style glue and lifecycle

| Signature | Description |
|---|---|
| `pub fn style_vnode_with_hover<F>(vnode: &VNode, author: &Stylesheet, is_hovered: &F) -> VNode where F: Fn(&str, &velox_dom::Props) -> bool` | Run the full cascade (`velox_style::apply_with_cascade_with_hover`) for one frame — the render path's styling entry point. |
| `pub struct LifecycleCleanupGuard` | On `Drop`, runs every destroy hook (`velox_core::lifecycle::run_all_destroy_hooks`). |
| `pub fn ensure_mounted(flag: &mut bool)` | Fire the mounted pass exactly once per flag. |

### Window loops — skia-native

| Signature | Description |
|---|---|
| `pub fn run_window_vnode_skia<F, G, H>(title: &str, make_view: F, on_event: G, get_title: H) -> Result<(), String> where F: FnMut(u32, u32) -> (velox_dom::VNode, Stylesheet) + 'static, G: FnMut(&str, Option<&str>) + 'static, H: FnMut() -> String + 'static` | Open a window and drive the render/event loop. Falls back to headless with `VELOX_HEADLESS=1` or no compositor. |
| `pub fn run_window_vnode_skia_with_hmr<F, G, H>(title: &str, make_view: F, on_event: G, get_title: H, hmr_rx: Arc<Mutex<Receiver<HmrMessage>>>) -> Result<(), String>` | Same loop, with an HMR receiver threaded in (same `where` clauses). |
| `pub fn logical_size(width: i32, height: i32, scale_factor: f32) -> (u32, u32)` | Logical window size for a physical size and scale. |
| `pub fn skia_draw_test_frame() -> Result<(), String>` | Render one frame offscreen (unix). |
| `pub fn create_direct_context() -> Result<crate::skia_gl::GlDirectContext, String>` | Create the GPU direct context (unix). |

### Offscreen rendering — skia-native

| Signature | Description |
|---|---|
| `pub fn render_vnode_to_rgba(vnode: &VNode, sheet: &Stylesheet, width: i32, height: i32) -> Result<Vec<u8>, String>` | Rasterise a styled tree to raw RGBA bytes. |
| `pub fn render_vnode_to_raster_png(vnode: &VNode, sheet: &Stylesheet, width: i32, height: i32) -> Result<Vec<u8>, String>` | Rasterise to PNG bytes. |
| `pub fn render_vnode_to_raster_png_with_scale(vnode: &VNode, sheet: &Stylesheet, width: i32, height: i32, scale_factor: f32) -> Result<Vec<u8>, String>` | PNG output at an explicit DPI scale (a non-positive scale falls back to `1.0`). |
| `pub fn image_decode_count() -> u64` / `pub fn reset_image_decode_count()` | Decode counters used by parity tests. |

### HMR

| Signature | Description |
|---|---|
| `pub const DEFAULT_HMR_PORT: u16 = 31313;` | Dev-server port when `VELOX_HMR_PORT` is unset. |
| `pub enum HmrMessage { FullReload, HotReload { module_path: String }, KeepWindow }` | Messages on the HMR channel (newline-delimited JSON). |
| `pub fn hmr_config() -> Option<u16>` | `Some(port)` when `VELOX_HMR=1` (`VELOX_HMR_PORT`, else `DEFAULT_HMR_PORT`); `None` otherwise. |
| `pub fn run_hmr_client(port: u16, tx: Sender<HmrMessage>)` | Spawn the reconnecting client thread; on `FullReload` the process exits with code 0 so the dev server restarts it. |

### Viewport — `velox_renderer::viewport`

| Signature | Description |
|---|---|
| `pub struct Viewport` | `new(physical_width: u32, physical_height: u32, scale: f32)`, `from_i32(i32, i32, f32)`, `from_logical(u32, u32, f32)`, `set_physical`, `set_physical_i32`, `set_scale`, `logical_size() -> (u32, u32)`, `physical_width_i32()`, `physical_height_i32()`. |
| `pub fn physical_from_logical(logical_w: u32, logical_h: u32, scale: f32) -> (u32, u32)` | The single DPI/rounding authority for logical → physical. |
| `pub struct PhysicalSize`, `pub struct LogicalSize` | `u32`-sized pixel and logical dimensions. |

### Events — `velox_renderer::events`

| Signature | Description |
|---|---|
| `pub struct EventRegistry` | Handler registry the dispatcher writes into. |
| `pub struct Runtime` (re-exported as `velox_renderer::EventRuntime`) | The event runtime holding registry and focus state. |
| `pub fn dispatch(event: &str, tree: &RenderTree, registry: &mut EventRegistry) -> usize` | Dispatch one event by name against a render tree; returns how many handlers ran. |
| `pub enum EditAction { Insert(char), Backspace, Delete, MoveLeft { shift: bool }, MoveRight { shift: bool }, Home { shift: bool }, End { shift: bool }, Submit, Blur }` | A semantic text edit. |
| `pub fn apply_edit(target: &mut InputTarget, value: &str, action: EditAction) -> EditResult` | Apply an edit to an input target and report what changed. |
| `pub fn collect_click_targets(...)` / `collect_hover_targets(...)` / `collect_input_targets(...)` | Enumerate dispatchable targets from a render tree. |
| `pub fn hit_test_click(...)` / `hit_test_hover(...)` / `hit_test_input(...)` / `hit_test_scrollable(...)` | Point-in-box hit tests returning the target's path. |
| `pub fn plan_keydown(vnode: &VNode, key_name: &str) -> KeydownPlan` / `pub fn apply_keydown(...)` | Keyboard routing for a rendered tree. |
| `pub fn apply_wheel_scroll(...)` / `apply_scroll_offsets(...)` | Wheel and offset application for scrollable boxes. |

### Text measurement and input metrics

| Signature | Description |
|---|---|
| `pub struct TextRenderConfig` | `new(font_family: &str, font_size: f32)` plus `with_color`, `with_weight`, `with_alignment`, `with_decoration`, `with_max_width`. |
| `pub struct TextMeasurer` | `measure(text: &str, config: &TextRenderConfig) -> (f32, f32)` and `measure_with_scale(text, config, scale: f32) -> (f32, f32)` — width, height. |
| `pub const DEFAULT_INPUT_TEXT_PADDING: f32 = 4.0;` / `pub struct InputTextMetrics` | Input box metrics (`velox_renderer::input_metrics`). |

## Example

With no features enabled the crate is a stub you can still build against:

```rust
use velox_dom::{h, text};
use velox_renderer::{backend_name, build_a11y_tree};

let vnode = h("button", (), vec![text("Save")]);
assert_eq!(backend_name(), "stub");

let tree = build_a11y_tree(&vnode, 800, 600);
println!("{}x{} root", tree.root.rect.w, tree.root.rect.h);
```

The window loop, as used by `examples/counter` (requires `--features skia-native`):

```rust
use std::sync::{Arc, Mutex};

use velox_dom::{Props, h, text};
use velox_style::Stylesheet;

fn main() -> Result<(), String> {
    // Called on every resize: a styled tree for the new dimensions.
    let make_view = |_w: u32, _h: u32| -> (velox_dom::VNode, Stylesheet) {
        let vnode = h("div", Props::new().set("style", "color: #e6edf3"), vec![text("Hello")]);
        (vnode, Stylesheet::default())
    };

    let on_event = |event: &str, _payload: Option<&str>| {
        if event == "click" {
            // route into your velox_core state here
        }
    };
    let get_title = || "Hello Velox".to_string();

    if let Some(port) = velox_renderer::hmr_config() {
        let (tx, rx) = std::sync::mpsc::channel::<velox_renderer::HmrMessage>();
        velox_renderer::run_hmr_client(port, tx);
        velox_renderer::run_window_vnode_skia_with_hmr(
            "Hello Velox",
            make_view,
            on_event,
            get_title,
            Arc::new(Mutex::new(rx)),
        )
    } else {
        velox_renderer::run_window_vnode_skia("Hello Velox", make_view, on_event, get_title)
    }
}
```

## How it relates

```mermaid
graph TD
    core["velox-core<br/>(leaf)"]
    dom["velox-dom<br/>(leaf)"]
    style["velox-style"]
    renderer["velox-renderer"]
    sfc["velox-sfc"]
    cli["veloxc"]
    ex["examples/*"]

    renderer -->|depends on| core
    renderer -->|depends on| dom
    renderer -->|depends on| style
    style -->|depends on| dom
    renderer -.->|dev-dependency| sfc
    cli --> renderer
    ex --> renderer
```

`velox-renderer` sits at the top of the library stack: it lays out `velox-dom` trees, asks `velox-style` for the cascade, and reads reactive state from `velox-core`. `veloxc` links it for compiled apps, and it keeps `velox-sfc` as a dev-dependency for its integration tests.

## License

MIT
