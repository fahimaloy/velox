//! Renderer crate with optional backends.
//! No features enabled => stub, compiles fast.

#[cfg(feature = "wgpu")]
pub mod wgpu_backend {
    use wgpu as _wgpu;
    use winit as _winit;

    pub fn init() {
        // TODO: implement Wayland (winit) + wgpu surface setup
    }
}

#[cfg(feature = "skia")]
pub mod skia_backend {
    use skia_safe as _sk;

    pub fn init() {
        // TODO: implement Skia GPU surface creation (GL/EGL)
    }
}

/// Stub init used when no backend features are enabled.
#[cfg(not(any(feature = "wgpu", feature = "skia")))]
pub fn init() {
    // Intentionally empty
}
