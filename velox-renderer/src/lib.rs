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

// Minimal window runner using winit when `wgpu` feature is enabled.
#[cfg(feature = "wgpu")]
pub fn run_window(title: &str) {
    use winit::event::{Event, WindowEvent};
    use winit::event_loop::EventLoop;
    use winit::window::WindowBuilder;

    let event_loop = EventLoop::new();
    let _window = WindowBuilder::new()
        .with_title(title)
        .build(&event_loop)
        .expect("create window");

    use winit::event_loop::ControlFlow;
    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::MainEventsCleared => {
            // request a redraw; no rendering yet
        }
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
