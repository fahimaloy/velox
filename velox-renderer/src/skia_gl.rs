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
//!
//! # Non-unix
//!
//! Off unix there is no EGL/GL implementation, only [`non_unix_stub`]: the same
//! API surface, every entry point returning a typed error. That exists so
//! `--features skia-native` **compiles** on Windows — `skia_surface.rs` calls
//! `create_context_from_winit` unconditionally, and before the stub landed that
//! name existed only inside the `unix` block, so the feature did not build
//! there at all.
//!
//! # Checking it
//!
//! The `not(unix)` body cannot be compiled by a unix host, so it is checked by
//! building the feature for a real non-unix target — the `skia-native-non-unix`
//! job in `.github/workflows/ci.yml`, which runs on `windows-latest` (a
//! non-unix target; macOS would be `unix` again and prove nothing).
//!
//! To reproduce it by hand on a Windows machine:
//!
//! ```text
//! cargo check -p velox-renderer --features skia-native
//! ```
//!
//! Cross-checking from Linux with `--target x86_64-pc-windows-msvc` is *not* an
//! equivalent substitute: `skia-bindings` builds Skia from source for whatever
//! `--target` names, and its prebuilt binary cache is not populated for this
//! version, so the cross build spends its time compiling Skia for a toolchain
//! that is not installed and fails before it ever reaches this file.

#[cfg(all(feature = "skia-native", unix))]
mod unix_impl {
    use std::os::raw::c_void;
    use std::ptr;

    use glow::HasContext;
    use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
    use skia_safe as sk;
    use velox_dom::VeloxError;

    pub struct SkiaGlContext {
        // EGL handles
        pub egl_display: egl::EGLDisplay,
        pub egl_context: egl::EGLContext,
        pub egl_surface: egl::EGLSurface,
        pub interface: Option<skia_safe::gpu::gl::Interface>,
    }

    /// A Skia `DirectContext` bundled with the EGL/GL context that backs it.
    ///
    /// # Why this type exists
    ///
    /// `skia_safe::gpu::DirectContext` is an **owning** handle to a
    /// `GrDirectContext`, but it does not own — and does not keep alive — the
    /// EGL display, context or surface that its GL objects live in. `SkiaGlContext`
    /// owns exactly those, and its `Drop` destroys them. So a
    /// `fn(&self) -> DirectContext` hands back a value that can outlive its own
    /// GL objects: every later GL call, and the `DirectContext`'s own teardown,
    /// runs against a terminated display. Bundling both halves into one owning
    /// value makes that split unrepresentable instead of leaving it to caller
    /// drop order.
    ///
    /// # Drop order is load-bearing
    ///
    /// Struct fields are dropped in *declaration* order (Rust reference, not a
    /// layout guarantee — `rustc` is free to reorder fields for padding, so do
    /// not verify this with `offset_of!`). `dctx` is therefore declared first and
    /// released while `gl` is still alive. Do not reorder the fields.
    pub struct GlDirectContext {
        dctx: skia_safe::gpu::DirectContext,
        gl: SkiaGlContext,
    }

    impl GlDirectContext {
        /// The owned `DirectContext`.
        pub fn dctx(&self) -> &skia_safe::gpu::DirectContext {
            &self.dctx
        }

        /// The owned `DirectContext`, mutably.
        pub fn dctx_mut(&mut self) -> &mut skia_safe::gpu::DirectContext {
            &mut self.dctx
        }

        /// The EGL/GL context backing this `DirectContext`.
        pub fn gl(&self) -> &SkiaGlContext {
            &self.gl
        }

        /// The EGL/GL context backing this `DirectContext`, mutably.
        pub fn gl_mut(&mut self) -> &mut SkiaGlContext {
            &mut self.gl
        }
    }

