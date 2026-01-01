//! Minimal Skia surface wrapper (Phase 1).
//!
//! Provides a small `SkiaSurface` helper for raster surfaces and a
//! placeholder for future window-backed surface creation.
#![allow(unused)]

#[cfg(feature = "skia-native")]
mod native {
    use skia_safe as sk;
    use std::path::Path;
    use glow;
    use glow::HasContext;

    pub struct SkiaSurface {
        surface: sk::Surface,
        pub width: i32,
        pub height: i32,
        // Optional GPU context if available (kept for future extension)
        pub _gpu_ctx: Option<sk::gpu::DirectContext>,
        // Keep the native GL/EGL context alive while the Skia surface exists.
        #[cfg(all(feature = "skia-native", unix))]
        pub _gl_ctx: Option<crate::skia_gl::SkiaGlContext>,
    }

    impl SkiaSurface {
        /// Create a CPU raster SkiaSurface.
        pub fn new_raster(width: i32, height: i32) -> Result<Self, String> {
            let surface = sk::surfaces::raster_n32_premul((width, height))
                .ok_or_else(|| "skia: failed to create raster surface".to_string())?;
            Ok(SkiaSurface { surface, width, height, _gpu_ctx: None, #[cfg(all(feature = "skia-native", unix))] _gl_ctx: None })
        }

        /// Return a reference to the canvas.
            pub fn canvas(&mut self) -> &sk::Canvas {
            // Prefer the mutable canvas accessor when available.
            #[allow(deprecated)]
            {
                // `canvas_mut` is the explicit mutable accessor in some skia-safe versions.
                // Fall back to calling `canvas()` on a mutable receiver if needed.
                if false {
                    // no-op to keep fallback logic clear
                }
            }
            self.surface.canvas()
        }

        /// Save the current surface snapshot to a PNG file (for debugging/tests).
        pub fn save_png<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
            let img = self.surface.image_snapshot();
            #[allow(deprecated)]
            let data = img
                .encode_to_data(skia_safe::EncodedImageFormat::PNG)
                .ok_or_else(|| "skia: failed to encode image".to_string())?;
            std::fs::write(path, data.as_bytes()).map_err(|e| format!("write failed: {}", e))
        }

        /// Present or flush any GPU work for this surface.
        ///
        /// For GPU-backed surfaces this will flush and submit the `DirectContext`.
        /// For raster surfaces this is a no-op.
        pub fn present(&mut self) -> Result<(), String> {
            if let Some(dctx) = &mut self._gpu_ctx {
                dctx.flush_and_submit();
            }
            Ok(())
        }
    }

        /// Attempt to create a window-backed Skia surface from a raw-window-handle.
        ///
        /// This function tries to create a native GL/EGL context for the provided
        /// `HasWindowHandle` and, if successful, will attempt to create a
        /// `DirectContext`. For Phase 1 we return a raster surface if creating a
        /// GPU-backed surface is not yet supported.
        pub fn create_window_surface_from_handle(
            window: &impl raw_window_handle::HasWindowHandle,
            width: i32,
            height: i32,
        ) -> Result<SkiaSurface, String> {
            // Try to create a native GL/EGL context using the helper in `skia_gl`.
            match crate::skia_gl::create_context_from_winit(window) {
                Ok(gl_ctx) => {
                    // Try to make a DirectContext from the GL interface.
                    if let Some(mut dctx) = gl_ctx.into_direct_context() {
                        eprintln!("[skia_surface] DirectContext created (GPU path available)");
                        // Attempt to build a GPU-backed Skia surface from the DirectContext.
                        // If successful, return a SkiaSurface that owns both the DirectContext
                        // and the native GL context so they are kept alive for the lifetime
                        // of the surface.
                        if let Some(gpu_surf) = create_gpu_surface_from_direct_context(&mut dctx, width, height) {
                            return Ok(SkiaSurface { surface: gpu_surf, width, height, _gpu_ctx: Some(dctx), #[cfg(all(feature = "skia-native", unix))] _gl_ctx: Some(gl_ctx) });
                        }
                        // Fallback to raster until the platform-specific path is implemented.
                        let surface = sk::surfaces::raster_n32_premul((width, height))
                            .ok_or_else(|| "skia: failed to create raster fallback surface".to_string())?;
                        return Ok(SkiaSurface { surface, width, height, _gpu_ctx: Some(dctx), #[cfg(all(feature = "skia-native", unix))] _gl_ctx: Some(gl_ctx) });
                    } else {
                        eprintln!("[skia_surface] Could not make DirectContext; falling back to raster");
                    }
                }
                Err(e) => {
                    eprintln!("[skia_surface] create_context_from_winit failed: {}", e);
                }
            }

            // Fallback to CPU raster surface
            SkiaSurface::new_raster(width, height)
        }

    pub use SkiaSurface as Surface;

    /// Attempt to create a GPU-backed Skia surface from an existing DirectContext.
    ///
    /// This is intentionally a stub: creating a `BackendRenderTarget` is
    /// platform-specific (GL/EGL vs Metal vs D3D) and requires native handles.
    /// Implementations should create a valid BackendRenderTarget and then call
    /// `skia_safe::gpu::Surface::from_backend_render_target()` or similar.
    fn create_gpu_surface_from_direct_context(
        dctx: &mut sk::gpu::DirectContext,
        width: i32,
        height: i32,
    ) -> Option<sk::Surface> {
        // Attempt to query the current GL framebuffer and wrap it as a
        // Skia BackendRenderTarget. This is a best-effort implementation
        // for EGL/GL on Unix. If anything fails, return None and the
        // caller will fall back to a raster surface.
        unsafe {
            // Create a glow context loader using EGL's get_proc_address.
            let gl = glow::Context::from_loader_function(|s| egl::get_proc_address(s) as *const _);

            // Query the currently bound FBO.
            let fb_binding = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) as u32;

            // Choose a color format. We assume a standard RGBA8 buffer here.
            let gl_format = glow::RGBA8;

            let fb_info = sk::gpu::gl::FramebufferInfo { fboid: fb_binding, format: gl_format, protected: sk::gpu::Protected::No };

            // Create a BackendRenderTarget for GL.
            let backend = sk::gpu::BackendRenderTarget::new_gl((width, height), 0, 8, fb_info);

            // Create a Skia GPU-backed Surface from the backend render target.
            let surface = sk::Surface::from_backend_render_target(
                dctx,
                &backend,
                sk::gpu::SurfaceOrigin::BottomLeft,
                sk::ColorType::RGBA8888,
                None,
                None,
            );

            if surface.is_some() {
                eprintln!("[skia_surface] created GPU-backed Surface (fbo={})", fb_binding);
            } else {
                eprintln!("[skia_surface] Surface::from_backend_render_target returned None");
            }

            surface
        }
    }
}

#[cfg(not(feature = "skia-native"))]
mod nostub {
    pub struct Surface { _private: () }
    pub fn create_window_surface(_w: i32, _h: i32) -> Result<Surface, String> { Err("skia-native feature not enabled".into()) }
    pub use Surface as SkiaSurface;
}

#[cfg(feature = "skia-native")]
pub use native::*;
#[cfg(not(feature = "skia-native"))]
pub use nostub::*;
