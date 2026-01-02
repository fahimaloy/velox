//! Integration test: render many boxes and verify PNG checksum.
//!
//! Ignored by default since it requires `--features skia-native` and native libs.

#[cfg(all(feature = "skia-native", unix))]
#[test]
#[ignore]
fn render_many_boxes_checksum() {
    use velox_dom::h;
    use velox_style::Stylesheet;

    let mut children = Vec::new();
    for i in 0..120 {
        let color = if i % 2 == 0 { "#44AA44" } else { "#4444AA" };
        let style = format!("background-color:{};width:6px;height:6px", color);
        children.push(h("div", vec![("style", style.as_str())], vec![]));
    }
    let vnode = h("div", vec![("style", "width:128px;height:128px")], children);

    let png = match velox_renderer::render_vnode_to_raster_png(&vnode, &Stylesheet::default(), 128, 128) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };

    let checksum = fnv1a(&png);
    println!("batching checksum: 0x{checksum:08x}");
    // Update this checksum after regenerating the raster output.
    const EXPECTED_BATCHING_CHECKSUM: u32 = 0x4315cbb4;
    assert_eq!(checksum, EXPECTED_BATCHING_CHECKSUM);
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
