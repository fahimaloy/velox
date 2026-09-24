//! Headless render proof for the counter example.
//!
//! `render_vnode_to_raster_png_with_scale` rasterizes the compiled `App.vx` tree on the
//! CPU (no window, no compositor), so this test is the repeatable way to prove
//! that the example paints real content at real viewport sizes. Proof PNGs are
//! written to `target/velox-render-proof/counter-<W>x<H>.png`.

use std::sync::Arc;

use velox_renderer::{render_vnode_to_raster_png_with_scale, render_vnode_to_rgba};
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/app.rs"));

/// Colors declared in `src/App.vx`, used as pixel probes.
const COUNT_FG: [u8; 3] = [56, 189, 248]; // .count
const INC_BG: [u8; 3] = [74, 222, 128]; // .btn.inc
const DEC_BG: [u8; 3] = [251, 191, 36]; // .btn.dec
const RESET_BG: [u8; 3] = [148, 163, 184]; // .btn.reset
const CARD_BG: [u8; 3] = [30, 41, 59]; // .card

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

/// Number of pixels close to `color` (tolerates antialiasing edges).
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

/// Scanline index with the most pixels close to `color`, and that pixel count.
fn busiest_row(
    rgba: &[u8],
    width: usize,
    height: usize,
    color: [u8; 3],
    tolerance: i32,
) -> (usize, usize) {
    let mut best = (0usize, 0usize);
    for y in 0..height {
        let start = y * width * 4;
        let count = pixels_near(&rgba[start..start + width * 4], color, tolerance);
        if count > best.1 {
            best = (y, count);
        }
    }
    best
}

/// Last scanline that contains a pixel close to `color`.
fn last_row_with_color(
    rgba: &[u8],
    width: usize,
    height: usize,
    color: [u8; 3],
    tolerance: i32,
) -> usize {
    (0..height)
        .rev()
        .find(|y| {
            let start = y * width * 4;
            pixels_near(&rgba[start..start + width * 4], color, tolerance) > 0
        })
        .expect("color never painted")
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
    assert_non_trivial("counter-large", &png, &rgba);
    // Card, count glyphs and the three buttons are all painted.
    assert!(pixels_near(&rgba, CARD_BG, 8) > 1000, "card background");
    assert!(pixels_near(&rgba, COUNT_FG, 40) > 200, "count text");
    assert!(pixels_near(&rgba, INC_BG, 12) > 200, "+1 button");
    assert!(pixels_near(&rgba, DEC_BG, 12) > 200, "-1 button");
    assert!(pixels_near(&rgba, RESET_BG, 12) > 200, "reset button");
    let path = write_proof("counter", LARGE.0, LARGE.1, &png);
    assert!(path.exists(), "proof png missing: {}", path.display());
}

#[test]
fn small_viewport_keeps_count_status_and_all_buttons_visible() {
    let state = Arc::new(app::script_rs::State::new());
    let (png, rgba) = render(&state, SMALL.0, SMALL.1);
    assert_non_trivial("counter-small", &png, &rgba);
    write_proof("counter", SMALL.0, SMALL.1, &png);

    let width = SMALL.0 as usize;
    let height = SMALL.1 as usize;
    assert!(
        pixels_near(&rgba, COUNT_FG, 40) > 200,
        "count text missing at 480x360"
    );

    // The status line is derived from the count, so incrementing must change
    // the rendered pixels ("not positive" -> "positive").
    let before = rgba.clone();
    state.increment();
    let (_, after) = render(&state, SMALL.0, SMALL.1);
    assert_ne!(before, after, "status label did not react to increment()");

    // All three buttons survive the small viewport. Velox lays them out as
    // full-width blocks, so they are stacked: each one paints a wide band and
    // the bands must not overlap.
    let mut previous_band = 0usize;
    for (label, color) in [("+1", INC_BG), ("-1", DEC_BG), ("reset", RESET_BG)] {
        let (row, pixels) = busiest_row(&rgba, width, height, color, 12);
        assert!(pixels > 150, "{label} button missing at 480x360");
        assert!(
            row >= previous_band + 20,
            "{label} button is not stacked below the previous one (row {row} vs {previous_band})"
        );
        previous_band = row;
    }

    // The lowest button is not clipped by the bottom edge.
    let last_reset_row = last_row_with_color(&rgba, width, height, RESET_BG, 12);
    assert!(
        last_reset_row + 4 < height,
        "reset button clipped at the bottom of 480x360 (row {last_reset_row})"
    );
}
