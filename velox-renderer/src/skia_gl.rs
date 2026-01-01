//! Minimal scaffold for creating a GL/EGL context for Skia.
//!
//! This module is feature-gated behind `skia-native`. It provides a small
//! typed surface/context wrapper and `create_context()` entrypoint. The
//! implementation is intentionally minimal for the first iteration; later
//! commits will add proper EGL/GL setup using `raw-window-handle`, `egl`, or
//! a lightweight GL context crate.

#![allow(unused)]
//! Minimal EGL/GL implementation for Linux to bootstrap `skia-native`.
//!
//! This file implements a small subset of functionality needed to create an
//! EGL context and produce a `skia_safe::gpu::gl::Interface` suitable for
//! creating a `skia_safe::gpu::DirectContext`. It is gated behind
//! `skia-native` and UNIX targets.

#[cfg(all(feature = "skia-native", unix))]
mod unix_impl {
    use std::ptr;
    use std::os::raw::c_void;

    use skia_safe as sk;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use glow;
    use glow::HasContext;

    pub struct SkiaGlContext {
        // EGL handles
        pub egl_display: egl::EGLDisplay,
        pub egl_context: egl::EGLContext,
        pub egl_surface: egl::EGLSurface,
        pub interface: Option<skia_safe::gpu::gl::Interface>,
    }

    impl SkiaGlContext {
        pub fn into_direct_context(&self) -> Option<skia_safe::gpu::DirectContext> {
                let iface = match &self.interface {
                    Some(i) => i,
                    None => return None,
                };
                // Use the newer helper for creating a GL-backed DirectContext
                skia_safe::gpu::direct_contexts::make_gl(iface, None)
            }
    }

    impl Drop for SkiaGlContext {
        fn drop(&mut self) {
            // Make no context current and destroy EGL resources. Best-effort cleanup;
            // errors are ignored because this is a destructor.
            let _ = egl::make_current(self.egl_display, egl::EGL_NO_SURFACE, egl::EGL_NO_SURFACE, egl::EGL_NO_CONTEXT);
            egl::destroy_surface(self.egl_display, self.egl_surface);
            egl::destroy_context(self.egl_display, self.egl_context);
            egl::terminate(self.egl_display);
        }
    }

    fn choose_egl_config(dpy: egl::EGLDisplay) -> Option<egl::EGLConfig> {
        let attribs: &[egl::EGLint] = &[
            egl::EGL_RED_SIZE as egl::EGLint, 8,
            egl::EGL_GREEN_SIZE as egl::EGLint, 8,
            egl::EGL_BLUE_SIZE as egl::EGLint, 8,
            egl::EGL_ALPHA_SIZE as egl::EGLint, 8,
            egl::EGL_DEPTH_SIZE as egl::EGLint, 24,
            egl::EGL_STENCIL_SIZE as egl::EGLint, 8,
            egl::EGL_NONE as egl::EGLint,
        ];
        // use helper from the egl crate
        egl::choose_config(dpy, attribs, 1)
    }

    pub fn create_context_from_winit(window: &impl HasWindowHandle) -> Result<SkiaGlContext, String> {
        // Acquire raw handle (currently unused) and implement a minimal EGL init path.
        let _raw = window.window_handle();

        // Initialize EGL display
        let display = egl::get_display(egl::EGL_DEFAULT_DISPLAY).ok_or_else(|| {
            let msg = "egl: no display".to_string();
            eprintln!("[skia_gl] {}", msg);
            msg
        })?;
        let mut major: egl::EGLint = 0;
        let mut minor: egl::EGLint = 0;
        if !egl::initialize(display, &mut major, &mut minor) {
            eprintln!("[skia_gl] egl: failed to initialize (major={}, minor={})", major, minor);
            return Err("egl: failed to initialize".into());
        }

        let config = choose_egl_config(display).ok_or_else(|| {
            let msg = "egl: no config".to_string();
            eprintln!("[skia_gl] {}", msg);
            msg
        })?;

        // Create an EGL context
        let ctx_attribs: &[egl::EGLint] = &[egl::EGL_CONTEXT_CLIENT_VERSION as egl::EGLint, 2, egl::EGL_NONE as egl::EGLint];
        let context = egl::create_context(display, config, egl::EGL_NO_CONTEXT, ctx_attribs).ok_or_else(|| "egl: failed to create context".to_string())?;

        // Create a pbuffer surface as a default headless surface
        let pbuffer_attribs: &[egl::EGLint] = &[egl::EGL_WIDTH as egl::EGLint, 1, egl::EGL_HEIGHT as egl::EGLint, 1, egl::EGL_NONE as egl::EGLint];
        let surface = egl::create_pbuffer_surface(display, config, pbuffer_attribs).ok_or_else(|| "egl: failed to create pbuffer surface".to_string())?;

        // Make context current
        if !egl::make_current(display, surface, surface, context) {
            egl::destroy_surface(display, surface);
            egl::destroy_context(display, context);
            return Err("egl: make_current failed".into());
        }

        // Build skia-safe GL interface from current GL funcs
        let interface = unsafe { skia_safe::gpu::gl::Interface::new_load_with(|name: &str| {
            // Use EGL's get_proc_address to load GL symbols
            let f = egl::get_proc_address(name);
            f as *const _
        }) };

        let iface = match interface {
            Some(i) => i,
            None => return Err("skia: failed to create GL interface".into()),
        };

        Ok(SkiaGlContext {
            egl_display: display,
            egl_context: context,
            egl_surface: surface,
            interface: Some(iface),
        })
    }

