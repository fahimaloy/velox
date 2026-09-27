//! The measurement seam, exercised with a registered measurer.
//!
//! The measurer here is synthetic on purpose. Its vertical metrics imitate a
//! real Latin face — cap/ascender height above the baseline, descent only when
//! the run actually has a descender, nothing at all for a run with no ink — so
//! the assertions can be pixel-exact and machine-independent. Whether a *real*
//! font backend supplies those metrics is a separate question, answered against
//! real Skia in `velox-renderer`; what this file proves is that layout uses
//! whatever the seam reports, and nothing else.
//!
//! `set_skia_measurer` writes a process-global with no unregister, so this binary
//! registers exactly ONE measurer, once, shared by every test. Two tests
//! registering two different functions would race, and the loser would assert
//! against a measurer it did not install — which is precisely what a first
//! draft of this file did. The fallback has its own binary,
//! `text_metrics_fallback.rs`.

use std::sync::Once;

use velox_dom::h;
use velox_dom::layout::compute_layout;
use velox_dom::text_wrap::{MeasuredText, measure_text, measure_text_metrics, set_skia_measurer};

const FAMILY: &str = "system-ui";
const FONT_SIZE: f32 = 16.0;

/// True for glyphs that rise above x-height in a Latin face.
fn has_ascender(run: &str) -> bool {
    run.chars()
        .any(|c| "bdfhkltABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(c))
}

/// True for glyphs that drop below the baseline in a Latin face.
fn has_descender(run: &str) -> bool {
    run.chars().any(|c| "gjpqy".contains(c))
}

/// A stand-in for a font backend: real width behaviour (0.5em per char, so line
/// breaking is unchanged) and content-dependent vertical metrics.
///
/// An all-whitespace run reports no vertical extent at all, which is what a real
/// font backend reports for it — a space has no ink. That is the one case where
/// "the measurer's answer" is a true statement and a useless one.
fn test_measurer(text: &str, font_size: f32, _family: &str, scale: f32) -> MeasuredText {
    let snapped = if scale.is_finite() && scale > 0.0 && scale != 1.0 {
        (font_size * scale).round() / scale
    } else {
        font_size
    };
    let width = snapped * 0.5 * text.chars().count() as f32;
    if text.chars().all(char::is_whitespace) {
        return MeasuredText {
            width,
            ascent: 0.0,
            descent: 0.0,
        };
    }
    MeasuredText {
        width,
        ascent: if has_ascender(text) {
            snapped * 0.55
        } else {
            snapped * 0.30
        },
        descent: if has_descender(text) {
            snapped * 0.20
        } else {
            0.0
        },
    }
}

/// Install the one measurer this binary uses. Idempotent, so every test calls it
/// and no test can be run against someone else's measurer.
fn register() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| set_skia_measurer(test_measurer));
}

/// The heights of the text line boxes `vnode` lays out, in order.
fn line_heights(vnode: &velox_dom::VNode) -> Vec<i32> {
    let laid = compute_layout(vnode, 300, 300);
    let mut out = vec![laid.rect.h];
    for child in &laid.children {
        if child.children.is_empty() {
            out.push(child.rect.h);
        } else {
            for grandchild in &child.children {
                out.push(grandchild.rect.h);
            }
        }
    }
    out
}

/// A div whose only child is one text node, wrapping only at explicit newlines.
fn pre_text_div(text: &str) -> velox_dom::VNode {
    register();
    let style = format!("width:240px;font-size:{FONT_SIZE}px;white-space:pre");
    h(
        "div",
        vec![("style", style.as_str())],
        vec![velox_dom::VNode::Text(text.to_string())],
    )
}

#[test]
fn a_registered_measurers_vertical_metrics_reach_the_seam() {
    register();
    let caps = measure_text_metrics("Hg", FONT_SIZE, FAMILY, 1.0);
    assert_eq!(caps.ascent, 8.8, "0.55em above the baseline");
    assert_eq!(caps.descent, 3.2, "0.20em below it");
    let x_only = measure_text_metrics("xxx", FONT_SIZE, FAMILY, 1.0);
    assert_eq!(x_only.ascent, 4.8, "x-height, not cap height");
    assert_eq!(x_only.descent, 0.0, "no descender in the run");
    assert_ne!(
        caps.line_extent(),
        x_only.line_extent(),
        "the seam must report a content-dependent extent, not a constant"
    );
}

#[test]
fn the_width_only_entry_point_is_a_projection_of_the_registered_measurer() {
    register();
    for text in ["", "a", "Hg", "not positive", "iiiiiiiii"] {
        for size in [1.0f32, 11.0, 16.0, 16.5, 33.0] {
            for scale in [1.0f32, 1.25, 1.5, 2.0] {
                let width_only = measure_text(text, size, FAMILY, scale);
                let full = measure_text_metrics(text, size, FAMILY, scale);
                assert_eq!(
                    width_only, full.width,
                    "width moved for {text:?} at {size}px scale {scale}"
                );
            }
        }
    }
}

#[test]
fn a_line_box_is_as_tall_as_the_run_in_it_and_not_the_fonts_nominal_line_height() {
    let heights = line_heights(&pre_text_div("Hg\nxxx"));
    assert_eq!(
        &heights[1..],
        &[12, 5],
        "each line box is the round of its own run's ascent+descent \
         (Hg: 8.8+3.2 = 12, xxx: 4.8+0 = 5)"
    );
    let nominal = (FONT_SIZE * 1.2).round() as i32;
    assert_eq!(nominal, 19);
    assert!(
        heights.iter().all(|h| *h != nominal),
        "no line box may still be the fixed 1.2em multiplier: {heights:?}"
    );
}

#[test]
fn two_lines_of_different_content_get_different_heights() {
    let heights = line_heights(&pre_text_div("Hg\nxxx"));
    assert_eq!(heights.len(), 3, "root, div, two line boxes: {heights:?}");
    assert_ne!(
        heights[1], heights[2],
        "line box height must follow the content, not a per-font constant: {heights:?}"
    );
    assert!(
        heights[1] > heights[2],
        "the run with a descender is taller"
    );
}

#[test]
fn a_run_with_no_ink_does_not_collapse_its_line_box() {
    // An all-space run genuinely has no ink, so its extent really is zero. The
    // seam must not pass that on as a zero-height line box.
    let heights = line_heights(&pre_text_div(" "));
    assert!(
        heights.iter().all(|h| *h > 0),
        "a line box collapsed to {heights:?}"
    );
    assert_eq!(
        heights[1],
        (FONT_SIZE * 1.2).round() as i32,
        "with no usable vertical measurement the documented approximation applies"
    );
}

#[test]
fn an_inkless_run_keeps_its_real_width() {
    register();
    // Only the vertical half may be substituted. The width is a real measurement
    // and falling back on it too would silently change every wrap.
    let m = measure_text_metrics("Hg", FONT_SIZE, FAMILY, 1.0);
    assert_eq!(
        m.width,
        FONT_SIZE * 0.5 * 2.0,
        "the registered measurer's width must be used verbatim"
    );
    assert_eq!(m.ascent, FONT_SIZE * 0.55);
    assert_eq!(m.descent, FONT_SIZE * 0.2);
    let blank = measure_text_metrics(" ", FONT_SIZE, FAMILY, 1.0);
    assert_eq!(blank.width, FONT_SIZE * 0.5, "width of a blank run is real");
    assert_eq!(blank.ascent, FONT_SIZE * 0.8, "vertical fell back");
    assert_eq!(blank.descent, FONT_SIZE * 0.4);
}
