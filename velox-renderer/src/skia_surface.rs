//! Minimal Skia surface wrapper (Phase 1).
//!
//! Provides a small `SkiaSurface` helper for raster surfaces and a
//! placeholder for future window-backed surface creation.
#![allow(unused)]

#[cfg(feature = "skia-native")]
mod native {
    use glow::HasContext;
    use skia_safe as sk;
    use std::path::Path;

    use crate::viewport::Viewport;

    pub struct SkiaSurface {
        surface: sk::Surface,
        pub viewport: Viewport,
        /// Legacy aliases kept for backwards compat — kept in sync with viewport.
        pub width: i32,
        pub height: i32,
        // Optional GPU context if available (kept for future extension).
        //
        // ONE field, not a `(DirectContext, SkiaGlContext)` pair. `GlDirectContext`
        // owns both halves, and its own field order already guarantees the
        // `DirectContext` is released while EGL is still alive. Holding the pair
        // here as two `Option`s made this struct's correctness rest on its OWN
        // declaration order matching a type in another file, with a comment
        // asking readers not to reorder them. `GlDirectContext` is the unit that
        // has to stay alive together; keeping it whole deletes the ordering
        // question instead of documenting it.
        pub(crate) _gpu: Option<crate::skia_gl::GlDirectContext>,
    }

