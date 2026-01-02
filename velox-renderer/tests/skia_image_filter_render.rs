//! Integration test: render image with opacity and filters and verify PNG checksum.
//!
//! Ignored by default since it requires `--features skia-native` and native libs.

#[cfg(all(feature = "skia-native", unix))]
use skia_safe as sk;

#[cfg(all(feature = "skia-native", unix))]
#[test]
#[ignore]
fn render_image_filters_checksum() {
    use velox_dom::h;
    use velox_style::Stylesheet;

    let image_path = "target/skia_test_image.png";
    std::fs::create_dir_all("target").ok();
    create_test_image(image_path);

    let vnode = h(
        "img",
        vec![
            ("src", image_path),
            ("style", "width:16px;height:16px;opacity:0.5;filter:blur(1px) brightness(1.2)"),
        ],
        vec![],
    );

    let png = match velox_renderer::render_vnode_to_raster_png(&vnode, &Stylesheet::default(), 32, 32) {
        Ok(b) => b,
        Err(e) => panic!("render failed: {}", e),
    };

    let checksum = fnv1a(&png);
    println!("image filter checksum: 0x{checksum:08x}");
    // Update this checksum after regenerating the raster output.
    const EXPECTED_IMAGE_FILTER_CHECKSUM: u32 = 0xc2e1032a;
    assert_eq!(checksum, EXPECTED_IMAGE_FILTER_CHECKSUM);
}

#[cfg(all(feature = "skia-native", unix))]
fn create_test_image(path: &str) {
    let mut surface = sk::surfaces::raster_n32_premul((16, 16)).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(sk::Color::WHITE);
    let mut paint = sk::Paint::default();
    paint.set_color(sk::Color::from_argb(255, 64, 128, 255));
    paint.set_anti_alias(true);
    let rect = sk::Rect::from_xywh(2.0, 2.0, 12.0, 12.0);
    canvas.draw_rect(rect, &paint);

    let image = surface.image_snapshot();
    #[allow(deprecated)]
    let data = image
        .encode_to_data(skia_safe::EncodedImageFormat::PNG)
        .expect("encode png");
    std::fs::write(path, data.as_bytes()).expect("write png");
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}
