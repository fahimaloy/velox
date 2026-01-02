//! Integration test: render hover styles and verify pixel changes.
//!
//! Ignored by default since it requires `--features skia-native` and native libs.

#[cfg(all(feature = "skia-native", unix))]
#[test]
#[ignore]
fn render_hover_styles_checksum() {
    use velox_dom::h;
    use velox_style::{apply_styles_with_hover, Stylesheet};

    let vnode = h(
        "div",
        vec![("class", "btn"), ("style", "background-color:#00FF00;width:60px;height:24px")],
        vec![],
    );
    let sheet = Stylesheet::parse(".btn:hover { background-color: #FF0000; }");

    let vnode_normal = apply_styles_with_hover(&vnode, &sheet, &|_, _| false);
    let vnode_hovered = apply_styles_with_hover(&vnode, &sheet, &|tag, props| {
        velox_renderer::events::is_hoverable(tag, props)
    });

    let png_normal = match velox_renderer::render_vnode_to_raster_png(
        &vnode_normal,
        &sheet,
        64,
        32,
    ) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };
    let png_hovered = match velox_renderer::render_vnode_to_raster_png(
        &vnode_hovered,
        &sheet,
        64,
        32,
    ) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };

    let checksum_normal = fnv1a(&png_normal);
    let checksum_hovered = fnv1a(&png_hovered);
    println!("hover normal checksum: 0x{checksum_normal:08x}");
    println!("hovered checksum: 0x{checksum_hovered:08x}");
    assert_ne!(checksum_normal, checksum_hovered);
    // Update this checksum after regenerating the raster output.
    const EXPECTED_HOVER_CHECKSUM: u32 = 0x14abb14a;
    assert_eq!(checksum_hovered, EXPECTED_HOVER_CHECKSUM);
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