    impl SkiaSurface {
        /// Create a CPU raster SkiaSurface.
        ///
        /// **This is deliberate — do not "optimize" it into a GPU context.**
        ///
        /// This is the CPU raster path (`raster_n32_premul`, below), and it is the
        /// *deterministic reference* the pixel proofs depend on. `velox-dom/tests/
        /// todo_item_pixels.rs` and `velox-renderer/tests/caret_pixels.rs` assert
        /// EXACT BYTE equality on rendered pixels; a GPU path with different
        /// antialiasing, subpixel positioning, or resource-cache-dependent output
        /// would silently invalidate ~949 tests that currently pass. Determinism was
        /// chosen over the paint speedup.
        ///
        /// There is a second, independent reason: the GPU path is not merely unused,
        /// it is *unsafe to adopt as-is*. A `GrDirectContext`'s resource cache
        /// defaults to **256 MB** and can occupy roughly twice that when full, and
        /// this crate sets no limit (`grep 'resource_cache|set_resource_limit'` → no
        /// hits). Any future GPU backend must set the limit explicitly and
        /// implement device-lost handling before it is enabled.
        ///
        /// The paint cost that remains on this path is **bandwidth, not allocation**
        /// — three full-surface touches per frame (`canvas.clear` in `skia_render.rs`,
        /// `read_pixels` into the reused `rgba` in `presenter.rs`, then a 1.92 MB
        /// `copy_from_slice` into winit's buffer). That is a damage-limited-repaint
        /// problem, not a buffer-reuse problem, and buffer reuse is already done.
        ///
        /// See `docs/RENDERING.md` for the full decision record.
        pub fn new_raster(width: i32, height: i32) -> Result<Self, String> {
            let viewport = Viewport::from_i32(width, height, 1.0);
            let w = viewport.physical_width_i32();
            let h = viewport.physical_height_i32();
            let surface = sk::surfaces::raster_n32_premul((w, h))
                .ok_or_else(|| "skia: failed to create raster surface".to_string())?;
            Ok(SkiaSurface {
                surface,
                viewport,
                width: w,
                height: h,
                _gpu: None,
            })
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
            #[cfg(all(feature = "skia-native", unix))]
            if let Some(gpu) = &self._gpu {
                let _ = gpu.gl().make_current();
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

        /// Encode the current surface snapshot as PNG bytes.
        pub fn encode_png(&mut self) -> Result<Vec<u8>, String> {
            let img = self.surface.image_snapshot();
            #[allow(deprecated)]
            let data = img
                .encode_to_data(skia_safe::EncodedImageFormat::PNG)
                .ok_or_else(|| "skia: failed to encode image".to_string())?;
            Ok(data.as_bytes().to_vec())
        }

        /// Present or flush any GPU work for this surface.
        ///
        /// For GPU-backed surfaces this will flush and submit the `DirectContext`.
        /// For raster surfaces this is a no-op.
        pub fn present(&mut self) -> Result<(), String> {
            #[cfg(all(feature = "skia-native", unix))]
            if let Some(gpu) = &self._gpu {
                let _ = gpu.gl().make_current();
            }
            if let Some(gpu) = &mut self._gpu {
                gpu.dctx_mut().flush_and_submit();
            }
            Ok(())
        }

        /// Read RGBA pixels from the surface into the provided buffer.
        pub fn read_pixels(
            &mut self,
            info: &sk::ImageInfo,
            dst: &mut [u8],
            row_bytes: usize,
            src: (i32, i32),
        ) -> bool {
            self.surface.read_pixels(info, dst, row_bytes, src)
        }

        /// Update the scale factor used for logical-to-physical mapping.
        pub fn set_scale_factor(&mut self, scale_factor: f32) {
            self.viewport.set_scale(scale_factor);
        }

        /// Current scale factor for logical-to-physical mapping.
        pub fn scale_factor(&self) -> f32 {
            self.viewport.scale
        }

        /// Resize the surface. Recreate a GPU-backed surface when a `DirectContext`
        /// is available; otherwise recreate a CPU raster surface.
        /// Clamps to max(1) before raster/glow, warns on error and keeps previous surface.
        pub fn resize(&mut self, width: i32, height: i32) -> Result<(), String> {
            let w = width.max(1);
            let h = height.max(1);

            // If we have a DirectContext, try to create a GPU-backed surface.
            if let Some(gpu) = &mut self._gpu {
                #[cfg(all(feature = "skia-native", unix))]
                {
                    let _ = gpu.gl().make_current();
                }
                if let Some(new_surf) = create_gpu_surface_from_direct_context(gpu.dctx_mut(), w, h)
                {
                    self.surface = new_surf;
                    self.viewport.set_physical_i32(w, h);
                    self.width = w;
                    self.height = h;
                    return Ok(());
                }
                // If GPU surface recreation failed, fall through to raster fallback.
            }

            // Raster fallback — attempt creation before mutating state.
            match sk::surfaces::raster_n32_premul((w, h)) {
                Some(surface) => {
                    self.surface = surface;
                    self.viewport.set_physical_i32(w, h);
                    self.width = w;
                    self.height = h;
                    Ok(())
                }
                None => {
                    log::warn!(
                        "SkiaSurface::resize failed to create raster surface {}x{} — keeping previous {}x{}",
                        w,
                        h,
                        self.width,
                        self.height
                    );
                    Err("skia: failed to create raster surface on resize".to_string())
                }
            }
        }
    }

    /// Attempt to create a window-backed Skia surface from a raw-window-handle.
    ///
    /// This function tries to create a native GL/EGL context for the provided
    /// `HasWindowHandle` and, if successful, will attempt to create a
    /// `DirectContext`. For Phase 1 we return a raster surface if creating a
    /// GPU-backed surface is not yet supported.
    pub fn create_window_surface_from_handle(
        window: &impl raw_window_handle::HasRawWindowHandle,
        width: i32,
        height: i32,
    ) -> Result<SkiaSurface, String> {
        let cw = width.max(1);
        let ch = height.max(1);
        // Try to create a native GL/EGL context using the helper in `skia_gl`.
        match crate::skia_gl::create_context_from_winit(window) {
            Ok(gl_ctx) => {
                // `gl_ctx` is *consumed*: the returned `GlDirectContext` owns both
                // the `DirectContext` and the EGL/GL context, so the two cannot be
                // separated and the `DirectContext` cannot outlive its GL objects.
                match gl_ctx.into_direct_context() {
                    Some(mut owned) => {
                        log::info!("DirectContext created (GPU path available)");
                        // Attempt to build a GPU-backed Skia surface from the DirectContext.
                        // If successful, return a SkiaSurface that owns both the DirectContext
                        // and the native GL context so they are kept alive for the lifetime
                        // of the surface.
                        if let Some(gpu_surf) =
                            create_gpu_surface_from_direct_context(owned.dctx_mut(), cw, ch)
                        {
                            let viewport = Viewport::from_i32(cw, ch, 1.0);
                            return Ok(SkiaSurface {
                                surface: gpu_surf,
                                viewport,
                                width: cw,
                                height: ch,
                                _gpu: Some(owned),
                            });
                        }
                        // Fallback to raster until the platform-specific path is implemented.
                        let viewport = Viewport::from_i32(cw, ch, 1.0);
                        let surface =
                            sk::surfaces::raster_n32_premul((cw, ch)).ok_or_else(|| {
                                "skia: failed to create raster fallback surface".to_string()
                            })?;
                        return Ok(SkiaSurface {
                            surface,
                            viewport,
                            width: cw,
                            height: ch,
                            _gpu: Some(owned),
                        });
                    }
                    None => {
                        log::warn!("Could not make DirectContext; falling back to raster");
                    }
                }
            }
            Err(e) => {
                log::error!("create_context_from_winit failed: {}", e);
            }
        }

        // Fallback to CPU raster surface (already clamps)
        SkiaSurface::new_raster(cw, ch)
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
        //
        // SAFETY: `from_loader_function` only stores the loader for later
        // symbol resolution, and `get_parameter_i32` reads the currently
        // bound FBO — both require a current GL context on this thread. The
        // sole caller (`create_window_surface_from_handle`, via
        // `create_gpu_surface_from_direct_context`) runs after
        // `into_direct_context` made the context current, so the context is
        // current here. A wrong FBO id would only mis-target the wrap (which
        // returns `None` and falls back to raster), never UB: no raw
        // pointers escape this block.
        unsafe {
            // Create a glow context loader using EGL's get_proc_address.
            let gl = glow::Context::from_loader_function(|s| egl::get_proc_address(s) as *const _);

            // Query the currently bound FBO.
            let fb_binding = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) as u32;

            // Choose a color format. We assume a standard RGBA8 buffer here.
            let gl_format = glow::RGBA8;

            let fb_info = sk::gpu::gl::FramebufferInfo {
                fboid: fb_binding,
                format: gl_format,
                protected: sk::gpu::Protected::No,
            };

            // Create a BackendRenderTarget for GL.
            let backend = sk::gpu::backend_render_targets::make_gl((width, height), 0, 8, fb_info);

            // Create a Skia GPU-backed Surface from the backend render target.
            let surface = sk::gpu::surfaces::wrap_backend_render_target(
                dctx,
                &backend,
                sk::gpu::SurfaceOrigin::BottomLeft,
                sk::ColorType::RGBA8888,
                None,
                None,
            );

            if surface.is_some() {
                log::info!("created GPU-backed Surface (fbo={})", fb_binding);
            } else {
                log::warn!("Surface::from_backend_render_target returned None");
            }

            surface
        }
    }
}

#[cfg(not(feature = "skia-native"))]
mod nostub {
    pub struct Surface {
        _private: (),
    }
    pub fn create_window_surface(_w: i32, _h: i32) -> Result<Surface, String> {
        Err("skia-native feature not enabled".into())
    }
    pub use Surface as SkiaSurface;
}

#[cfg(feature = "skia-native")]
pub use native::*;
#[cfg(not(feature = "skia-native"))]
pub use nostub::*;
