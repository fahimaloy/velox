//! The documented vertical-metric approximation, with no measurer registered.
//!
//! This is a separate test binary from `text_metrics_seam.rs` on purpose.
//! `set_skia_measurer` writes a process-global with no way to unregister, so any
//! binary that registers one can no longer observe the no-measurer path, and
//! which of the two tests ran first would otherwise decide the answer.

use velox_dom::layout::FontMetrics;
use velox_dom::style::{TextOverflow, WhiteSpace};
use velox_dom::text_wrap::{measure_text, measure_text_metrics, wrap_text_with_options};

const FAMILY: &str = "system-ui";

#[test]
fn heuristic_vertical_is_the_documented_approximation() {
    let (ascent, descent) = FontMetrics::heuristic_vertical(16.0);
    assert_eq!(ascent, 12.8, "ascent is documented as 0.8em");
    assert_eq!(descent, 6.4, "descent is documented as 0.4em");
}

#[test]
fn font_metrics_carries_the_approximation_on_both_its_new_fields() {
    let m = FontMetrics::from_font_size(16.0);
    assert_eq!(m.ascent, 12.8);
    assert_eq!(m.descent, 6.4);
}

#[test]
fn char_width_and_line_height_are_untouched() {
    // Requirement: widening the seam must not change the horizontal metrics or
    // how the line height is derived. These two values are what they have always
    // been; if a later task moves either, this says so.
    let m = FontMetrics::from_font_size(16.0);
    assert_eq!(m.char_width, 9.6, "0.6em per char, unchanged");
    assert_eq!(m.line_height, 19.2, "1.2em, unchanged");
}

#[test]
fn a_fallback_run_measures_to_the_documented_approximation() {
    let m = measure_text_metrics("Hg", 16.0, FAMILY, 1.0);
    assert_eq!(m.ascent, 12.8, "no measurer: ascent is the approximation");
    assert_eq!(m.descent, 6.4, "no measurer: descent is the approximation");
    assert_eq!(m.width, 16.0, "width is 0.5em per char, unchanged");
    assert_eq!(m.line_extent(), 19.2, "0.8em + 0.4em");
}

#[test]
fn a_fallback_run_never_measures_zero_vertical() {
    // A non-Skia build has no font backend. If the approximation ever reported a
    // zero ascent or descent, every line box on the no-Skia path would collapse to
    // zero height — a silent, catastrophic wrong answer on the path a test is
    // most likely to reach.
    for text in ["", " ", "\u{2007}", "Hg", "x", "iiiiiiiii"] {
        for size in [1.0f32, 9.0, 12.5, 16.0, 23.75, 64.0, 129.0] {
            let m = measure_text_metrics(text, size, FAMILY, 1.0);
            assert!(
                m.ascent > 0.0 && m.descent > 0.0 && m.line_extent() > 0.0,
                "fallback vertical metrics collapsed for {text:?} at {size}px: \
                 ascent={} descent={}",
                m.ascent,
                m.descent
            );
        }
    }
}

#[test]
fn the_approximation_leaves_every_line_height_exactly_where_it_was() {
    // The split of 0.8em/0.4em is a guess, but its total is not: 1.2em is the
    // `line_height` the wrap path has always used. So no line box moves when the
    // seam is introduced. This is checked over a wide range of sizes, and at the
    // half-integer sizes where a float sum is most likely to round the other way.
    for i in 1..=400 {
        let size = i as f32 * 0.05;
        let m = measure_text_metrics("Hg", size, FAMILY, 1.0);
        let from_seam = m.line_extent().round() as i32;
        let from_font_metrics = (size * 1.2).round() as i32;
        assert_eq!(
            from_seam, from_font_metrics,
            "line height moved at {size}px: seam {from_seam} vs 1.2em {from_font_metrics}"
        );
    }
}

#[test]
fn a_wrapped_line_is_as_tall_on_the_fallback_path_as_it_always_was() {
    let lines = wrap_text_with_options(
        "Hg",
        200,
        16.0,
        FAMILY,
        1.0,
        WhiteSpace::Pre,
        TextOverflow::Clip,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].height, 19, "16 * 1.2 rounded, unchanged");
    assert_eq!(lines[0].width, 16, "0.5em per char, unchanged");
}

#[test]
fn the_width_only_entry_point_is_unchanged_by_the_vertical_half() {
    // `measure_text` must stay a pure projection of the same measurement: adding
    // vertical metrics to the seam must not have perturbed the width by a
    // rounding step or a re-snap.
    for text in ["", "a", "Hg", "not positive", "iiiiiiiii"] {
        for size in [1.0f32, 11.0, 16.0, 16.5, 33.0] {
            for scale in [1.0f32, 1.25, 1.5, 2.0] {
                let width_only = measure_text(text, size, FAMILY, scale);
                let full = measure_text_metrics(text, size, FAMILY, scale);
                assert_eq!(
                    width_only, full.width,
                    "width moved for {text:?} at {size}px scale {scale}: \
                     {width_only} vs seam {}",
                    full.width
                );
            }
        }
    }
}

/// The 0.5em-per-character width formula, pinned as a formula rather than as a
/// relationship, because a relationship here would only prove the two halves of
/// the seam agree with each other and would happily accept any 0.5.
///
/// This matters more than it looks. `velox-renderer` has a second, independent
/// copy of this exact expression in its non-Skia branch, and the 0.6 `char_width`
/// on `FontMetrics` is a third. Falsification showed the pre-existing suite does
/// not notice a 0.62 in `char_width`, so a pin of this kind is the only guard.
#[test]
fn the_fallback_width_is_exactly_half_an_em_per_character_at_the_snapped_size() {
    for text in ["", "a", "Hg", "not positive", "iiiiiiiii"] {
        for size in [1.0f32, 11.0, 16.0, 16.5, 33.0, 47.25] {
            for scale in [1.0f32, 1.25, 1.5, 2.0] {
                let snapped = if scale.is_finite() && scale > 0.0 && scale != 1.0 {
                    (size * scale).round() / scale
                } else {
                    size
                };
                let expected = snapped * 0.5 * text.chars().count() as f32;
                let actual = measure_text(text, size, FAMILY, scale);
                assert_eq!(
                    actual, expected,
                    "fallback width for {text:?} at {size}px scale {scale} \
                     is not 0.5em per char at the snapped size"
                );
            }
        }
    }
}
