//! Shared helpers for the tests that need a measurer with vertical metrics.
//!
//! `set_skia_measurer` is a process global with no unregister, so a test binary
//! that registers one can never observe the no-measurer fallback, and a binary
//! that does not register can never see real vertical metrics. That is why the
//! two concerns live in separate binaries -- `text_metrics_fallback.rs` and
//! `text_metrics_seam.rs` -- and why the measurer they share lives here rather
//! than being written out twice.
//!
//! A real measurer needs a font backend, which only `velox-renderer` has, so
//! these tests use a synthetic one. It is a MODEL, not a measurement, and tests
//! built on it prove that the layout honours the seam's vertical metrics -- not
//! that a real font produces those numbers. The real-measure evidence lives in
//! `velox-renderer`'s `skia-native`-gated tests, and it proves sensitivity for
//! the same reason.

#![allow(dead_code)]

use std::sync::Once;
use velox_dom::text_wrap::{MeasuredText, set_skia_measurer};

/// The default face's OS/2 `sTypoAscender`, from `NotoSans-Regular.ttf`
/// (unitsPerEm 1000). Read off the font file, not computed from the layout.
pub const FONT_ASCENT_EM: f32 = 1.069;
/// The same table's `sTypoDescender`, negated.
pub const FONT_DESCENT_EM: f32 = 0.293;
/// The same table's `sxHeight`.
pub const FONT_X_HEIGHT_EM: f32 = 0.536;
/// A line box is at least this tall, in em, whatever run is on it.
pub const STRUT_EM: f32 = FONT_ASCENT_EM + FONT_DESCENT_EM;
/// The synthetic measurer's ink for a run with an ascender, in em. Deliberately
/// OVER the strut, so a test can tell a line whose height the run decided from
/// one whose height the strut decided.
pub const SYNTHETIC_ASCENT_EM: f32 = 2.0;
/// The same, for a run with a descender.
pub const SYNTHETIC_DESCENT_EM: f32 = 0.5;
/// The synthetic measurer's ink for a run with neither, in em. Deliberately
/// UNDER the strut, for the same reason.
pub const SYNTHETIC_XHEIGHT_EM: f32 = 0.30;
/// The synthetic measurer's advance width per character, in em. Matches the
/// no-measurer fallback's 0.5em, so widths are the same with and without it and
/// a test can change only the vertical half.
pub const SYNTHETIC_WIDTH_EM: f32 = 0.5;

/// Whether any character in `text` rises above x-height.
pub fn has_ascender(text: &str) -> bool {
    text.chars().any(|c| {
        "bdfhklt"
            .chars()
            .chain("ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars())
            .any(|a| a == c)
    })
}

/// Whether any character in `text` descends below the baseline.
pub fn has_descender(text: &str) -> bool {
    text.chars().any(|c| "gjpqy".chars().any(|a| a == c))
}

/// A measurer with a deliberate, content-dependent vertical extent.
pub fn synthetic_measurer(
    text: &str,
    font_size_px: f32,
    _font_family: &str,
    scale: f32,
) -> MeasuredText {
    let snapped = if scale.is_finite() && scale > 0.0 && (scale - 1.0).abs() > f32::EPSILON {
        (font_size_px * scale).round() / scale
    } else {
        font_size_px
    };
    if text.chars().all(char::is_whitespace) {
        // Real ink, honestly reported as none: a run of spaces has no glyphs, so
        // the seam's zero-extent guard is the thing under test.
        return MeasuredText {
            width: snapped * SYNTHETIC_WIDTH_EM * text.chars().count() as f32,
            ascent: 0.0,
            descent: 0.0,
        };
    }
    MeasuredText {
        width: snapped * SYNTHETIC_WIDTH_EM * text.chars().count() as f32,
        ascent: if has_ascender(text) {
            snapped * SYNTHETIC_ASCENT_EM
        } else {
            snapped * SYNTHETIC_XHEIGHT_EM
        },
        descent: if has_descender(text) {
            snapped * SYNTHETIC_DESCENT_EM
        } else {
            0.0
        },
    }
}

/// Install the one measurer a binary uses. Idempotent, so every test can call it
/// and no test can be run against someone else's measurer.
pub fn register_synthetic() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| set_skia_measurer(synthetic_measurer));
}