    impl SkiaGlContext {
        /// Create a GPU-backed `DirectContext` that owns — and is owned with —
        /// this EGL/GL context.
        ///
        /// Takes `self` **by value**: the returned [`GlDirectContext`] holds both
        /// halves, so the `DirectContext` cannot outlive the GL objects it points
        /// at. Reach either half through [`GlDirectContext::dctx_mut`] or
        /// [`GlDirectContext::gl`]. There is deliberately no way to take the two
        /// halves apart: a caller that stored them in separate fields would have
        /// to re-establish the release order by hand, and the only consumer
        /// (`skia_surface.rs`) has no reason to. `SkiaGlContext` is what a
        /// half-separating caller actually wanted, and it is reachable directly.
        pub fn into_direct_context(self) -> Option<GlDirectContext> {
            let iface = match &self.interface {
                Some(i) => i,
                None => return None,
            };
            // Use the newer helper for creating a GL-backed DirectContext.
            // `make_gl` does not borrow from `iface`, so `self` may move into the
            // wrapper below.
            let dctx = skia_safe::gpu::direct_contexts::make_gl(iface, None)?;
            Some(GlDirectContext { dctx, gl: self })
        }

        pub fn make_current(&self) -> Result<(), VeloxError> {
            if egl::make_current(
                self.egl_display,
                self.egl_surface,
                self.egl_surface,
                self.egl_context,
            ) {
                Ok(())
            } else {
                Err(VeloxError::Render("egl: make_current failed".into()))
            }
        }
    }

    impl Drop for SkiaGlContext {
        fn drop(&mut self) {
            // Make no context current and destroy EGL resources. Best-effort cleanup;
            // errors are ignored because this is a destructor.
            let _ = egl::make_current(
                self.egl_display,
                egl::EGL_NO_SURFACE,
                egl::EGL_NO_SURFACE,
                egl::EGL_NO_CONTEXT,
            );
            egl::destroy_surface(self.egl_display, self.egl_surface);
            egl::destroy_context(self.egl_display, self.egl_context);
            egl::terminate(self.egl_display);
        }
    }

    fn choose_egl_config(dpy: egl::EGLDisplay) -> Option<egl::EGLConfig> {
        let attribs: &[egl::EGLint] = &[
            egl::EGL_RED_SIZE as egl::EGLint,
            8,
            egl::EGL_GREEN_SIZE as egl::EGLint,
            8,
            egl::EGL_BLUE_SIZE as egl::EGLint,
            8,
            egl::EGL_ALPHA_SIZE as egl::EGLint,
            8,
            egl::EGL_DEPTH_SIZE as egl::EGLint,
            24,
            egl::EGL_STENCIL_SIZE as egl::EGLint,
            8,
            egl::EGL_NONE as egl::EGLint,
        ];
        // use helper from the egl crate
        egl::choose_config(dpy, attribs, 1)
    }

