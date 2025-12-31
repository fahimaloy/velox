//! Renderer crate with optional backends.
//! No features enabled => stub, compiles fast.

use velox_dom::VNode;
use velox_style::{Stylesheet, apply_styles_with_hover};
use std::collections::{HashMap, HashSet};

pub mod events;

/// In-memory representation of a mounted tree (stubbed for now).
pub struct RenderTree {
    pub root: VNode,
    pub node_count: usize,
    pub text_count: usize,
}

fn summarize(v: &VNode, counts: &mut (usize, usize)) {
    match v {
        VNode::Text(_) => {
            counts.0 += 1;
            counts.1 += 1;
        }
        VNode::Element { children, .. } => {
            counts.0 += 1;
            for c in children {
                summarize(c, counts);
            }
        }
    }
}

fn build_render_tree(v: &VNode) -> RenderTree {
    let mut counts = (0, 0);
    summarize(v, &mut counts);
    RenderTree { root: v.clone(), node_count: counts.0, text_count: counts.1 }
}

/// Reconcile two VNode children vectors using an optional `key` prop.
/// This is a simple helper that prefers reusing old nodes when the child's
/// `Props` contains a `key` attribute matching a new child's `key`.
pub fn reconcile_keyed_children(old: &mut Vec<VNode>, new: &Vec<VNode>) {
    let mut key_to_index: HashMap<String, usize> = HashMap::new();
    for (i, n) in old.iter().enumerate() {
        if let VNode::Element { props, .. } = n {
            if let Some(k) = props.attrs.get("key") {
                key_to_index.insert(k.clone(), i);
            }
        }
    }
    let mut used: HashSet<usize> = HashSet::new();
    let mut out: Vec<VNode> = Vec::with_capacity(new.len());
    for nn in new.iter() {
        if let VNode::Element { props: nprops, .. } = nn {
            if let Some(k) = nprops.attrs.get("key") {
                if let Some(&idx) = key_to_index.get(k) {
                    out.push(old[idx].clone());
                    used.insert(idx);
                    continue;
                }
            }
        }
        out.push(nn.clone());
    }
    *old = out;
}

/// Minimal renderer trait. Backends implement this to expose a consistent API.
pub trait Renderer {
    fn backend_name(&self) -> &'static str;
    fn mount(&self, vnode: &VNode) -> RenderTree;
}

#[cfg(feature = "wgpu")]
pub mod wgpu_backend {
    use wgpu as _wgpu;
    use winit as _winit;

    pub fn init() {
        // Attempt headless WGPU initialization to verify adapter/device availability.
        // This is intentionally best-effort and will not panic on failure; it logs to stderr.
        let instance = _wgpu::Instance::new(_wgpu::InstanceDescriptor { backends: _wgpu::Backends::all(), dx12_shader_compiler: Default::default() });
        let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: _wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })) {
            None => {
                eprintln!("wgpu backend: no adapter found (init skipped)");
                return;
            }
            Some(a) => a,
        };

        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("velox-wgpu-device"),
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        }, None)) {
            Ok((_device, _queue)) => {
                let info = adapter.get_info();
                eprintln!("wgpu backend: init OK — adapter='{}'", info.name);
            }
            Err(e) => {
                eprintln!("wgpu backend: failed to request device: {:?}", e);
            }
        }
    }

    pub struct WgpuRenderer;
    impl crate::Renderer for WgpuRenderer {
        fn backend_name(&self) -> &'static str {
            "wgpu"
        }
        fn mount(&self, vnode: &velox_dom::VNode) -> crate::RenderTree {
            crate::build_render_tree(vnode)
        }
    }
}

// Real Skia backend only when `skia-native` is enabled.
#[cfg(feature = "skia-native")]
pub mod skia_backend {
    use skia_safe as _sk;

    pub fn init() {
        // TODO: implement Skia GPU surface creation (GL/EGL)
    }

    pub struct SkiaRenderer;
    impl crate::Renderer for SkiaRenderer {
        fn backend_name(&self) -> &'static str {
            "skia"
        }
        fn mount(&self, vnode: &velox_dom::VNode) -> crate::RenderTree {
            crate::build_render_tree(vnode)
        }
    }
}

// Skia stub backend to allow compiling with `--features skia` without native deps.
#[cfg(all(feature = "skia", not(feature = "skia-native")))]
pub mod skia_backend {
    pub fn init() {}

    pub struct SkiaRenderer;
    impl crate::Renderer for SkiaRenderer {
        fn backend_name(&self) -> &'static str { "skia" }
        fn mount(&self, vnode: &velox_dom::VNode) -> crate::RenderTree {
            crate::build_render_tree(vnode)
        }
    }
}

/// Stub init used when no backend features are enabled.
#[cfg(not(any(feature = "wgpu", feature = "skia")))]
pub fn init() {
    // Intentionally empty
}

// Simple identifier of the selected backend, useful for tests.
#[cfg(all(feature = "wgpu", feature = "skia"))]
pub const BACKEND: &str = "wgpu+skia";
#[cfg(all(feature = "wgpu", not(feature = "skia")))]
pub const BACKEND: &str = "wgpu";
#[cfg(all(not(feature = "wgpu"), feature = "skia"))]
pub const BACKEND: &str = "skia";
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
pub const BACKEND: &str = "stub";

pub fn backend_name() -> &'static str {
    BACKEND
}

/// Feature-selected renderer type and constructor for tests and examples.
#[cfg(feature = "wgpu")]
pub type SelectedRenderer = wgpu_backend::WgpuRenderer;
#[cfg(all(not(feature = "wgpu"), any(feature = "skia", feature = "skia-native")))]
pub type SelectedRenderer = skia_backend::SkiaRenderer;
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
pub struct StubRenderer;
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
pub type SelectedRenderer = StubRenderer;
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
impl Renderer for StubRenderer {
    fn backend_name(&self) -> &'static str {
        "stub"
    }
    fn mount(&self, vnode: &VNode) -> RenderTree {
        build_render_tree(vnode)
    }
}

/// Construct the feature-selected renderer.
pub fn new_selected_renderer() -> SelectedRenderer {
    #[cfg(feature = "wgpu")]
    {
        wgpu_backend::WgpuRenderer
    }
    #[cfg(all(not(feature = "wgpu"), feature = "skia"))]
    {
        skia_backend::SkiaRenderer
    }
    #[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
    {
        StubRenderer
    }
}

pub use events::Runtime as EventRuntime;

#[cfg(feature = "wgpu")]
fn load_system_font() -> Option<ab_glyph::FontArc> {
    use std::fs;
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/gnu-free/FreeSans.ttf",
    ];
    for p in CANDIDATES {
        if let Ok(bytes) = fs::read(p) {
            if let Ok(font) = ab_glyph::FontArc::try_from_vec(bytes) {
                return Some(font);
            }
        }
    }
    None
}

