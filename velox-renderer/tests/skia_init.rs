#[cfg(feature = "skia-native")]
#[test]
#[ignore]
fn smoke_skia_init() {
    // Ignored by default — requires EGL/native libs available (CI or local).
    match velox_renderer::create_direct_context() {
        Ok(_dctx) => (),
        Err(e) => panic!("skia init failed: {}", e),
    }
}