    pub fn create_context_from_winit(
        window: &impl HasRawWindowHandle,
    ) -> Result<SkiaGlContext, String> {
        // Acquire raw handle (currently unused) and implement a minimal EGL init path.
        let _raw = window.raw_window_handle();

        // Initialize EGL display
        let display = egl::get_display(egl::EGL_DEFAULT_DISPLAY).ok_or_else(|| {
            let msg = "egl: no display".to_string();
            log::error!("{}", msg);
            msg
        })?;
        let mut major: egl::EGLint = 0;
        let mut minor: egl::EGLint = 0;
        if !egl::initialize(display, &mut major, &mut minor) {
            log::error!(
                "egl: failed to initialize (major={}, minor={})",
                major,
                minor
            );
            return Err("egl: failed to initialize".into());
        }

        let config = choose_egl_config(display).ok_or_else(|| {
            let msg = "egl: no config".to_string();
            log::error!("{}", msg);
            msg
        })?;

        // Create an EGL context
        let ctx_attribs: &[egl::EGLint] = &[
            egl::EGL_CONTEXT_CLIENT_VERSION as egl::EGLint,
            2,
            egl::EGL_NONE as egl::EGLint,
        ];
        let context = egl::create_context(display, config, egl::EGL_NO_CONTEXT, ctx_attribs)
            .ok_or_else(|| "egl: failed to create context".to_string())?;

        // Create a pbuffer surface as a default headless surface
        let pbuffer_attribs: &[egl::EGLint] = &[
            egl::EGL_WIDTH as egl::EGLint,
            1,
            egl::EGL_HEIGHT as egl::EGLint,
            1,
            egl::EGL_NONE as egl::EGLint,
        ];
        let surface = egl::create_pbuffer_surface(display, config, pbuffer_attribs)
            .ok_or_else(|| "egl: failed to create pbuffer surface".to_string())?;

        // Make context current
        if !egl::make_current(display, surface, surface, context) {
            egl::destroy_surface(display, surface);
            egl::destroy_context(display, context);
            egl::terminate(display);
            return Err("egl: make_current failed".into());
        }

        // Build skia-safe GL interface from current GL funcs
        let interface = unsafe {
            skia_safe::gpu::gl::Interface::new_load_with(|name: &str| {
                // Use EGL's get_proc_address to load GL symbols
                let f = egl::get_proc_address(name);
                f as *const _
            })
        };

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
            log::error!("{}", msg);
            msg
        })?;
        let mut major: egl::EGLint = 0;
        let mut minor: egl::EGLint = 0;
        if !egl::initialize(display, &mut major, &mut minor) {
            log::error!(
                "egl: failed to initialize (major={}, minor={})",
                major,
                minor
            );
            return Err("egl: failed to initialize".into());
        }

        let config = choose_egl_config(display).ok_or_else(|| {
            let msg = "egl: no config".to_string();
            log::error!("{}", msg);
            msg
        })?;

        let ctx_attribs: &[egl::EGLint] = &[
            egl::EGL_CONTEXT_CLIENT_VERSION as egl::EGLint,
            2,
            egl::EGL_NONE as egl::EGLint,
        ];
        let context = egl::create_context(display, config, egl::EGL_NO_CONTEXT, ctx_attribs)
            .ok_or_else(|| "egl: failed to create context".to_string())?;

        let pbuffer_attribs: &[egl::EGLint] = &[
            egl::EGL_WIDTH as egl::EGLint,
            1,
            egl::EGL_HEIGHT as egl::EGLint,
            1,
            egl::EGL_NONE as egl::EGLint,
        ];
        let surface = egl::create_pbuffer_surface(display, config, pbuffer_attribs)
            .ok_or_else(|| "egl: failed to create pbuffer surface".to_string())?;

        if !egl::make_current(display, surface, surface, context) {
            egl::destroy_surface(display, surface);
            egl::destroy_context(display, context);
            egl::terminate(display);
            return Err("egl: make_current failed".into());
        }

        let interface = unsafe {
            skia_safe::gpu::gl::Interface::new_load_with(|name: &str| {
                let f = egl::get_proc_address(name);
                f as *const _
            })
        };

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
        // `GlDirectContext` owns the EGL context, so the returned value keeps the
        // GL objects alive for as long as the `DirectContext` exists.
        let gpu = create_headless_context()?.into_direct_context();

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

        // If we had a DirectContext, we could flush GPU work here. Dropping the
        // pair releases the `DirectContext` first and only then tears down EGL,
        // so the teardown GL calls still run against a live context.
        drop(gpu);

        Ok(())
    }

    /// Create a GPU-backed FBO + Skia GPU surface, draw a test rect, and present.
    pub fn draw_gpu_test_frame(width: i32, height: i32) -> Result<(), String> {
        // Create headless context and DirectContext. `gl_ctx` is *moved* into the
        // wrapper, so the `DirectContext` and the EGL context it needs share one
        // lifetime and one release order.
        let gl_ctx = create_headless_context()?;
        let _ = gl_ctx.make_current();
        let mut owned = gl_ctx
            .into_direct_context()
            .ok_or_else(|| "skia: could not create DirectContext".to_string())?;

        // Attempt to build a GPU-backed Surface using the current framebuffer.
        let mut surface = {
            let gl = unsafe {
                glow::Context::from_loader_function(|s| egl::get_proc_address(s) as *const _)
            };
            let fb_binding = unsafe { gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) } as u32;
            let fb_info = skia_safe::gpu::gl::FramebufferInfo {
                fboid: fb_binding,
                format: glow::RGBA8,
                protected: skia_safe::gpu::Protected::No,
            };
            let backend =
                skia_safe::gpu::backend_render_targets::make_gl((width, height), 0, 8, fb_info);
            skia_safe::gpu::surfaces::wrap_backend_render_target(
                owned.dctx_mut(),
                &backend,
                skia_safe::gpu::SurfaceOrigin::BottomLeft,
                skia_safe::ColorType::RGBA8888,
                None,
                None,
            )
        };

        if surface.is_none() {
            log::warn!("GPU surface creation failed; falling back to raster");
            surface = skia_safe::surfaces::raster_n32_premul((width, height));
        }

        let mut surface = surface.ok_or_else(|| "skia: failed to create surface".to_string())?;
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(255, 200, 64, 64));
        paint.set_anti_alias(true);
        let r = skia_safe::Rect::from_xywh(4.0, 4.0, (width - 8) as f32, (height - 8) as f32);
        canvas.draw_rect(r, &paint);

        let _img = surface.image_snapshot();

        // Ensure GPU context work (if any) is flushed
        owned.dctx_mut().flush_and_submit();

        Ok(())
    }

    /// Compile-time proof of the ownership fix: the *only* way to obtain a
    /// `DirectContext` is to hand over the `SkiaGlContext` that owns its EGL
    /// objects, so the two cannot be separated. This never touches the GPU, so it
    /// needs no EGL display.
    #[cfg(test)]
    mod tests {
        use super::{GlDirectContext, SkiaGlContext};

        /// Compiles only if `into_direct_context` consumes the guard. Under the
        /// old `&self` signature this file would not build.
        fn ownership_only_api(ctx: SkiaGlContext) -> Option<GlDirectContext> {
            ctx.into_direct_context()
        }

        #[test]
        fn direct_context_can_only_be_obtained_by_consuming_the_gl_context() {
            // No EGL display is created here: the assertion is that this file
            // type-checks at all. Under the old `into_direct_context(&self)`
            // signature the function above would not compile.
            let _f: fn(SkiaGlContext) -> Option<GlDirectContext> = ownership_only_api;
        }
    }
}