#[cfg(feature = "wgpu")]
pub fn run_window_vnode<F, G, H>(title: &str, mut make_view: F, mut on_event: G, mut get_title: H)
where
    F: FnMut(u32, u32) -> (velox_dom::VNode, Stylesheet) + 'static,
    G: FnMut(&str, Option<&str>) + 'static,
    H: FnMut() -> String + 'static,
{
    use winit::dpi::PhysicalSize;
    use winit::event::{ElementState, Event, MouseButton, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    // Setup window
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(PhysicalSize::new(800, 600))
        .build(&event_loop)
        .expect("window");
    let mut size = window.inner_size();
    let _title_owned = title.to_string();

    // WGPU setup (reuse pipeline from run_window)
    let instance = wgpu::Instance::default();
    let surface = unsafe { instance.create_surface(&window) }.expect("surface");
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("velox-device"),
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        },
        None,
    ))
    .expect("device");

    if size.width == 0 || size.height == 0 {
        size = PhysicalSize::new(800, 600);
        window.set_inner_size(size);
    }
    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats[0];
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width,
        height: size.height,
        present_mode: caps.present_modes[0],
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        pos: [f32; 2],
        color: [f32; 3],
    }
    let shader_src = r#"
        struct VsOut { @builtin(position) position: vec4<f32>, @location(0) color: vec3<f32>, };
        @vertex fn vs(@location(0) pos: vec2<f32>, @location(1) color: vec3<f32>) -> VsOut {
            var out: VsOut; out.position = vec4<f32>(pos, 0.0, 1.0); out.color = color; return out;
        }
        @fragment fn fs(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> { return vec4<f32>(color, 1.0); }
    "#;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("velox-shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });
    let vlayout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 8, shader_location: 1 },
        ],
    };
    let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("velox-pl"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("velox-pipeline"),
        layout: Some(&pl_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: "vs", buffers: &[vlayout] },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs",
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });
    let mut vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("velox-vbuf"),
        size: 6 * std::mem::size_of::<Vertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Extract first child rect (button) from VNode layout
    fn to_ndc(w: u32, h: u32, x: f32, y: f32) -> [f32; 2] {
        [x / w as f32 * 2.0 - 1.0, 1.0 - y / h as f32 * 2.0]
    }
    // (helpers defined once above)
    fn has_class(props: &velox_dom::Props, class: &str) -> bool {
        props
            .attrs
            .get("class")
            .map(|s| s.split_whitespace().any(|c| c == class))
            .unwrap_or(false)
    }
    fn find_rect_pred(
        vnode: &velox_dom::VNode,
        layout: &velox_dom::layout::LayoutNode,
        pred: &dyn Fn(&velox_dom::VNode) -> bool,
    ) -> Option<velox_dom::layout::Rect> {
        if pred(vnode) {
            return Some(layout.rect);
        }
        match vnode {
            velox_dom::VNode::Element { children, .. } => {
                for (i, ch) in children.iter().enumerate() {
                    if let Some(lc) = layout.children.get(i) {
                        if let Some(r) = find_rect_pred(ch, lc, pred) {
                            return Some(r);
                        }
                    }
                }
                None
            }
            velox_dom::VNode::Text(_) => None,
        }
    }
    fn find_text_in_class(vnode: &velox_dom::VNode, class: &str) -> Option<String> {
        fn first_text(node: &velox_dom::VNode) -> Option<String> {
            match node {
                velox_dom::VNode::Text(t) => {
                    let s = t.trim();
                    if s.is_empty() { None } else { Some(s.to_string()) }
                }
                velox_dom::VNode::Element { children, .. } => {
                    for ch in children { if let Some(s) = first_text(ch) { return Some(s); } }
                    None
                }
            }
        }
        match vnode {
            velox_dom::VNode::Element { props, children, .. } => {
                if has_class(props, class) {
                    return first_text(vnode);
                }
                for ch in children { if let Some(s) = find_text_in_class(ch, class) { return Some(s); } }
                None
            }
            _ => None,
        }
    }
    let mut btn_rect: (f32, f32, f32, f32) = (0.0, 0.0, 0.0, 0.0);
    let mut hovered = false;
    let mut mouse = (0.0f32, 0.0f32);
    let mut bg_color: [f32; 4] = [0.12, 0.12, 0.14, 1.0];
    let mut text_color: [f32; 4] = [0.90, 0.93, 0.95, 1.0];
    let mut font_size: f32 = 18.0;
    let mut btn_color: [f32; 4] = [0.2, 0.5, 0.8, 1.0];
    let mut btn_text_color: [f32; 4] = text_color;
    let mut btn_text: String = String::new();
    let mut btn_handler: Option<String> = None;
    let mut btn_pad_left: f32 = 0.0;
    let mut btn_pad_top: f32 = 0.0;
    let mut click_targets: Vec<(f32,f32,f32,f32,String, Option<String>)> = Vec::new();

    // Keep previous vnode around so we can attempt keyed reconciliation between frames.
    let mut prev_vnode: Option<velox_dom::VNode> = None;

    let make_vertices = |w: u32, h: u32, r: (f32, f32, f32, f32), color: [f32; 4]| -> [Vertex; 6] {
        let (x0, y0, x1, y1) = r;
        let (r, g, b) = (color[0], color[1], color[2]);
        [
            Vertex { pos: to_ndc(w, h, x0, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x1, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x1, y1), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x0, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x1, y1), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x0, y1), color: [r, g, b] },
        ]
    };

    // Font and text renderer (optional): try system font + bundled fonts; otherwise skip text drawing
    let mut glyph: Option<(wgpu_glyph::GlyphBrush<()>, wgpu::util::StagingBelt)> = {
        let mut fonts: Vec<ab_glyph::FontArc> = Vec::new();
        if let Some(sys) = load_system_font() { fonts.push(sys); }
        if let Ok(f) = ab_glyph::FontArc::try_from_slice(include_bytes!("../assets/DejaVuSans.ttf")) { fonts.push(f); }
        if let Ok(f) = ab_glyph::FontArc::try_from_slice(include_bytes!("../assets/NotoSans-Regular.ttf")) { fonts.push(f); }
        if fonts.is_empty() { None } else { Some((wgpu_glyph::GlyphBrushBuilder::using_fonts(fonts).build(&device, format), wgpu::util::StagingBelt::new(1024))) }
    };

    // style helpers
    fn parse_color(style: Option<&str>, key: &str, default: [f32; 4]) -> [f32; 4] {
        let s = if let Some(s) = style { s } else { return default };
        for decl in s.split(';') {
            let d = decl.trim();
            if d.is_empty() { continue; }
            if let Some((k, v)) = d.split_once(':') {
                if k.trim() == key {
                    let v = v.trim();
                    if let Some(hex) = v.strip_prefix('#') {
                        if hex.len() == 6 {
                            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f32 / 255.0;
                            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f32 / 255.0;
                            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f32 / 255.0;
                            return [r, g, b, 1.0];
                        }
                    }
                }
            }
        }
        default
    }
    fn parse_px_f32(style: Option<&str>, key: &str, default: f32) -> f32 {
        let s = if let Some(s) = style { s } else { return default };
        for decl in s.split(';') {
            let d = decl.trim();
            if d.is_empty() { continue; }
            if let Some((k, v)) = d.split_once(':') {
                if k.trim() == key {
                    let v = v.trim();
                    let v = v.strip_suffix("px").unwrap_or(v);
                    if let Ok(f) = v.trim().parse::<f32>() { return f; }
                }
            }
        }
        default
    }
    fn parse_text_align(style: Option<&str>) -> wgpu_glyph::HorizontalAlign {
        if let Some(v) = style_lookup(style, "text-align") {
            let v = v.to_ascii_lowercase();
            if v.contains("center") { return wgpu_glyph::HorizontalAlign::Center; }
            if v.contains("right") { return wgpu_glyph::HorizontalAlign::Right; }
        }
        wgpu_glyph::HorizontalAlign::Left
    }
    fn parse_font_family_id(style: Option<&str>) -> usize {
        if let Some(v) = style_lookup(style, "font-family") {
            let v = v.to_ascii_lowercase();
            if v.contains("dejavu") { return 1; }
            if v.contains("noto") { return 2; }
        }
        0
    }
    fn style_lookup<'a>(style: Option<&'a str>, key: &str) -> Option<&'a str> {
        let s = style?;
        for decl in s.split(';') {
            let d = decl.trim(); if d.is_empty() { continue; }
            if let Some((k, v)) = d.split_once(':') { if k.trim() == key { return Some(v.trim()); } }
        }
        None
    }
    fn parse_font_weight(style: Option<&str>) -> bool {
        if let Some(v) = style_lookup(style, "font-weight") {
            if v.eq_ignore_ascii_case("bold") { return true; }
            if let Ok(n) = v.parse::<i32>() { return n >= 600; }
        }
        false
    }
    #[derive(Clone, Copy, Default)]
    struct TextDecor { underline: bool, line_through: bool }
    fn parse_text_decoration(style: Option<&str>) -> TextDecor {
        if let Some(v) = style_lookup(style, "text-decoration") {
            let mut td = TextDecor::default();
            for part in v.split_whitespace() { let p = part.trim().to_ascii_lowercase(); if p == "underline" { td.underline = true; } else if p == "line-through" { td.line_through = true; } }
            return td;
        }
        TextDecor::default()
    }
    fn approx_text_width_px(s: &str, font_size: f32) -> f32 { (s.chars().count() as f32) * font_size * 0.6 }

    // Helper to find the first element matching a predicate and return its rect and props
    fn find_node_and_rect<'a>(
        vnode: &'a velox_dom::VNode,
        layout: &velox_dom::layout::LayoutNode,
        pred: &dyn Fn(&velox_dom::VNode) -> bool,
    ) -> Option<(velox_dom::layout::Rect, &'a velox_dom::Props, &'a [velox_dom::VNode])> {
        if pred(vnode) {
            if let velox_dom::VNode::Element { props, children, .. } = vnode {
                return Some((layout.rect, props, children.as_slice()));
            }
        }
        match vnode {
            velox_dom::VNode::Element { children, .. } => {
                for (i, ch) in children.iter().enumerate() {
                    if let Some(lc) = layout.children.get(i) {
                        if let Some(found) = find_node_and_rect(ch, lc, pred) {
                            return Some(found);
                        }
                    }
                }
                None
            }
            velox_dom::VNode::Text(_) => None,
        }
    }

    // Recompute layout-derived values and GPU vertices from a vnode + stylesheet, respecting hover
    fn recompute_from_vnode(
        vnode_raw: &velox_dom::VNode,
        sheet: &Stylesheet,
        hovered_btn: bool,
        viewport_w: u32,
        viewport_h: u32,
        bg_color: &mut [f32; 4],
        text_color: &mut [f32; 4],
        font_size: &mut f32,
        btn_rect: &mut (f32, f32, f32, f32),
        btn_color: &mut [f32; 4],
        btn_text_color: &mut [f32; 4],
        btn_text: &mut String,
        btn_handler: &mut Option<String>,
        btn_pad_left: &mut f32,
        btn_pad_top: &mut f32,
        click_targets: &mut Vec<(f32,f32,f32,f32,String, Option<String>)>,
        queue: &wgpu::Queue,
        vbuf: &wgpu::Buffer,
    ) {
        let is_hovered = |tag: &str, props: &velox_dom::Props| -> bool {
            hovered_btn && (props.attrs.contains_key("on:click") || tag == "button" || has_class(props, "btn"))
        };
        let vnode = apply_styles_with_hover(vnode_raw, sheet, &is_hovered);
        // root styles
        if let velox_dom::VNode::Element { ref props, .. } = vnode {
            *bg_color = parse_color(props.attrs.get("style").map(|s| s.as_str()), "background", *bg_color);
            *text_color = parse_color(props.attrs.get("style").map(|s| s.as_str()), "color", *text_color);
            *font_size = parse_px_f32(props.attrs.get("style").map(|s| s.as_str()), "font-size", *font_size);
        }
        // layout and clickable target
        let layout = velox_dom::layout::compute_layout(&vnode, viewport_w as i32, viewport_h as i32);
        let pred = |n: &velox_dom::VNode| match n {
            velox_dom::VNode::Element { props, tag, .. } => {
                props.attrs.contains_key("on:click") || *tag == "button" || has_class(props, "btn")
            }
            _ => false,
        };
        // collect all clickable targets for event hit testing
        fn collect_clicks(vnode: &velox_dom::VNode, layout: &velox_dom::layout::LayoutNode, out: &mut Vec<(f32,f32,f32,f32,String, Option<String>)>) {
            match vnode {
                velox_dom::VNode::Text(_) => {}
                velox_dom::VNode::Element { props, children, .. } => {
                    if let Some(handler) = props.attrs.get("on:click").cloned() {
                        let payload = props.attrs.get("on:click-payload").cloned();
                        let r = layout.rect;
                        out.push((r.x as f32, r.y as f32, (r.x + r.w) as f32, (r.y + r.h) as f32, handler, payload));
                    }
                    for (i,ch) in children.iter().enumerate() {
                        if let Some(lc) = layout.children.get(i) { collect_clicks(ch, lc, out); }
                    }
                }
            }
        }
        click_targets.clear();
        collect_clicks(&vnode, &layout, click_targets);
        if let Some((r, props, children)) = find_node_and_rect(&vnode, &layout, &pred) {
            *btn_rect = (r.x as f32, r.y as f32, (r.x + r.w) as f32, (r.y + r.h) as f32);
            // element styles
            let style_str = props.attrs.get("style").map(|s| s.as_str());
            *btn_color = parse_color(style_str, "background", *btn_color);
            *btn_text_color = parse_color(style_str, "color", *text_color);
            *btn_handler = props.attrs.get("on:click").cloned();
            // padding for label position
            let pad_left = parse_px_f32(style_str, "padding-left", parse_px_f32(style_str, "padding", 0.0));
            let pad_top = parse_px_f32(style_str, "padding-top", parse_px_f32(style_str, "padding", 0.0));
            *btn_pad_left = pad_left;
            *btn_pad_top = pad_top;
            // label text: first text child
            btn_text.clear();
            for ch in children {
                if let velox_dom::VNode::Text(t) = ch { let s = t.trim(); if !s.is_empty() { btn_text.push_str(s); break; } }
            }
        }
        // update GPU vertices
        let to_ndc = |w: u32, h: u32, x: f32, y: f32| -> [f32; 2] {
            [x / w as f32 * 2.0 - 1.0, 1.0 - y / h as f32 * 2.0]
        };
        let (x0, y0, x1, y1) = *btn_rect;
        let (r, g, b) = (btn_color[0], btn_color[1], btn_color[2]);
        let verts = [
            Vertex { pos: to_ndc(viewport_w, viewport_h, x0, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(viewport_w, viewport_h, x1, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(viewport_w, viewport_h, x1, y1), color: [r, g, b] },
            Vertex { pos: to_ndc(viewport_w, viewport_h, x0, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(viewport_w, viewport_h, x1, y1), color: [r, g, b] },
            Vertex { pos: to_ndc(viewport_w, viewport_h, x0, y1), color: [r, g, b] },
        ];
        queue.write_buffer(vbuf, 0, bytemuck::cast_slice(&verts));
    }

    {
        let (vnode_raw, sheet) = make_view(config.width, config.height);
        recompute_from_vnode(&vnode_raw, &sheet, false, config.width, config.height, &mut bg_color, &mut text_color, &mut font_size, &mut btn_rect, &mut btn_color, &mut btn_text_color, &mut btn_text, &mut btn_handler, &mut btn_pad_left, &mut btn_pad_top, &mut click_targets, &queue, &vbuf);
        // set initial title from SFC state
        window.set_title(&get_title());
    }

    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => { *control_flow = ControlFlow::Exit; }
        Event::WindowEvent { event: WindowEvent::Resized(sz), .. } => {
            config.width = sz.width.max(1);
            config.height = sz.height.max(1);
            surface.configure(&device, &config);
            let (vnode_raw, sheet) = make_view(config.width, config.height);
            recompute_from_vnode(&vnode_raw, &sheet, hovered, config.width, config.height, &mut bg_color, &mut text_color, &mut font_size, &mut btn_rect, &mut btn_color, &mut btn_text_color, &mut btn_text, &mut btn_handler, &mut btn_pad_left, &mut btn_pad_top, &mut click_targets, &queue, &vbuf);
            window.request_redraw();
        }
        Event::WindowEvent { event: WindowEvent::CursorMoved { position, .. }, .. } => {
            mouse = (position.x as f32, position.y as f32);
            let (x0,y0,x1,y1) = btn_rect;
            let h = mouse.0>=x0&&mouse.0<=x1&&mouse.1>=y0&&mouse.1<=y1;
            if h!=hovered {
                hovered=h;
                // recompute styles with hover
                let (vnode_raw, sheet) = make_view(config.width, config.height);
                recompute_from_vnode(&vnode_raw, &sheet, hovered, config.width, config.height, &mut bg_color, &mut text_color, &mut font_size, &mut btn_rect, &mut btn_color, &mut btn_text_color, &mut btn_text, &mut btn_handler, &mut btn_pad_left, &mut btn_pad_top, &mut click_targets, &queue, &vbuf);
            }
        }
        Event::WindowEvent { event: WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. }, .. } => {
            // dispatch to first matching clickable rect
            if let Some((_,_,_,_, name, payload_opt)) = click_targets.iter().find(|(x0,y0,x1,y1,_,_)| mouse.0>=*x0&&mouse.0<=*x1&&mouse.1>=*y0&&mouse.1<=*y1) {
                // Prepare payload: prefer explicit payload from attribute, otherwise forward mouse coords as JSON
                let payload_owned = payload_opt.clone().unwrap_or_else(|| format!("{{\"x\":{},\"y\":{}}}", mouse.0, mouse.1));
                on_event(name, Some(&payload_owned));
                let (vnode_raw, sheet) = make_view(config.width, config.height);
                recompute_from_vnode(&vnode_raw, &sheet, hovered, config.width, config.height, &mut bg_color, &mut text_color, &mut font_size, &mut btn_rect, &mut btn_color, &mut btn_text_color, &mut btn_text, &mut btn_handler, &mut btn_pad_left, &mut btn_pad_top, &mut click_targets, &queue, &vbuf);
                window.set_title(&get_title());
                window.request_redraw();
            }
        }
        Event::RedrawRequested(_) => {
            let frame = match surface.get_current_texture() { Ok(f)=>f, Err(wgpu::SurfaceError::Lost)=>{ surface.configure(&device, &config); return; }, Err(_) => return };
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("velox-enc") });
            // Build and draw quads for all clickable buttons
            // Compute vnode + layout once for this frame
            let (frame_vnode_raw, frame_sheet) = make_view(config.width, config.height);
            // Attempt keyed reconciliation with prior frame to prefer node reuse when `key` props are present
            let frame_vnode_reconciled = if let Some(mut old) = prev_vnode.take() {
                match (&mut old, &frame_vnode_raw) {
                    (velox_dom::VNode::Element { children: old_ch, .. }, velox_dom::VNode::Element { children: new_ch, .. }) => {
                        // run keyed reconciliation on children
                        crate::reconcile_keyed_children(old_ch, new_ch);
                        old
                    }
                    _ => frame_vnode_raw.clone(),
                }
            } else {
                frame_vnode_raw.clone()
            };
            let frame_vnode = apply_styles_with_hover(&frame_vnode_reconciled, &frame_sheet, &|tag, props| hovered && (props.attrs.contains_key("on:click") || tag == "button" || has_class(props, "btn")));
            fn collect_click_nodes<'a>(vnode: &'a velox_dom::VNode, layout: &velox_dom::layout::LayoutNode, out: &mut Vec<(velox_dom::layout::Rect, &'a velox_dom::Props, &'a [velox_dom::VNode])>) {
                match vnode {
                    velox_dom::VNode::Text(_) => {}
                    velox_dom::VNode::Element { props, children, .. } => {
                        if props.attrs.contains_key("on:click") { out.push((layout.rect, props, children.as_slice())); }
                        for (i, ch) in children.iter().enumerate() { if let Some(lc) = layout.children.get(i) { collect_click_nodes(ch, lc, out); } }
                    }
                }
            }
            let layout2 = velox_dom::layout::compute_layout(&frame_vnode, config.width as i32, config.height as i32);
            let mut buttons: Vec<(velox_dom::layout::Rect, &velox_dom::Props, &[velox_dom::VNode])> = Vec::new();
            collect_click_nodes(&frame_vnode, &layout2, &mut buttons);
            let mut verts_all: Vec<Vertex> = Vec::with_capacity(buttons.len() * 6);
            for (rect, props, _) in &buttons {
                let style_str = props.attrs.get("style").map(|s| s.as_str());
                let color = parse_color(style_str, "background", [0.2,0.5,0.8,1.0]);
                let (x0,y0,x1,y1) = (rect.x as f32, rect.y as f32, (rect.x+rect.w) as f32, (rect.y+rect.h) as f32);
                let to = |x: f32, y: f32| -> [f32;2] { [ (x / config.width as f32) * 2.0 - 1.0, 1.0 - (y / config.height as f32) * 2.0 ] };
                let (r,g,b) = (color[0], color[1], color[2]);
                verts_all.push(Vertex{pos:to(x0,y0),color:[r,g,b]});
                verts_all.push(Vertex{pos:to(x1,y0),color:[r,g,b]});
                verts_all.push(Vertex{pos:to(x1,y1),color:[r,g,b]});
                verts_all.push(Vertex{pos:to(x0,y0),color:[r,g,b]});
                verts_all.push(Vertex{pos:to(x1,y1),color:[r,g,b]});
                verts_all.push(Vertex{pos:to(x0,y1),color:[r,g,b]});
            }
            {
                if !verts_all.is_empty() {
                    let quad_buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("velox-quads"), size: (verts_all.len()*std::mem::size_of::<Vertex>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                    queue.write_buffer(&quad_buf, 0, bytemuck::cast_slice(&verts_all));
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: Some("velox-pass"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: bg_color[0] as f64, g: bg_color[1] as f64, b: bg_color[2] as f64, a: bg_color[3] as f64 }), store: true } })], depth_stencil_attachment: None });
                    rpass.set_pipeline(&pipeline);
                    rpass.set_vertex_buffer(0, quad_buf.slice(..));
                    rpass.draw(0..(verts_all.len() as u32), 0..1);
                } else {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: Some("velox-pass"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: bg_color[0] as f64, g: bg_color[1] as f64, b: bg_color[2] as f64, a: bg_color[3] as f64 }), store: true } })], depth_stencil_attachment: None });
                    rpass.set_pipeline(&pipeline);
                }
            }
            // draw texts from vnode: button label and count using their own styles
            if let Some((ref mut glyph_brush, ref mut staging_belt)) = glyph {
                use wgpu_glyph::{Section, Text, Layout, HorizontalAlign, VerticalAlign, FontId};
                let (x0,y0,x1,y1) = btn_rect;
                let (vnode_raw, sheet) = make_view(config.width, config.height);
                let vnode = apply_styles_with_hover(&vnode_raw, &sheet, &|tag, props| hovered && (props.attrs.contains_key("on:click") || tag == "button" || has_class(props, "btn")));

                // helpers to locate nodes
                fn find_rect_for_class<'a>(vnode: &'a velox_dom::VNode, layout: &velox_dom::layout::LayoutNode, class: &str) -> Option<(velox_dom::layout::Rect, &'a velox_dom::Props)> {
                    match vnode {
                        velox_dom::VNode::Text(_) => None,
                        velox_dom::VNode::Element { props, children, .. } => {
                            let has = props.attrs.get("class").map(|s| s.split_whitespace().any(|c| c == class)).unwrap_or(false);
                            if has { return Some((layout.rect, props)); }
                            for (i, ch) in children.iter().enumerate() { if let Some(lc) = layout.children.get(i) { if let Some(v) = find_rect_for_class(ch, lc, class) { return Some(v); } } }
                            None
                        }
                    }
                }
                fn find_click_node<'a>(vnode: &'a velox_dom::VNode, layout: &velox_dom::layout::LayoutNode) -> Option<(&'a velox_dom::Props)> {
                    match vnode {
                        velox_dom::VNode::Text(_) => None,
                        velox_dom::VNode::Element { tag, props, children, .. } => {
                            let is_btn = props.attrs.contains_key("on:click") || *tag == "button" || props.attrs.get("class").map(|s| s.split_whitespace().any(|c| c == "btn")).unwrap_or(false);
                            if is_btn { return Some(props); }
                            for (i, ch) in children.iter().enumerate() { let _ = layout.children.get(i)?; if let Some(p) = find_click_node(ch, &layout.children[i]) { return Some(p); } }
                            None
                        }
                    }
                }
                let layout2 = velox_dom::layout::compute_layout(&vnode, config.width as i32, config.height as i32);

                // button text placement with line-height and bold/decoration
                let btn_style = find_click_node(&vnode, &layout2).and_then(|p| p.attrs.get("style")).map(|s| s.as_str());
                let btn_line_h = parse_px_f32(btn_style, "line-height", font_size);
                let btn_font_size = parse_px_f32(btn_style, "font-size", font_size);
                // padding right/bottom
                let btn_pad_right = parse_px_f32(btn_style, "padding-right", parse_px_f32(btn_style, "padding", 0.0));
                let btn_pad_bottom = parse_px_f32(btn_style, "padding-bottom", parse_px_f32(btn_style, "padding", 0.0));
                // text top-left for glyph_brush (Section position is top-left), vertically centered in line box
                let mut label_pos = (x0 + btn_pad_left, y0 + btn_pad_top + (btn_line_h - btn_font_size).max(0.0) * 0.5);
                if label_pos.1 + btn_font_size > y1 - 1.0 { label_pos.1 = (y1 - 1.0 - btn_font_size).max(y0 + btn_pad_top); }
                let label = if btn_text.is_empty() { String::new() } else { btn_text.clone() };
                let btn_td = parse_text_decoration(btn_style);
                let btn_bold = parse_font_weight(btn_style);
                let btn_italic = style_lookup(btn_style, "font-style").map(|v| v.eq_ignore_ascii_case("italic")).unwrap_or(false);
                let btn_align = parse_text_align(btn_style);
                let btn_font_id = parse_font_family_id(btn_style);
                if !label.is_empty() {
                    let mut offsets: Vec<(f32,f32)> = if btn_bold { vec![(0.0,0.0),(0.6,0.0),(0.0,0.6)] } else { vec![(0.0,0.0)] };
                    if btn_italic { offsets.push((0.4, 0.0)); }
                    let bounds = ( (x1 - x0 - btn_pad_left - btn_pad_right).max(0.0), (y1 - y0 - btn_pad_top - btn_pad_bottom).max(0.0) );
                    let layout = Layout::default().h_align(btn_align).v_align(VerticalAlign::Top);
                    for (ox, oy) in offsets {
                        glyph_brush.queue(Section {
                            screen_position: (label_pos.0 + ox, label_pos.1 + oy),
                            bounds,
                            layout,
                            text: vec![Text::new(&label).with_color(btn_text_color).with_scale(btn_font_size).with_font_id(FontId(btn_font_id))],
                            ..Default::default()
                        });
                    }
                }

                // Save reconciled vnode for next frame
                prev_vnode = Some(frame_vnode_reconciled);

                // count text placement with its own padding/line-height and bold/decoration
                let (count_text, count_pos, count_style, count_bounds) = if let Some((rect, props)) = find_rect_for_class(&vnode, &layout2, "count") {
                    let style_str = props.attrs.get("style").map(|s| s.as_str());
                    let cp_l = parse_px_f32(style_str, "padding-left", parse_px_f32(style_str, "padding", 0.0));
                    let cp_t = parse_px_f32(style_str, "padding-top", parse_px_f32(style_str, "padding", 0.0));
                    let cp_r = parse_px_f32(style_str, "padding-right", parse_px_f32(style_str, "padding", 0.0));
                    let cp_b = parse_px_f32(style_str, "padding-bottom", parse_px_f32(style_str, "padding", 0.0));
                    let line_h = parse_px_f32(style_str, "line-height", font_size);
                    let count_font_size = parse_px_f32(style_str, "font-size", font_size);
                    let mut pos_y = rect.y as f32 + cp_t + (line_h - count_font_size).max(0.0) * 0.5;
                    if pos_y + count_font_size > (rect.y + rect.h - 1) as f32 { pos_y = (rect.y + rect.h - 1) as f32 - count_font_size; }
                    let pos = (rect.x as f32 + cp_l, pos_y);
                    // Allow vertical overflow to be visible by giving a tall bound down to bottom of viewport
                    let bounds_h = (config.height as f32 - rect.y as f32).max((rect.h as f32 - cp_t - cp_b).max(0.0));
                    let bounds = ( (rect.w as f32 - cp_l - cp_r).max(0.0), bounds_h );
                    (find_text_in_class(&vnode, "count").unwrap_or_default(), pos, style_str, bounds)
                } else { (String::new(), (x0, y0), None, (0.0, 0.0)) };
                let count_td = parse_text_decoration(count_style);
                let count_bold = parse_font_weight(count_style);
                let count_italic = style_lookup(count_style, "font-style").map(|v| v.eq_ignore_ascii_case("italic")).unwrap_or(false);
                let count_align = parse_text_align(count_style);
                let count_font_id = parse_font_family_id(count_style);
                if !count_text.is_empty() {
                    let mut offsets: Vec<(f32,f32)> = if count_bold { vec![(0.0,0.0),(0.6,0.0),(0.0,0.6)] } else { vec![(0.0,0.0)] };
                    if count_italic { offsets.push((0.4, 0.0)); }
                    let count_font_size = parse_px_f32(count_style, "font-size", font_size);
                    let layout = Layout::default().h_align(count_align).v_align(VerticalAlign::Top);
                    for (ox, oy) in offsets {
                        glyph_brush.queue(Section {
                            screen_position: (count_pos.0 + ox, count_pos.1 + oy),
                            bounds: count_bounds,
                            layout,
                            text: vec![Text::new(&count_text).with_color(text_color).with_scale(count_font_size).with_font_id(FontId(count_font_id))],
                            ..Default::default()
                        });
                    }
                }
                let _ = glyph_brush.draw_queued(&device, staging_belt, &mut encoder, &view, config.width, config.height);
                staging_belt.finish();
                // Text decorations as thin quads in a second pass
                let mut deco_verts: Vec<Vertex> = Vec::new();
                let mut push_rect = |x0: f32, y0: f32, x1: f32, y1: f32, color: [f32;3]| {
                    let to = |x: f32, y: f32| [ (x / config.width as f32) * 2.0 - 1.0, 1.0 - (y / config.height as f32) * 2.0 ];
                    deco_verts.push(Vertex { pos: to(x0,y0), color });
                    deco_verts.push(Vertex { pos: to(x1,y0), color });
                    deco_verts.push(Vertex { pos: to(x1,y1), color });
                    deco_verts.push(Vertex { pos: to(x0,y0), color });
                    deco_verts.push(Vertex { pos: to(x1,y1), color });
                    deco_verts.push(Vertex { pos: to(x0,y1), color });
                };
                let thickness = 1.0f32.max(font_size.max(parse_px_f32(btn_style, "font-size", font_size)).max(parse_px_f32(count_style, "font-size", font_size)) * 0.06);
                if !label.is_empty() && (btn_td.underline || btn_td.line_through) {
                    let fs = parse_px_f32(btn_style, "font-size", font_size);
                    let w = approx_text_width_px(&label, fs);
                    let y_u = (label_pos.1 + fs + thickness).min(y1 - 1.0);
                    let y_s = label_pos.1 + fs * 0.65;
                    if btn_td.underline { push_rect(label_pos.0, y_u, label_pos.0 + w, (y_u + thickness).min(y1 - 1.0), [btn_text_color[0], btn_text_color[1], btn_text_color[2]]); }
                    if btn_td.line_through { push_rect(label_pos.0, y_s, label_pos.0 + w, y_s + thickness, [btn_text_color[0], btn_text_color[1], btn_text_color[2]]); }
                    // overline
                    if style_lookup(btn_style, "text-decoration").map(|v| v.to_ascii_lowercase().contains("overline")).unwrap_or(false) {
                        let y_o = (label_pos.1).max(y0 + btn_pad_top);
                        push_rect(label_pos.0, y_o, label_pos.0 + w, (y_o + thickness).min(y1 - 1.0), [btn_text_color[0], btn_text_color[1], btn_text_color[2]]);
                    }
                }
                if !count_text.is_empty() && (count_td.underline || count_td.line_through) {
                    let cf = parse_px_f32(count_style, "font-size", font_size);
                    let w = approx_text_width_px(&count_text, cf);
                    let y_u = count_pos.1 + cf + thickness;
                    let y_s = count_pos.1 + cf * 0.65;
                    if count_td.underline { push_rect(count_pos.0, y_u, count_pos.0 + w, y_u + thickness, [text_color[0], text_color[1], text_color[2]]); }
                    if count_td.line_through { push_rect(count_pos.0, y_s, count_pos.0 + w, y_s + thickness, [text_color[0], text_color[1], text_color[2]]); }
                    if style_lookup(count_style, "text-decoration").map(|v| v.to_ascii_lowercase().contains("overline")).unwrap_or(false) {
                        let y_o = count_pos.1;
                        push_rect(count_pos.0, y_o, count_pos.0 + w, y_o + thickness, [text_color[0], text_color[1], text_color[2]]);
                    }
                }
                if !deco_verts.is_empty() {
                    let deco_buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("velox-deco"), size: (deco_verts.len() * std::mem::size_of::<Vertex>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                    queue.write_buffer(&deco_buf, 0, bytemuck::cast_slice(&deco_verts));
                    {
                        let mut rpass2 = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: Some("velox-deco-pass"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: true } })], depth_stencil_attachment: None });
                        rpass2.set_pipeline(&pipeline);
                        rpass2.set_vertex_buffer(0, deco_buf.slice(..));
                        rpass2.draw(0..(deco_verts.len() as u32), 0..1);
                    }
                }
                queue.submit(Some(encoder.finish()));
                device.poll(wgpu::Maintain::Wait);
                staging_belt.recall();
                frame.present();
            } else {
                queue.submit(Some(encoder.finish()));
                frame.present();
            }
        }
        Event::MainEventsCleared => { window.request_redraw(); }
        _ => {}
    });
}

