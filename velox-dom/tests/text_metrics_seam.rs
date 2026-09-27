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
            snapped * 2.0
        } else {
            snapped * 0.30
        },
        descent: if has_descender(text) {
            snapped * 0.5
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
    assert_eq!(caps.ascent, 32.0, "2.0em above the baseline");
    assert_eq!(caps.descent, 8.0, "0.5em below it");
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

/// The auto height of a div holding exactly one line, in a parent that does not
/// constrain it.
///
/// A block container's height is the sum of its line boxes', so a div with one
/// line IS that line box's height. This is the observable that lets a line box
/// be measured at all: a text fragment's own box is the line's font content box,
/// which is the same height on every line, and its top sits below the line box's
/// top by however far the run's ink overshoots the strut. A line box's extent is
/// therefore read from its CONTAINER, never recomputed from the alignment pass
/// that produced it.
fn one_line_height(text: &str) -> i32 {
    register();
    let style = format!("font-size:{FONT_SIZE}px;white-space:pre");
    let outer = h(
        "div",
        vec![],
        vec![h(
            "div",
            vec![("style", style.as_str())],
            vec![velox_dom::VNode::Text(text.to_string())],
        )],
    );
    compute_layout(&outer, 600, 600).children[0].rect.h
}

#[test]
fn a_line_box_is_as_tall_as_the_run_in_it_and_not_the_fonts_nominal_line_height() {
    // RESTATED for the inline formatting context. This test used to read the
    // line box's height off a text node's own `rect.h`. That conflated two
    // different boxes, and the inline formatting context is what separates them:
    //
    //   * a text FRAGMENT's box is its own font's content box -- ascent +
    //     descent, CSS 2.1 §10.8.1 -- so it is the STRUT's height for every
    //     fragment on a line, whatever ink the run has;
    //   * the LINE BOX is the union of the fragments on it, so a run whose ink
    //     overshoots the strut makes the LINE taller without making its own box
    //     taller.
    //
    // Both halves are asserted: the fragments' boxes, and the lines' extents.
    let heights = line_heights(&pre_text_div("Hg\nxxx"));
    // The strut: 1.069em + 0.293em from the default face's typo metrics, 21.8px
    // at 16px. Derived from the font file, NOT by re-running the implementation.
    let nominal = (FONT_SIZE * 1.362).round() as i32;
    assert_eq!(nominal, 22, "the strut at 16px");
    assert_eq!(
        heights[1..],
        [22, 22],
        "every text fragment's box is the line's font content box -- the strut -- \
         whatever the run's ink: {heights:?}"
    );
    // "Hg" ink is 2.0em + 0.5em = 2.5em = 40px, over the 21.8px strut, so the
    // RUN sets that line box's height. "xxx" ink is 0.30em = 4.8px, under the
    // strut, so the STRUT sets that one's.
    assert_eq!(
        one_line_height("Hg"),
        40,
        "the \"Hg\" run's ink is 2.5em, over the 1.362em strut, so the RUN \
         must set that line box's height"
    );
    assert_eq!(
        one_line_height("xxx"),
        nominal,
        "the \"xxx\" run's ink is 0.30em, under the strut, so the STRUT must \
         set that line box's height"
    );
}

#[test]
fn two_lines_of_different_content_get_different_heights() {
    // RESTATED with the same distinction as the test above: the two LINE boxes
    // differ, and each fragment's own box does not. They are separated by
    // measuring each line in a container of its own, which is also what keeps
    // the assertion independent of the alignment pass that produced them.
    let first = one_line_height("Hg");
    let second = one_line_height("xxx");
    assert_ne!(
        first, second,
        "line box height must follow the content, not a per-font constant: \
         {first} then {second}"
    );
    assert!(
        first > second,
        "the run with a descender is taller: {first} then {second}"
    );
    // Two lines in ONE container must come to exactly the sum of the two
    // measured apart. That is what says the line boxes are independent -- that
    // the second is not inheriting the first's height -- rather than a block
    // container reporting one height for everything inside it.
    let both = {
        register();
        let style = format!("font-size:{FONT_SIZE}px;white-space:pre");
        let outer = h(
            "div",
            vec![],
            vec![h(
                "div",
                vec![("style", style.as_str())],
                vec![velox_dom::VNode::Text("Hg\nxxx".to_string())],
            )],
        );
        compute_layout(&outer, 600, 600).children[0].rect.h
    };
    assert_eq!(
        both,
        first + second,
        "a block container's height is the sum of its line boxes: {both} \
         against {first} + {second}"
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
        (FONT_SIZE * 1.362).round() as i32,
        "a zero extent must not pass through: the seam substitutes the ink \
         approximation (1.2em) and the STRUT then floors it to 1.362em. Were the \
         zero passed through instead, this would be 0"
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
    assert_eq!(m.ascent, FONT_SIZE * 2.0);
    assert_eq!(m.descent, FONT_SIZE * 0.5);
    let blank = measure_text_metrics(" ", FONT_SIZE, FAMILY, 1.0);
    assert_eq!(blank.width, FONT_SIZE * 0.5, "width of a blank run is real");
    assert_eq!(blank.ascent, FONT_SIZE * 0.8, "vertical fell back");
    assert_eq!(blank.descent, FONT_SIZE * 0.4);
}

// ===== the bare `VNode::Text` path in `at()`, not the wrapped-lines path =====
//
// Every other test in this file drives a text node through `wrap_text_*`, so
// they all exercise `text_wrap::line_box_height`. `text_dimensions` serves a
// different path: `at()`'s own `VNode::Text` arm, taken when a text node is the
// laid-out node itself rather than a child a container wraps. R-5a changed its
// height and nothing tested it, so this is the only place the change is
// verified at the seam it actually happens on.

/// The height `at()`'s bare `VNode::Text` arm gives a run, which is the path
/// `text_dimensions` serves.
fn bare_text_height(text: &str) -> i32 {
    register();
    compute_layout(&velox_dom::VNode::Text(text.to_string()), 300, 300)
        .rect
        .h
}

/// A bare text node's height comes from the registered measurer, not from a
/// fixed `font_size * 1.2`.
///
/// This is the test that makes R-5a's change to `text_dimensions` observable at
/// all. The old height was `16 * 1.2 = 19` for every run, so an assertion that
/// merely checks the height is 19 would pass against the code before R-5a too;
/// this one checks that two runs of the same size get different heights, and
/// that each equals the extent the seam reports for it, and neither of those can
/// be true of a 1.2em multiplier.
#[test]
fn a_bare_text_node_is_as_tall_as_the_run_in_it() {
    let caps = bare_text_height("Hg");
    let x_only = bare_text_height("xxx");
    assert_eq!(caps, 40, "\"Hg\": 2.0em + 0.5em = 40px, over the strut");
    assert_eq!(
        x_only, 22,
        "\"xxx\": 0.30em = 4.8px is UNDER the 1.362em strut, so the strut decides"
    );
    assert_ne!(
        caps, x_only,
        "two runs at the same size must get different heights, or the height is \
         not following the content at all"
    );
    // "Hg" is 40, above every constant in play. "xxx" is 22, which happens to
    // equal the old 1.2em: that is coincidence, not the old code, because the
    // bare-text path went 19 -> 9 (ink) -> 22 (strut) and both edges are asserted
    // above. Asserted here so the collision is visible rather than implied.
    assert_eq!(
        caps, 40,
        "\"Hg\" ink still clears the strut on the bare-text path"
    );
    assert_ne!(
        caps, 22,
        "if \"Hg\" also came out 22 the bare-text path would be ignoring the run"
    );
}

/// The width of a bare text node is still the 0.6 heuristic, and stays that way.
///
/// `text_dimensions` deliberately measures only its HEIGHT through the seam: its
/// width is `chars * font_size * 0.6`, which is a different formula from the
/// seam's 0.5-per-char, and switching it would change every text width in a Skia
/// run. This pins that the half that was left alone really was left alone.
#[test]
fn a_bare_text_nodes_width_half_is_still_the_heuristic_not_the_seam() {
    register();
    // 0.6em per char at 16px is 9.6px; the seam's 0.5em would be 8.0px.
    let laid = compute_layout(&velox_dom::VNode::Text("Hg".to_string()), 300, 300);
    assert_eq!(
        laid.rect.w, 19,
        "2 chars * 0.6em * 16px, rounded — width is NOT the strut's business"
    );
    assert_ne!(
        laid.rect.w, 16,
        "if this is 16 the width went through the seam, which is not what R-5a \
         decided and would change every text width in a Skia run"
    );
}