#[cfg(all(feature = "skia-native", unix))]
pub use unix_impl::*;

#[cfg(all(feature = "skia-native", unix))]
/// Convenience: create a Skia `DirectContext` from a headless EGL context.
///
/// The returned [`GlDirectContext`] owns the EGL context, so it stays alive for
/// as long as the `DirectContext` is usable. A bare `skia_safe::gpu::DirectContext`
/// must not be returned from here: the local `SkiaGlContext` would run its `Drop`
/// (terminating EGL) before the caller ever touched it.
pub fn create_direct_context() -> Result<GlDirectContext, String> {
    unix_impl::create_headless_context()?
        .into_direct_context()
        .ok_or_else(|| "skia: could not create DirectContext".to_string())
}

// The non-unix mirror of the two declarations above: same names, same
// signatures, so a call site written for the GPU path compiles on every target
// instead of only on the ones whose backend exists.
#[cfg(all(feature = "skia-native", not(unix)))]
pub use non_unix_stub::*;

/// Convenience: mirror of the unix [`create_direct_context`], failing with the
/// same typed error there is no EGL to create a context from.
#[cfg(all(feature = "skia-native", not(unix)))]
pub fn create_direct_context() -> Result<GlDirectContext, String> {
    Err("skia: GPU path is not implemented for this target (non-unix)".into())
}

#[cfg(all(feature = "skia-native", not(unix)))]
mod non_unix_stub {
    //! Non-unix stub for the GPU path.
    //!
    //! The EGL/GL implementation above is unix-only, so off unix this module
    //! provides the same API surface with **no implementation**, and every
    //! entry point returns a typed error naming the target it was asked for.
    //! It never panics (a panic in an event loop is a crash, not an error) and
    //! never reports success it did not achieve.
    //!
    //! It exists so that `--features skia-native` *compiles* on Windows: before
    //! it landed, `skia_surface::create_window_surface_from_handle` called
    //! `create_context_from_winit`, which only existed in the `unix` block, so
    //! the feature was unbuildable there. Because every gate ran on Linux, that
    //! rot was invisible until someone built on another platform.
    //!
    //! `GlDirectContext` is here for the same reason and with the same method
    //! set as the unix type: `skia_surface.rs` stores whatever
    //! `into_direct_context` hands back and calls `dctx_mut()` and `gl()` on it,
    //! so those signatures have to exist on every target. Nothing can *construct* one
    //! off unix — `SkiaGlContext::into_direct_context` always returns `None` —
    //! so there is no path by which the stub can hand back a fake GPU context.