// Minimal window runner using winit when `wgpu` feature is enabled.
#[cfg(feature = "wgpu")]
pub fn run_window(title: &str) {
    use wgpu::SurfaceError;
    use winit::dpi::PhysicalSize;
    use winit::event::{Event, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    println!("[window] launching '{}'", title);
    let event_loop = EventLoop::new();
    let window = match WindowBuilder::new()
        .with_title(title)
        .with_inner_size(PhysicalSize::new(800, 600))
        .build(&event_loop)
    {
        Ok(w) => {
            println!("[window] opened: {}", title);
            w
        }
        Err(e) => {
            eprintln!("[window] failed to create window: {}", e);
            return;
        }
    };

    // WGPU setup
    let instance = wgpu::Instance::default();
    let surface = unsafe { instance.create_surface(&window) }.expect("create surface");
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .expect("no suitable GPU adapters");
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("velox-device"),
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        },
        None,
    ))
    .expect("request device");

    let mut size = window.inner_size();
    if size.width == 0 || size.height == 0 {
        size = PhysicalSize::new(800, 600);
        window.set_inner_size(size);
    }
    let surface_caps = surface.get_capabilities(&adapter);
    let format = surface_caps.formats[0];
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width,
        height: size.height,
        present_mode: surface_caps.present_modes[0],
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    // Simple colored quad pipeline (two triangles) for a button placeholder
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex { pos: [f32; 2], color: [f32; 3] }

    let shader_src = r#"
        struct VsOut {
            @builtin(position) position: vec4<f32>,
            @location(0) color: vec3<f32>,
        };

        @vertex
        fn vs(@location(0) pos: vec2<f32>, @location(1) color: vec3<f32>) -> VsOut {
            var out: VsOut;
            out.position = vec4<f32>(pos, 0.0, 1.0);
            out.color = color;
            return out;
        }

        @fragment
        fn fs(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> {
            return vec4<f32>(color, 1.0);
        }
    "#;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("velox-shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });

    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 8, shader_location: 1 },
        ],
    };

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("velox-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("velox-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: "vs", buffers: &[vertex_layout] },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs",
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    // Button rect in pixel space; we convert to NDC in create_vertices
    let mut mouse_pos: (f32, f32) = (0.0, 0.0);
    let mut count: i32 = 0;
    let mut hovered = false;

    let mut vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("velox-vertices"), size: 6 * std::mem::size_of::<Vertex>() as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false
    });

    let create_vertices = |w: u32, h: u32, hovered: bool| -> [Vertex; 6] {
        let bw = 200.0; let bh = 80.0;
        let cx = w as f32 / 2.0; let cy = h as f32 / 2.0;
        let x0 = cx - bw / 2.0; let y0 = cy - bh / 2.0; let x1 = cx + bw / 2.0; let y1 = cy + bh / 2.0;
        let to_ndc = |x: f32, y: f32| -> [f32; 2] { [ (x / w as f32) * 2.0 - 1.0, 1.0 - (y / h as f32) * 2.0 ] };
        let (r,g,b) = if hovered { (0.25, 0.6, 0.9) } else { (0.2, 0.5, 0.8) };
        [
            Vertex { pos: to_ndc(x0, y0), color: [r,g,b] },
            Vertex { pos: to_ndc(x1, y0), color: [r,g,b] },
            Vertex { pos: to_ndc(x1, y1), color: [r,g,b] },
            Vertex { pos: to_ndc(x0, y0), color: [r,g,b] },
            Vertex { pos: to_ndc(x1, y1), color: [r,g,b] },
            Vertex { pos: to_ndc(x0, y1), color: [r,g,b] },
        ]
    };

    // initial vertices
    let verts = create_vertices(config.width, config.height, hovered);
    queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts));

    fn render(
        surface: &wgpu::Surface,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
        render_pipeline: &wgpu::RenderPipeline,
        vertex_buffer: &wgpu::Buffer,
    ) -> Result<(), SurfaceError> {
        let frame = surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("velox-encoder") });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("velox-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.12, g: 0.12, b: 0.14, a: 1.0 }), store: true }
                })],
                depth_stencil_attachment: None,
            });
            rpass.set_pipeline(render_pipeline);
            rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
            rpass.draw(0..6, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    let mut redraw_pending = true;
    // Move owned state into the event loop
    let mut config = config;
    let mut surface = surface;
    let mut device = device;
    let mut queue = queue;
    let mut vertex_buffer = vertex_buffer;
    let render_pipeline = render_pipeline;
    let title_owned = title.to_string();

    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::WindowEvent { event: WindowEvent::Resized(new_size), .. } => {
            config.width = new_size.width.max(1);
            config.height = new_size.height.max(1);
            surface.configure(&device, &config);
            let verts2 = create_vertices(config.width, config.height, hovered);
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts2));
            redraw_pending = true;
        }
        Event::WindowEvent { event: WindowEvent::CursorMoved { position, .. }, .. } => {
            mouse_pos = (position.x as f32, position.y as f32);
            let bw = 200.0; let bh = 80.0;
            let cx = config.width as f32 / 2.0; let cy = config.height as f32 / 2.0;
            let x0 = cx - bw/2.0; let y0 = cy - bh/2.0; let x1 = cx + bw/2.0; let y1 = cy + bh/2.0;
            let now_hovered = mouse_pos.0 >= x0 && mouse_pos.0 <= x1 && mouse_pos.1 >= y0 && mouse_pos.1 <= y1;
            if now_hovered != hovered { hovered = now_hovered; let verts3 = create_vertices(config.width, config.height, hovered); queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts3)); }
            window.request_redraw();
        }
        Event::WindowEvent { event: WindowEvent::MouseInput { state: winit::event::ElementState::Pressed, button: winit::event::MouseButton::Left, .. }, .. } => {
            let bw = 200.0; let bh = 80.0;
            let cx = config.width as f32 / 2.0; let cy = config.height as f32 / 2.0;
            let x0 = cx - bw/2.0; let y0 = cy - bh/2.0; let x1 = cx + bw/2.0; let y1 = cy + bh/2.0;
            if mouse_pos.0 >= x0 && mouse_pos.0 <= x1 && mouse_pos.1 >= y0 && mouse_pos.1 <= y1 {
                count += 1;
                window.set_title(&format!("{} — count {}", title_owned, count));
            }
        }
        Event::MainEventsCleared => {
            if redraw_pending {
                window.request_redraw();
            }
        }
        Event::RedrawRequested(_) => {
            match render(&surface, &device, &queue, &config, &render_pipeline, &vertex_buffer) {
                Ok(()) => {}
                Err(SurfaceError::Lost) => { surface.configure(&device, &config); }
                Err(SurfaceError::OutOfMemory) => { *control_flow = ControlFlow::Exit; }
                Err(_) => {}
            }
            redraw_pending = false;
        }
        _ => {}
    });
}

