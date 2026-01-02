//! Integration test: render with DPR=2 and verify PNG checksum.
//!
//! Ignored by default since it requires `--features skia-native` and native libs.

#[cfg(all(feature = "skia-native", unix))]
#[test]
#[ignore]
fn render_dpr2_checksum() {
    use velox_dom::{h, text};
    use velox_style::Stylesheet;

    let vnode = h(
        "div",
        vec![("style", "background-color:#FFFFFF;border:1px solid #000000;width:64px;height:32px")],
        vec![text("Hi")],
    );

    let png = match velox_renderer::render_vnode_to_raster_png_with_scale(
        &vnode,
        &Stylesheet::default(),
        64,
        32,
        2.0,
    ) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };

    let checksum = fnv1a(&png);
    println!("dpr2 checksum: 0x{checksum:08x}");
    // Update this checksum after regenerating the raster output.
    const EXPECTED_DPR_CHECKSUM: u32 = 0x9659a433;
    assert_eq!(checksum, EXPECTED_DPR_CHECKSUM);
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
