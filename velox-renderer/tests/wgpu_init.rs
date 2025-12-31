#[cfg(feature = "wgpu")]
#[test]
#[ignore]
fn smoke_wgpu_init() {
    // Ignored by default because CI or dev machines may not have a usable GPU/Vulkan environment.
    // Run locally with `cargo test -p velox-renderer -- --ignored` when you have a headless GPU environment.
    velox_renderer::wgpu_backend::init();
}