#[cfg(feature = "wgpu")]
pub fn run_window_counter<F>(title: &str, mut on_change: F)
where
    F: FnMut(i32) + 'static,
{
    use winit::dpi::PhysicalSize;
    use winit::event::{Event, WindowEvent, ElementState, MouseButton};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().with_title(title).with_inner_size(PhysicalSize::new(800,600)).build(&event_loop).expect("window");
    let title_owned = title.to_string();

    // Reuse the rendering path
    // Minimal re-init by calling into `run_window`-like setup inline to avoid refactor
    let instance = wgpu::Instance::default();
    let surface = unsafe { instance.create_surface(&window) }.expect("surface");
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), force_fallback_adapter: false })).expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: Some("velox-device"), features: wgpu::Features::empty(), limits: wgpu::Limits::default() }, None)).expect("device");
    let mut size = window.inner_size();
    if size.width == 0 || size.height == 0 { size = PhysicalSize::new(800, 600); window.set_inner_size(size); }
    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats[0];
    let mut config = wgpu::SurfaceConfiguration { usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format, width: size.width, height: size.height, present_mode: caps.present_modes[0], alpha_mode: caps.alpha_modes[0], view_formats: vec![] };
    surface.configure(&device, &config);

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex { pos: [f32; 2], color: [f32; 3] }
    let shader_src = r#"
        struct VsOut { @builtin(position) position: vec4<f32>, @location(0) color: vec3<f32>, };
        @vertex fn vs(@location(0) pos: vec2<f32>, @location(1) color: vec3<f32>) -> VsOut {
            var out: VsOut; out.position = vec4<f32>(pos, 0.0, 1.0); out.color = color; return out;
        }
        @fragment fn fs(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> { return vec4<f32>(color, 1.0); }
    "#;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("velox-shader"), source: wgpu::ShaderSource::Wgsl(shader_src.into()) });
    let vlayout = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress, step_mode: wgpu::VertexStepMode::Vertex, attributes: &[
        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 8, shader_location: 1 },
    ]};
    let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("velox-pl"), bind_group_layouts: &[], push_constant_ranges: &[] });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: Some("velox-pipeline"), layout: Some(&pl_layout), vertex: wgpu::VertexState { module: &shader, entry_point: "vs", buffers: &[vlayout] }, fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs", targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }), primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None });
    let mut vbuf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("velox-vbuf"), size: 6 * std::mem::size_of::<Vertex>() as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

    let make_quad = |w: u32, h: u32, hovered: bool| -> [Vertex; 6] {
        let bw = 200.0; let bh = 80.0; let cx = w as f32 / 2.0; let cy = h as f32 / 2.0;
        let x0 = cx - bw/2.0; let y0 = cy - bh/2.0; let x1 = cx + bw/2.0; let y1 = cy + bh/2.0;
        let to_ndc = |x: f32, y: f32| [ (x / w as f32) * 2.0 - 1.0, 1.0 - (y / h as f32) * 2.0 ];
        let (r,g,b) = if hovered { (0.25,0.6,0.9) } else { (0.2,0.5,0.8) };
        [ Vertex{pos:to_ndc(x0,y0),color:[r,g,b]}, Vertex{pos:to_ndc(x1,y0),color:[r,g,b]}, Vertex{pos:to_ndc(x1,y1),color:[r,g,b]}, Vertex{pos:to_ndc(x0,y0),color:[r,g,b]}, Vertex{pos:to_ndc(x1,y1),color:[r,g,b]}, Vertex{pos:to_ndc(x0,y1),color:[r,g,b]} ]
    };
    let mut hovered = false;
    queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&make_quad(config.width, config.height, hovered)));
    let mut mouse = (0.0f32, 0.0f32);
    let mut count = 0;
    on_change(count);

    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => { *control_flow = ControlFlow::Exit; }
        Event::WindowEvent { event: WindowEvent::Resized(sz), .. } => { config.width = sz.width.max(1); config.height = sz.height.max(1); surface.configure(&device, &config); queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&make_quad(config.width, config.height, hovered))); window.request_redraw(); }
        Event::WindowEvent { event: WindowEvent::CursorMoved { position, .. }, .. } => { mouse = (position.x as f32, position.y as f32); let bw=200.0; let bh=80.0; let cx=config.width as f32/2.0; let cy=config.height as f32/2.0; let x0=cx-bw/2.0; let y0=cy-bh/2.0; let x1=cx+bw/2.0; let y1=cy+bh/2.0; let h = mouse.0>=x0 && mouse.0<=x1 && mouse.1>=y0 && mouse.1<=y1; if h!=hovered { hovered=h; queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&make_quad(config.width, config.height, hovered))); } window.request_redraw(); }
        Event::WindowEvent { event: WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. }, .. } => { let bw=200.0; let bh=80.0; let cx=config.width as f32/2.0; let cy=config.height as f32/2.0; let x0=cx-bw/2.0; let y0=cy-bh/2.0; let x1=cx+bw/2.0; let y1=cy+bh/2.0; if mouse.0>=x0 && mouse.0<=x1 && mouse.1>=y0 && mouse.1<=y1 { count += 1; window.set_title(&format!("{} — count {}", title_owned, count)); on_change(count); } }
        Event::RedrawRequested(_) => {
            let frame = match surface.get_current_texture() { Ok(f)=>f, Err(wgpu::SurfaceError::Lost)=>{ surface.configure(&device, &config); return; }, Err(_) => return };
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("velox-enc") });
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: Some("velox-pass"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.12, g: 0.12, b: 0.14, a: 1.0 }), store: true } })], depth_stencil_attachment: None });
                rpass.set_pipeline(&pipeline);
                rpass.set_vertex_buffer(0, vbuf.slice(..));
                rpass.draw(0..6, 0..1);
            }
            queue.submit(Some(encoder.finish()));
            frame.present();
        }
        Event::MainEventsCleared => { window.request_redraw(); }
        _ => {}
    });
}

#[cfg(feature = "wgpu")]
pub fn run_counter_window() {
    use winit::event::{ElementState, Event, MouseButton, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Velox - Count: 0 (click to increment)")
        .build(&event_loop)
        .expect("create window");

    let mut count: i32 = 0;
    let mut update_title = move |c: i32| {
        window.set_title(&format!("Velox - Count: {} (click to increment)", c));
    };

    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::WindowEvent { event: WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. }, .. } => {
            count += 1;
            update_title(count);
        }
        Event::MainEventsCleared => {}
        _ => {}
    });
}
