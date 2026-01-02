//! Integration test: render a rounded rect with clipped child content and verify PNG checksum.
//!
//! Ignored by default since it requires `--features skia-native` and native libs.

#[cfg(all(feature = "skia-native", unix))]
#[test]
#[ignore]
fn render_border_radius_vnode_checksum() {
    use velox_dom::h;
    use velox_style::Stylesheet;

    let vnode = h(
        "div",
        vec![("style", "background-color:#FFFFFF;border-radius:12px")],
        vec![h("div", vec![("style", "background-color:#FF0000")], vec![])],
    );

    let png = match velox_renderer::render_vnode_to_raster_png(&vnode, &Stylesheet::default(), 64, 64) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };

    let checksum = fnv1a(&png);
    println!("border-radius checksum: 0x{checksum:08x}");
    // Update this checksum after regenerating the raster output.
    const EXPECTED_BORDER_RADIUS_CHECKSUM: u32 = 0x2e5b01ed;
    assert_eq!(checksum, EXPECTED_BORDER_RADIUS_CHECKSUM);
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
