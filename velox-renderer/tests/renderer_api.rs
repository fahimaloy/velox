#[test]
fn selected_renderer_matches_backend_const() {
    let r = velox_renderer::new_selected_renderer();
    assert_eq!(r.backend_name(), velox_renderer::backend_name());
}

#[cfg(feature = "wgpu")]
#[test]
fn selected_renderer_is_wgpu() {
    let r = velox_renderer::new_selected_renderer();
    assert_eq!(r.backend_name(), "wgpu");
}
use velox_renderer::Renderer;
