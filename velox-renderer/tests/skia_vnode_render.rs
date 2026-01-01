//! Integration test: render a simple VNode using the skia raster helper.
//!
//! Ignored by default since it requires `--features skia-native` and native libs.

#[cfg(all(feature = "skia-native", unix))]
#[test]
#[ignore]
fn render_simple_vnode_to_png() {
    use velox_dom::{h, text, VNode};
    use velox_style::Stylesheet;

    // Build a small vnode: a green background div with a text child.
    let vnode = h(
        "div",
        vec![("style", "background-color:#00FF88")],
        vec![text("Hello Skia")],
    );

    let png = match velox_renderer::render_vnode_to_raster_png(&vnode, &Stylesheet::default(), 320, 120) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };

    std::fs::create_dir_all("target").ok();
    std::fs::write("target/skia_vnode_render.png", &png).expect("failed to write png");
}
