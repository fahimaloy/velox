// Ignored by default; exercise the headless GPU path if available.
#![cfg(all(feature = "skia-native", unix))]

#[test]
#[ignore]
fn gpu_surface_present() {
    // Try to draw a GPU test frame (headless). This function will create
    // a headless EGL context and a DirectContext; it currently falls back
    // to raster drawing if a true GPU-backed surface can't be created.
    match velox_renderer::skia_draw_test_frame() {
        Ok(()) => {
            eprintln!("skia_gpu_surface: draw_gpu_test_frame succeeded");
        }
        Err(e) => {
            eprintln!("skia_gpu_surface: draw_gpu_test_frame failed: {}", e);
            panic!("GPU surface test failed: {}", e);
        }
    }
}
