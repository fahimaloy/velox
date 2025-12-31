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

    pub struct SkiaGlContext {
        // EGL handles
        pub egl_display: egl::EGLDisplay,
        pub egl_context: egl::EGLContext,
        pub egl_surface: egl::EGLSurface,
        pub interface: Option<skia_safe::gpu::gl::Interface>,
    }

    impl SkiaGlContext {
        pub fn into_direct_context(self) -> Option<skia_safe::gpu::DirectContext> {
            let iface = match self.interface {
                Some(i) => i,
                None => return None,
            };
            // Use the newer helper for creating a GL-backed DirectContext
            skia_safe::gpu::direct_contexts::make_gl(&iface, None)
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
        let display = egl::get_display(egl::EGL_DEFAULT_DISPLAY).ok_or_else(|| "egl: no display".to_string())?;
        let mut major: egl::EGLint = 0;
        let mut minor: egl::EGLint = 0;
        if !egl::initialize(display, &mut major, &mut minor) {
            return Err("egl: failed to initialize".into());
        }

        let config = choose_egl_config(display).ok_or_else(|| "egl: no config".to_string())?;

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
        let display = egl::get_display(egl::EGL_DEFAULT_DISPLAY).ok_or_else(|| "egl: no display".to_string())?;
        let mut major: egl::EGLint = 0;
        let mut minor: egl::EGLint = 0;
        if !egl::initialize(display, &mut major, &mut minor) {
            return Err("egl: failed to initialize".into());
        }

        let config = choose_egl_config(display).ok_or_else(|| "egl: no config".to_string())?;

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

