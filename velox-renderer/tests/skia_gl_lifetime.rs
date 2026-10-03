//! Regression coverage for the `GlDirectContext` ownership fix (task A1).
//!
//! `SkiaGlContext::into_direct_context` used to take `&self` and return an
//! **owning** `skia_safe::gpu::DirectContext`, while `Drop for SkiaGlContext`
//! destroys the EGL display, context and surface. A caller that kept the
//! returned value — which is exactly what `velox_renderer::create_direct_context`
//! did — ended up holding a `DirectContext` whose GL objects were already gone:
//! every GL call, and the `DirectContext`'s own teardown, ran against a
//! terminated display. Both halves now travel together in one owning value, so
//! the split is unrepresentable rather than a caller convention.
//!
//! These tests go through the public crate-root entry point, which now returns
//! the owning pair. They need a real EGL display and GL driver, so they are
//! ignored by default (same convention as `tests/skia_init.rs`).

#[cfg(all(feature = "skia-native", unix))]
mod gpu {
    use velox_renderer::create_direct_context;

    /// The EGL context must still be alive when GL work is issued through the
    /// returned value. Before the fix this first `flush_and_submit` was a
    /// use-after-free: the crate root returned a bare `DirectContext` only after
    /// the `SkiaGlContext` backing it had run its `Drop`.
    #[test]
    #[ignore = "requires skia-native feature and GPU hardware"]
    fn direct_context_outlives_the_egl_context_that_backs_it() {
        let mut owned = create_direct_context().expect("create_direct_context");
        owned.dctx_mut().flush_and_submit();
        owned.dctx_mut().flush_and_submit();
        // Drops the `DirectContext` first, then the EGL context.
        drop(owned);
    }

    /// Repeated create/drop must be clean. This is the shape that used to
    /// terminate EGL while a `DirectContext` was still queued for teardown.
    #[test]
    #[ignore = "requires skia-native feature and GPU hardware"]
    fn repeated_create_and_drop_is_clean() {
        for _ in 0..3 {
            let owned = create_direct_context().expect("create_direct_context");
            drop(owned);
        }
    }
}
