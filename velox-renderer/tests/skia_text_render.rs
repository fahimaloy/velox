//! Integration test: render text color, alignment, and underline and verify PNG checksum.
//!
//! Ignored by default since it requires `--features skia-native` and native libs.

#[cfg(all(feature = "skia-native", unix))]
#[test]
#[ignore]
fn render_text_styles_checksum() {
    use velox_dom::{h, text};
    use velox_style::Stylesheet;

    let vnode = h(
        "div",
        vec![(
            "style",
            "background-color:#FFFFFF;text-align:center;color:#00FF00;text-decoration:underline",
        )],
        vec![text("Hello")],
    );

    let png = match velox_renderer::render_vnode_to_raster_png(&vnode, &Stylesheet::default(), 80, 32) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };

    let checksum = fnv1a(&png);
    println!("text checksum: 0x{checksum:08x}");
    // Update this checksum after regenerating the raster output.
    const EXPECTED_TEXT_CHECKSUM: u32 = 0xdbcd3a71;
    assert_eq!(checksum, EXPECTED_TEXT_CHECKSUM);
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
