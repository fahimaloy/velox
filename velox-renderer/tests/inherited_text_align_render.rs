//! Headless pixel proof that the expanded inheritable set changes rendered
//! output (Task 11 / CX-10): `.app { text-align: center }` must propagate
//! through the cascade inheritance filter (`velox_style::apply_with_cascade`)
//! onto a plain child row whose own matching carries no text-align, moving
//! the row's text from left-aligned to horizontally centered.
//!
//! Requires `--features skia-native` (raster surface, no GPU/window needed).

#![cfg(feature = "skia-native")]

use velox_dom::{Props, VNode, h, text};
use velox_renderer::render_vnode_to_rgba;
use velox_style::{Stylesheet, apply_with_cascade};

/// Constrained canvas/viewport width: 400px, so centering is observable.
const W: i32 = 400;
const H: i32 = 120;

/// `.app` root carrying a plain inner row. The row has NO class, NO inline
/// style — `text-align` can reach it (and its text) only by INHERITING it
/// from `.app` through the cascade's inheritable-props filter.
fn tree() -> VNode {
    h(
        "div",
        Props::new().set("class", "app"),
        vec![h("div", Props::new(), vec![text("VEL0X")])],
    )
}

/// Style via the cascade path, render headless, and measure the text.
/// Returns (text_pixel_count, column_centroid = mean x of text pixels).
///
/// Pixel classification: the `.app` box paints an opaque white background
/// (255,255,255,255); the text paints opaque black with anti-aliased edges.
/// The raster surface clears to transparent (0,0,0,0), so a pixel is "text"
/// when it is opaque-ish (alpha > 60) and not near-white (some channel
/// < 200) — white bg and bare transparent canvas are both excluded.
fn render_and_measure(css: &str) -> (usize, f64) {
    let author = Stylesheet::parse(css);
    // The cascade pass is the point: it must propagate `.app`'s text-align
    // into the plain row's inline style via filter_inheritable.
    let styled = apply_with_cascade(&tree(), &author);
    // Render the cascade-styled tree with an empty sheet (same convention as
    // app_render_test.rs): any visual delta comes from the cascade above.
    let rgba = render_vnode_to_rgba(&styled, &Stylesheet::default(), W, H).expect("render to rgba");

    let mut count = 0usize;
    let mut sum_x = 0f64;
    for y in 0..H as usize {
        for x in 0..W as usize {
            let base = (y * W as usize + x) * 4;
            let (r, g, b, a) = (rgba[base], rgba[base + 1], rgba[base + 2], rgba[base + 3]);
            if a > 60 && (r < 200 || g < 200 || b < 200) {
                count += 1;
                sum_x += x as f64;
            }
        }
    }
    let centroid = if count == 0 {
        f64::NAN
    } else {
        sum_x / count as f64
    };
    (count, centroid)
}

/// Common geometry/color/font declarations for both variants. Only
/// `text-align: center` differs between the centered sheet and the control.
fn css(text_align: Option<&str>) -> String {
    let align = match text_align {
        Some(a) => format!("text-align: {a};"),
        None => String::new(),
    };
    format!(
        ".app {{ width: 400px; height: 120px; background: #FFFFFF; \
         color: #000000; font-size: 40px; {align} }}"
    )
}

#[test]
fn inherited_text_align_centers_rendered_text() {
    // Centered variant: `.app` centers; the plain row inherits it.
    let (n_center, x_center) = render_and_measure(&css(Some("center")));
    assert!(
        n_center > 200,
        "centered render must contain text pixels, got {n_center} \
         (font rendering unavailable headless?)"
    );

    // Control: identical tree/sheet minus text-align → default left align.
    let (n_left, x_left) = render_and_measure(&css(None));
    assert!(
        n_left > 200,
        "left-aligned control must contain text pixels, got {n_left} \
         (font rendering unavailable headless?)"
    );

    // Arithmetic:
    // - text row band is identical in both renders (same tree geometry);
    // - left-aligned text starts at x = 0, so its centroid ≈ text_width / 2
    //   (roughly 60–80px for 40px glyphs);
    // - centered text is placed at x = (400 - text_width) / 2, so its
    //   centroid ≈ 200 ± half the text width.
    println!(
        "left: n={n_left} centroid={x_left:.1} | centered: n={n_center} centroid={x_center:.1}"
    );
    assert!(
        x_center > x_left + 50.0,
        "inherited text-align must shift the text centroid right by > 50px: \
         left={x_left:.1} centered={x_center:.1}"
    );
    let half_width = W as f64 / 2.0;
    let tolerance = 0.15 * W as f64; // within 15% of width/2 → [140, 260]
    assert!(
        (x_center - half_width).abs() < tolerance,
        "centered centroid must sit near the horizontal center ({half_width} \
         ± {tolerance}), got {x_center:.1}"
    );
    // Sanity: the control must NOT read as centered (guards a filter change
    // that would center everything regardless of the sheet).
    assert!(
        (x_left - half_width).abs() >= tolerance,
        "left-aligned control must stay near the left edge, got {x_left:.1}"
    );
}
