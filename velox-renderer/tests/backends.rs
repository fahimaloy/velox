#[cfg(feature = "wgpu")]
#[test]
fn wgpu_backend_reports_name() {
    assert_eq!(velox_renderer::backend_name(), "wgpu");
}

#[cfg(feature = "skia")]
#[test]
fn skia_backend_reports_name() {
    assert_eq!(velox_renderer::backend_name(), "skia");
}

