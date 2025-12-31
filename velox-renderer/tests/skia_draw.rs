#[cfg(feature = "skia-native")]
#[test]
#[ignore]
fn smoke_skia_draw() {
    if let Err(e) = velox_renderer::skia_draw_test_frame() {
        panic!("skia draw test failed: {}", e);
    }
}
