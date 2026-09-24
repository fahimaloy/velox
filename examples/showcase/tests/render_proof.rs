//! Headless render proof for the showcase example.
//!
//! Rasterizes the compiled `App.vx` tree on the CPU (no window, no compositor)
//! and writes proof PNGs to `target/velox-render-proof/showcase-<W>x<H>.png`.
//! The assertions double as a parity gallery check: spacing/flex fills, the
//! `margin: 0 auto` centered block, and the scroll container are all painted.

use std::sync::Arc;

use velox_renderer::{render_vnode_to_raster_png_with_scale, render_vnode_to_rgba};
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/app.rs"));

/// Colors declared in `src/App.vx`.
const SECTION_BG: [u8; 3] = [30, 41, 59]; // .section
const CELL_BG: [u8; 3] = [51, 65, 85]; // .cell / .wrap-cell / .scroll-item
const CENTERED_BG: [u8; 3] = [167, 139, 250]; // .centered
const PAD_A_BG: [u8; 3] = [56, 189, 248]; // .box-a
const PAD_B_BG: [u8; 3] = [74, 222, 128]; // .box-b
const PAD_C_BG: [u8; 3] = [251, 191, 36]; // .box-c

const LARGE: (i32, i32) = (1280, 800);
const SMALL: (i32, i32) = (480, 360);

fn proof_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("velox-render-proof");
    std::fs::create_dir_all(&dir).expect("create proof dir");
    dir
}

fn render(state: &Arc<app::script_rs::State>, width: i32, height: i32) -> (Vec<u8>, Vec<u8>) {
    let vnode = app::render_with_state(Arc::clone(state), app::make_resolve(Arc::clone(state)));
    let sheet = Stylesheet::parse(app::STYLE);
    let png = render_vnode_to_raster_png_with_scale(&vnode, &sheet, width, height, 1.0)
        .expect("raster png");
    let rgba = render_vnode_to_rgba(&vnode, &sheet, width, height).expect("raster rgba");
    (png, rgba)
}

fn write_proof(name: &str, width: i32, height: i32, png: &[u8]) -> std::path::PathBuf {
    let path = proof_dir().join(format!("{name}-{width}x{height}.png"));
    std::fs::write(&path, png).expect("write proof png");
    path
}

fn pixels_near(rgba: &[u8], color: [u8; 3], tolerance: i32) -> usize {
    rgba.chunks_exact(4)
        .filter(|px| {
            (px[0] as i32 - color[0] as i32).abs() <= tolerance
                && (px[1] as i32 - color[1] as i32).abs() <= tolerance
                && (px[2] as i32 - color[2] as i32).abs() <= tolerance
        })
        .count()
}

fn distinct_colors(rgba: &[u8]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for px in rgba.chunks_exact(4) {
        seen.insert((px[0] / 16, px[1] / 16, px[2] / 16));
    }
    seen.len()
}

fn assert_non_trivial(name: &str, png: &[u8], rgba: &[u8]) {
    assert!(
        png.len() > 1000,
        "{name}: png suspiciously small ({} bytes)",
        png.len()
    );
    assert!(
        distinct_colors(rgba) > 4,
        "{name}: render is flat ({} distinct colors)",
        distinct_colors(rgba)
    );
}

#[test]
fn renders_large_viewport_proof_png() {
    let state = Arc::new(app::script_rs::State::new());
    let (png, rgba) = render(&state, LARGE.0, LARGE.1);
    assert_non_trivial("showcase-large", &png, &rgba);
    // Spacing cards, flex cells, the auto-margin centered block and the
    // scroll/wrap sections all paint their own fills.
    assert!(pixels_near(&rgba, SECTION_BG, 8) > 2000, "section cards");
    assert!(pixels_near(&rgba, CELL_BG, 8) > 2000, "flex cells");
    assert!(pixels_near(&rgba, CENTERED_BG, 12) > 500, "centered block");
    assert!(pixels_near(&rgba, PAD_A_BG, 12) > 200, "padding card A");
    assert!(pixels_near(&rgba, PAD_B_BG, 12) > 200, "padding card B");
    assert!(pixels_near(&rgba, PAD_C_BG, 12) > 100, "padding card C");
    let path = write_proof("showcase", LARGE.0, LARGE.1, &png);
    assert!(path.exists(), "proof png missing: {}", path.display());
}

#[test]
fn small_viewport_keeps_the_gallery_visible() {
    let state = Arc::new(app::script_rs::State::new());
    let (png, rgba) = render(&state, SMALL.0, SMALL.1);
    assert_non_trivial("showcase-small", &png, &rgba);
    write_proof("showcase", SMALL.0, SMALL.1, &png);

    // The centered block is clamped by max-width, so it stays fully inside a
    // 480px-wide viewport instead of overflowing.
    let width = SMALL.0 as usize;
    let centered = pixels_near(&rgba, CENTERED_BG, 12);
    assert!(centered > 200, "centered block missing at 480x360");
    let rows_with_centered = (0..SMALL.1 as usize)
        .filter(|y| {
            let start = y * width * 4;
            pixels_near(&rgba[start..start + width * 4], CENTERED_BG, 12) > 0
        })
        .count();
    assert!(rows_with_centered > 10, "centered block is squashed");
    assert!(
        rows_with_centered < SMALL.1 as usize,
        "centered block overflows the viewport"
    );
}