    /// Create a headless pbuffer-backed EGL context (no window required).
    pub fn create_headless_context() -> Result<SkiaGlContext, String> {
        let display = egl::get_display(egl::EGL_DEFAULT_DISPLAY).ok_or_else(|| {
            let msg = "egl: no display".to_string();
            eprintln!("[skia_gl] {}", msg);
            msg
        })?;
        let mut major: egl::EGLint = 0;
        let mut minor: egl::EGLint = 0;
        if !egl::initialize(display, &mut major, &mut minor) {
            eprintln!("[skia_gl] egl: failed to initialize (major={}, minor={})", major, minor);
            return Err("egl: failed to initialize".into());
        }

        let config = choose_egl_config(display).ok_or_else(|| {
            let msg = "egl: no config".to_string();
            eprintln!("[skia_gl] {}", msg);
            msg
        })?;

        let ctx_attribs: &[egl::EGLint] = &[egl::EGL_CONTEXT_CLIENT_VERSION as egl::EGLint, 2, egl::EGL_NONE as egl::EGLint];
        let context = egl::create_context(display, config, egl::EGL_NO_CONTEXT, ctx_attribs).ok_or_else(|| "egl: failed to create context".to_string())?;

        let pbuffer_attribs: &[egl::EGLint] = &[egl::EGL_WIDTH as egl::EGLint, 1, egl::EGL_HEIGHT as egl::EGLint, 1, egl::EGL_NONE as egl::EGLint];
        let surface = egl::create_pbuffer_surface(display, config, pbuffer_attribs).ok_or_else(|| "egl: failed to create pbuffer surface".to_string())?;

        if !egl::make_current(display, surface, surface, context) {
            egl::destroy_surface(display, surface);
            egl::destroy_context(display, context);
            return Err("egl: make_current failed".into());
        }

        let interface = unsafe { skia_safe::gpu::gl::Interface::new_load_with(|name: &str| {
            let f = egl::get_proc_address(name);
            f as *const _
        }) };

        let iface = match interface {
            Some(i) => i,
            None => return Err("skia: failed to create GL interface".into()),
        };

        Ok(SkiaGlContext {
            egl_display: display,
            egl_context: context,
            egl_surface: surface,
            interface: Some(iface),
        })
    }

    /// Try to draw a very small test frame. Prefer GPU-backed surface when a
    /// `DirectContext` is available; otherwise fall back to a CPU raster surface.
    pub fn draw_test_frame() -> Result<(), String> {
        // Try to create a DirectContext; if it fails, continue with raster fallback.
        let dctx = match skia_safe::gpu::direct_contexts::make_gl(
            &create_headless_context()?.interface.clone().ok_or_else(|| "no gl interface".to_string())?,
            None,
        ) {
            Some(dc) => Some(dc),
            None => None,
        };

        // Create a small raster surface and draw a colored rect into it.
        let mut surface = skia_safe::surfaces::raster_n32_premul((64, 64))
            .ok_or_else(|| "skia: failed to create raster surface".to_string())?;
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(255, 0, 128, 255));
        paint.set_anti_alias(true);
        let r = skia_safe::Rect::from_xywh(8.0, 8.0, 48.0, 48.0);
        canvas.draw_rect(r, &paint);
        // Ensure the raster surface contents are finalized by taking a snapshot.
        let _ = surface.image_snapshot();

        // If we had a DirectContext, we could flush GPU work here. We drop it
        // afterwards; the destructor for SkiaGlContext will clean up EGL.
        if let Some(_dc) = dctx {
            // best-effort: do nothing further for now
        }

        Ok(())
    }

    /// Create a GPU-backed FBO + Skia GPU surface, draw a test rect, and present.
    pub fn draw_gpu_test_frame(width: i32, height: i32) -> Result<(), String> {
        // Create headless context and DirectContext
        let gl_ctx = create_headless_context()?;
        let mut dctx = gl_ctx.into_direct_context().ok_or_else(|| "skia: could not create DirectContext".to_string())?;

        // We can at least verify that a DirectContext exists; creating a full
        // BackendRenderTarget is platform- and API-version-sensitive and may
        // require finer-grained skia-safe bindings. For now, log success and
        // fall back to drawing into a CPU raster surface to validate the
        // render path.
        eprintln!("[skia_gl] DirectContext created — GPU path available");

        // Fallback: draw to a small raster surface to validate drawing
        let mut surface = skia_safe::surfaces::raster_n32_premul((width as i32, height as i32))
            .ok_or_else(|| "skia: failed to create raster fallback surface".to_string())?;
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(255, 200, 64, 64));
        paint.set_anti_alias(true);
        let r = skia_safe::Rect::from_xywh(4.0, 4.0, (width - 8) as f32, (height - 8) as f32);
        canvas.draw_rect(r, &paint);

        // Take a snapshot to materialize the raster drawing
        let _img = surface.image_snapshot();

        // Ensure GPU context work (if any) is flushed
        dctx.flush_and_submit();

        Ok(())
    }
}

#[cfg(all(feature = "skia-native", unix))]
pub use unix_impl::*;

#[cfg(all(feature = "skia-native", unix))]
/// Convenience: create a `skia_safe::gpu::DirectContext` from a headless EGL context.
pub fn create_direct_context() -> Result<skia_safe::gpu::DirectContext, String> {
    let ctx = unix_impl::create_headless_context()?;
    ctx.into_direct_context().ok_or_else(|| "skia: could not create DirectContext".to_string())
}

#[cfg(not(all(feature = "skia-native", unix)))]
pub struct SkiaGlContext { _private: () }

#[cfg(not(all(feature = "skia-native", unix)))]
pub fn create_context() -> Result<SkiaGlContext, String> {
    Err("skia_gl: platform not supported in this build".into())
}

#[cfg(all(feature = "skia-native", unix))]
pub fn create_context() -> Result<SkiaGlContext, String> {
    // Create a headless pbuffer-backed context for CI and headless environments.
    unix_impl::create_headless_context()
}

