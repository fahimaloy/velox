//! Integration test: render wrapped text and verify PNG checksum.
//!
//! Ignored by default since it requires `--features skia-native` and native libs.

#[cfg(all(feature = "skia-native", unix))]
#[test]
#[ignore]
fn render_wrapped_text_checksum() {
    use velox_dom::{h, text};
    use velox_style::Stylesheet;

    let vnode = h(
        "div",
        vec![("style", "background-color:#FFFFFF;width:80px;height:64px")],
        vec![text("Hello from the Velox renderer")],
    );

    let png = match velox_renderer::render_vnode_to_raster_png(&vnode, &Stylesheet::default(), 96, 64) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };

    let checksum = fnv1a(&png);
    println!("wrapped text checksum: 0x{checksum:08x}");
    // Update this checksum after regenerating the raster output.
    const EXPECTED_TEXT_WRAP_CHECKSUM: u32 = 0x6146e6d8;
    assert_eq!(checksum, EXPECTED_TEXT_WRAP_CHECKSUM);
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
