//! Renderer crate with optional backends.
//! No features enabled => stub, compiles fast.

use velox_dom::VNode;

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
        // TODO: implement Wayland (winit) + wgpu surface setup
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
pub fn run_window_vnode<F, G>(title: &str, mut make_view: F, mut on_click: G)
where
    F: FnMut(u32, u32) -> velox_dom::VNode + 'static,
    G: FnMut() + 'static,
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
    let title_owned = title.to_string();

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
    let mut btn_rect: (f32, f32, f32, f32) = (0.0, 0.0, 0.0, 0.0);
    let mut hovered = false;
    let mut mouse = (0.0f32, 0.0f32);
    let mut bg_color: [f32; 4] = [0.12, 0.12, 0.14, 1.0];
    let mut text_color: [f32; 4] = [0.90, 0.93, 0.95, 1.0];
    let mut font_size: f32 = 18.0;

    let make_vertices = |w: u32, h: u32, r: (f32, f32, f32, f32), hov: bool| -> [Vertex; 6] {
        let (x0, y0, x1, y1) = r;
        let (r, g, b) = if hov { (0.25, 0.6, 0.9) } else { (0.2, 0.5, 0.8) };
        [
            Vertex { pos: to_ndc(w, h, x0, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x1, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x1, y1), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x0, y0), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x1, y1), color: [r, g, b] },
            Vertex { pos: to_ndc(w, h, x0, y1), color: [r, g, b] },
        ]
    };

    // Font and text renderer
    let font = ab_glyph::FontArc::try_from_slice(include_bytes!("../assets/DejaVuSans.ttf")).expect("font");
    let mut glyph_brush = wgpu_glyph::GlyphBrushBuilder::using_font(font).build(&device, format);
    let mut staging_belt = wgpu::util::StagingBelt::new(1024);

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

    // Initial compute
    {
        let vnode = make_view(config.width, config.height);
        let layout = velox_dom::layout::compute_layout(&vnode, config.width as i32);
        // root style
        if let velox_dom::VNode::Element { props, .. } = &vnode { bg_color = parse_color(props.attrs.get("style").map(|s| s.as_str()), "background", bg_color); text_color = parse_color(props.attrs.get("style").map(|s| s.as_str()), "color", text_color); font_size = parse_px_f32(props.attrs.get("style").map(|s| s.as_str()), "font-size", font_size); }
        if let Some(first) = layout.children.get(0) {
            let r = first.rect; btn_rect = (r.x as f32, r.y as f32, (r.x + r.w) as f32, (r.y + r.h) as f32);
        } else {
            let bw = 200.0; let bh = 80.0; let cx = config.width as f32/2.0; let cy = config.height as f32/2.0;
            btn_rect = (cx-bw/2.0, cy-bh/2.0, cx+bw/2.0, cy+bh/2.0);
        }
        let verts = make_vertices(config.width, config.height, btn_rect, hovered);
        queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&verts));
    }

    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => { *control_flow = ControlFlow::Exit; }
        Event::WindowEvent { event: WindowEvent::Resized(sz), .. } => { config.width = sz.width.max(1); config.height = sz.height.max(1); surface.configure(&device, &config);
            let vnode = make_view(config.width, config.height);
            let layout = velox_dom::layout::compute_layout(&vnode, config.width as i32);
            if let Some(first) = layout.children.get(0) { let r = first.rect; btn_rect = (r.x as f32, r.y as f32, (r.x + r.w) as f32, (r.y + r.h) as f32); }
            let verts = make_vertices(config.width, config.height, btn_rect, hovered);
            queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&verts)); window.request_redraw(); }
        Event::WindowEvent { event: WindowEvent::CursorMoved { position, .. }, .. } => { mouse = (position.x as f32, position.y as f32); let (x0,y0,x1,y1) = btn_rect; let h = mouse.0>=x0&&mouse.0<=x1&&mouse.1>=y0&&mouse.1<=y1; if h!=hovered { hovered=h; queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&make_vertices(config.width, config.height, btn_rect, hovered))); } }
        Event::WindowEvent { event: WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. }, .. } => { let (x0,y0,x1,y1) = btn_rect; if mouse.0>=x0&&mouse.0<=x1&&mouse.1>=y0&&mouse.1<=y1 { on_click();
            let vnode = make_view(config.width, config.height);
            let layout = velox_dom::layout::compute_layout(&vnode, config.width as i32);
            if let Some(first) = layout.children.get(0) { let r = first.rect; btn_rect = (r.x as f32, r.y as f32, (r.x + r.w) as f32, (r.y + r.h) as f32); }
            let verts = make_vertices(config.width, config.height, btn_rect, hovered);
            queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&verts)); window.request_redraw(); } }
        Event::RedrawRequested(_) => {
            let frame = match surface.get_current_texture() { Ok(f)=>f, Err(wgpu::SurfaceError::Lost)=>{ surface.configure(&device, &config); return; }, Err(_) => return };
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("velox-enc") });
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: Some("velox-pass"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: bg_color[0] as f64, g: bg_color[1] as f64, b: bg_color[2] as f64, a: bg_color[3] as f64 }), store: true } })], depth_stencil_attachment: None });
                rpass.set_pipeline(&pipeline);
                rpass.set_vertex_buffer(0, vbuf.slice(..));
                rpass.draw(0..6, 0..1);
            }
            // draw texts: button label and count near the button rect
            let (x0,y0,x1,y1) = btn_rect;
            let label_pos = (x0 + 16.0, y0 + font_size + 16.0);
            let count_pos = (x0, y1 + font_size + 12.0);
            use wgpu_glyph::{Section, Text};
            glyph_brush.queue(Section { screen_position: (label_pos.0, label_pos.1), text: vec![Text::new("Increment").with_color(text_color).with_scale(font_size)], ..Default::default() });
            glyph_brush.queue(Section { screen_position: (count_pos.0, count_pos.1), text: vec![Text::new("count").with_color(text_color).with_scale(font_size)], ..Default::default() });
            let _ = glyph_brush.draw_queued(&device, &mut staging_belt, &mut encoder, &view, config.width, config.height);
            staging_belt.finish();
            device.poll(wgpu::Maintain::Wait);
            staging_belt.recall();
            queue.submit(Some(encoder.finish()));
            frame.present();
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