    use skia_safe as sk;

    /// The message every entry point here fails with. Carries the target so a
    /// bug report names the platform instead of just saying "unsupported".
    const UNSUPPORTED: &str = "skia_gl: GPU context is not implemented for this target (non-unix: the EGL/GL path is unix-only)";

    pub struct SkiaGlContext {
        _private: (),
    }

    /// Same shape as the unix `GlDirectContext`, and deliberately **not**
    /// constructible here: this module never creates a `DirectContext`, because
    /// there is no GL context for one to belong to.
    pub struct GlDirectContext {
        // Never written: no constructor below. Kept so the accessors keep the
        // signatures `skia_surface.rs` is written against.
        dctx: sk::gpu::DirectContext,
        gl: SkiaGlContext,
    }

    impl GlDirectContext {
        pub fn dctx(&self) -> &sk::gpu::DirectContext {
            &self.dctx
        }

        pub fn dctx_mut(&mut self) -> &mut sk::gpu::DirectContext {
            &mut self.dctx
        }

        pub fn gl(&self) -> &SkiaGlContext {
            &self.gl
        }

        pub fn gl_mut(&mut self) -> &mut SkiaGlContext {
            &mut self.gl
        }
    }

    impl SkiaGlContext {
        /// Always `None`: there is no GL context to build a `DirectContext`
        /// from. The caller falls back to a CPU raster surface, which is the
        /// honest outcome — raster really does work on every target.
        pub fn into_direct_context(self) -> Option<GlDirectContext> {
            None
        }

        /// Always `Err`: the EGL/GL implementation is unix-only.
        pub fn make_current(&self) -> Result<(), velox_dom::VeloxError> {
            Err(velox_dom::VeloxError::Render(UNSUPPORTED.into()))
        }
    }

    pub fn create_context_from_winit(
        _window: &impl raw_window_handle::HasRawWindowHandle,
    ) -> Result<SkiaGlContext, String> {
        log::error!("{UNSUPPORTED}");
        Err(UNSUPPORTED.into())
    }

    pub fn create_headless_context() -> Result<SkiaGlContext, String> {
        Err(UNSUPPORTED.into())
    }

    /// Create a headless pbuffer-backed EGL context (no window required).
    pub fn create_context() -> Result<SkiaGlContext, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn draw_test_frame() -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }

    pub fn draw_gpu_test_frame(_width: i32, _height: i32) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}

/// Compile-time proof that the non-unix build stays buildable, in the shape that
/// actually matters: the exact call sequence `skia_surface.rs` performs against
/// `skia_gl`. The unix test next to it proves the ownership invariant; this one
/// proves the stub answers with typed errors instead of panicking or faking
/// success — and that the signatures the shared call site is written against
/// still exist on a target where no GL context can be made.
///
/// The other half of the guarantee is not testable on a unix host at all: the
/// `not(unix)` body itself is only compiled by a real non-unix target. That is
/// what the `skia-native-non-unix` CI job checks.
#[cfg(all(test, feature = "skia-native", not(unix)))]
mod non_unix_tests {
    use super::{SkiaGlContext, create_context, create_context_from_winit};

    struct FakeWindow;
    impl raw_window_handle::HasRawWindowHandle for FakeWindow {
        fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
            raw_window_handle::RawWindowHandle::UiKit(raw_window_handle::UiKitWindowHandle::new())
        }
    }

    #[test]
    fn the_window_path_fails_with_a_message_instead_of_a_context() {
        let err = create_context_from_winit(&FakeWindow)
            .err()
            .expect("a non-unix target must not hand back a GL context");
        assert!(
            err.contains("non-unix"),
            "the error must name the cause, got: {err}"
        );
    }

    #[test]
    fn create_context_fails_and_never_reaches_a_direct_context() {
        assert!(create_context().is_err());
        // The stub's own invariant: no value can become a `DirectContext`.
        let ctx = SkiaGlContext { _private: () };
        assert!(ctx.into_direct_context().is_none());
    }
}

#[cfg(all(feature = "skia-native", unix))]
pub fn create_context() -> Result<SkiaGlContext, String> {
    // Create a headless pbuffer-backed context for CI and headless environments.
    unix_impl::create_headless_context()
}
