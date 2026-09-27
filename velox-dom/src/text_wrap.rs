// Text wrapping and layout utilities — Skia-unified, white-space/ellipsis aware.

use crate::layout::{FontMetrics, LayoutNode, Rect};
use crate::style::{TextOverflow, WhiteSpace};

/// Result of wrapping text into lines
pub struct TextLine {
    pub text: String,
    pub width: i32,
    pub height: i32,
}

/// Internal snapped font size: logical -> device snap -> logical
#[inline]
fn snapped_size(font_size_px: f32, scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 && scale != 1.0 {
        (font_size_px * scale).round() / scale
    } else {
        font_size_px
    }
}

/// Heuristic measure matching velox-renderer fallback (also Skia FontCache when available).
/// Uses snapped size * 0.5 per-char (renderer fallback) — when Skia is available via
/// velox-renderer::measure_text the test supplies the renderer value; this fallback
/// is kept identical to renderer's non-skia path so headless parity holds.
fn measure_heuristic(text: &str, font_size_px: f32, scale: f32) -> f32 {
    let snapped = snapped_size(font_size_px, scale);
    snapped * 0.5 * text.chars().count() as f32
}

use std::sync::RwLock;

/// One measured run of text: how wide it advances, and how far it reaches above
/// and below its own baseline.
///
/// `ascent` and `descent` are the extent of *this run's ink*, so they are a
/// property of the characters, not of the font: a run of "xxx" stops at
/// x-height and measures a smaller ascent than a run of "Hg". A font's
/// typographic ascent/descent — the strut a line box may not be shorter than —
/// are the separate `ascent`/`descent` on `crate::layout::FontMetrics`.
///
/// The vertical split is a fact the measurer now *reports* where a real font
/// backend is registered. It is not yet the fact a line box is built from: the
/// height of a line is still `ascent + descent` of its own run, with no strut
/// floor. That floor, line boxes, baseline alignment and inline-block all belong
/// to the inline formatting context, not here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredText {
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}

impl MeasuredText {
    /// Total vertical extent of the run: the full height of its ink.
    #[inline]
    pub fn line_extent(&self) -> f32 {
        self.ascent + self.descent
    }
}

static SKIA_MEASURER: RwLock<Option<fn(&str, f32, &str, f32) -> MeasuredText>> = RwLock::new(None);
static CURRENT_SCALE: RwLock<f32> = RwLock::new(1.0);

/// Set the current viewport scale for layout text measure (single rounding point remains Viewport).
pub fn set_current_scale(scale: f32) {
    if let Ok(mut g) = CURRENT_SCALE.write() {
        *g = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
    }
}
pub fn current_scale() -> f32 {
    CURRENT_SCALE.read().ok().map(|g| *g).unwrap_or(1.0)
}

/// Register a measurer backed by a real font backend (called by velox-renderer
/// at init). It reports the run's advance width *and* its vertical extent, so
/// layout can size a line box from the content instead of from a multiplier.
pub fn set_skia_measurer(f: fn(&str, f32, &str, f32) -> MeasuredText) {
    if let Ok(mut g) = SKIA_MEASURER.write() {
        *g = Some(f);
    }
}

fn measure_text_internal(text: &str, font_size_px: f32, font_family: &str, scale: f32) -> f32 {
    if let Ok(g) = SKIA_MEASURER.read() {
        if let Some(f) = *g {
            return f(text, font_size_px, font_family, scale).width;
        }
    }
    measure_heuristic(text, font_size_px, scale)
}

/// Measure one run's vertical extent as well as its width, through the same seam
/// the width-only path uses.
///
/// With no measurer registered the vertical half is the documented approximation
/// on `FontMetrics` — `FontMetrics::heuristic_vertical` — so it is never zero and
/// a line box never collapses. A registered measurer that reports no usable
/// vertical extent (an all-whitespace run, which really has no ink) also falls
/// back, but only for the vertical half: its width is a real measurement and is
/// kept. Falling back rather than propagating zero is what stops a blank run from
/// silently producing a zero-height line.
pub fn measure_text_metrics(
    text: &str,
    font_size_px: f32,
    font_family: &str,
    scale: f32,
) -> MeasuredText {
    let (ascent, descent) = FontMetrics::heuristic_vertical(snapped_size(font_size_px, scale));
    if let Ok(g) = SKIA_MEASURER.read() {
        if let Some(f) = *g {
            let m = f(text, font_size_px, font_family, scale);
            if m.ascent.is_finite() && m.descent.is_finite() && m.line_extent() > 0.0 {
                return m;
            }
            return MeasuredText {
                width: m.width,
                ascent,
                descent,
            };
        }
    }
    MeasuredText {
        width: measure_heuristic(text, font_size_px, scale),
        ascent,
        descent,
    }
}

/// Public measure for tests / layout: snapped, family-aware (family ignored in heuristic).
pub fn measure_text(text: &str, font_size_px: f32, font_family: &str, scale: f32) -> f32 {
    measure_text_internal(text, font_size_px, font_family, scale)
}

/// Height of a line box containing this run, floored by the container's strut.
///
/// This is THE line box height. Every path that turns a run into a box calls it:
/// `wrap_text`, `wrap_text_with_options`, and `layout::text_dimensions` (the bare
/// `VNode::Text` arm). It was private once, and `text_dimensions` re-derived the
/// formula inline, so a strut added here would have reached the wrapping paths and
/// skipped that one.
///
/// The floor is `strut.ascent + strut.descent` — 1.362em at the default size,
/// from the face's own typographic metrics. It is what makes a line of "xxx" the
/// same height as a line of "Hg", which is what a browser does and what this did
/// not. A run that overshoots the strut (a tall run, or a large `font-size`)
/// still sets the height itself.
///
/// With no measurer registered the run's own extent is 1.2em, below the strut, so
/// the strut decides — headless and Skia builds then agree, which they do not
/// today. Where a measurer is registered the two can disagree: real "xxx" ink at
/// 16px is about 9px, so the strut is doing the work there too.
pub(crate) fn line_box_height(m: &MeasuredText, strut: &FontMetrics) -> i32 {
    m.line_extent().max(strut.ascent + strut.descent).round() as i32
}

/// Legacy heuristic width single-char estimate for divergence test — old 0.6 ratio.
#[allow(dead_code)]
fn measure_heuristic_old(text: &str, font_size_px: f32) -> f32 {
    font_size_px * 0.6 * text.chars().count() as f32
}

/// The glyph a truncated run ends in. CSS calls it a "text-overflow ellipsis",
/// which U+2026 HORIZONTAL ELLIPSIS is.
pub(crate) const ELLIPSIS: &str = "\u{2026}";

/// One measured fragment offered for truncation.
///
/// The font is per fragment, not per call: an inline run can mix sizes, and each
/// piece has to be measured in its own. The ellipsis is measured in the font of
/// the fragment it joins, which is where a browser puts it.
pub(crate) struct MeasurableFragment {
    pub text: String,
    /// The width `text` measures, from the caller's own measurement.
    pub width: f32,
    pub font_size: f32,
    pub font_family: String,
}

/// Truncate an already-measured sequence of text fragments to `max_width`,
/// returning how many characters of each survive, or `None` when they all fit.
///
/// The ellipsis is reserved space: a character is kept only if it AND the
/// ellipsis fit, so a truncated run never overflows the box it is clipped in.
/// CSS 2.1 §11.1.1.
///
/// Characters are measured cumulatively within a fragment, so a split never
/// lands mid-cluster, and fragment widths are SUMMED across fragments: the seam
/// measures a string and this crate has no shaping cache, so there is nothing to
/// measure the joined string with. This is the same approximation the inline
/// formatting context's line fill makes, which is why they can share the
/// decision.
///
/// This is the ONE implementation of "where does the ellipsis go". The inline
/// formatting context in `layout.rs` fills lines from a flattened run of
/// fragments belonging to different elements; this module wraps a single string.
/// They used to decide independently, and when R-5b replaced the block path's
/// only caller of [`wrap_text_with_style`] with that context,
/// `text-overflow: ellipsis` stopped reaching the layout tree while the pixels
/// stayed right, because the renderer truncates again at paint time. Two
/// implementations of a decision the layout tree depends on is how that
/// happened, and it survived review because a green test on this function said
/// nothing about the live path.
pub(crate) fn truncate_fragments_with_ellipsis(
    fragments: &[MeasurableFragment],
    max_width: f32,
    scale: f32,
) -> Option<Vec<usize>> {
    if fragments.is_empty() {
        return None;
    }
    if max_width <= 0.0 {
        return Some(vec![0; fragments.len()]);
    }
    if fragments.iter().map(|f| f.width).sum::<f32>() <= max_width {
        return None;
    }
    // `done` is the width of the fragments ALREADY FINISHED. `prefix` below is a
    // fragment's own cumulative width, so `done + prefix` is the total -- the two
    // must not be conflated, and `done` moves once per fragment, not per
    // character. Conflating them truncated a fragment short of where it fitted.
    let mut kept = vec![0usize; fragments.len()];
    let mut done = 0.0f32;
    for (fi, fragment) in fragments.iter().enumerate() {
        let snapped = snapped_size(fragment.font_size, scale);
        let ellipsis_w = measure_text_internal(ELLIPSIS, snapped, &fragment.font_family, scale);
        let mut cur = String::new();
        let mut width = 0.0f32;
        for ch in fragment.text.chars() {
            cur.push(ch);
            let next = measure_text_internal(&cur, snapped, &fragment.font_family, scale);
            if done + next + ellipsis_w > max_width {
                break;
            }
            kept[fi] += 1;
            width = next;
        }
        done += width;
        if kept[fi] < fragment.text.chars().count() {
            break;
        }
    }
    Some(kept)
}

/// Truncate `text` to fit `max_width` px, appending "…" when overflow.
///
/// One fragment, delegated to [`truncate_fragments_with_ellipsis`], which is
/// where the rule lives. See that function for why there is only one.
fn truncate_with_ellipsis(
    text: &str,
    max_width: f32,
    font_size_px: f32,
    font_family: &str,
    scale: f32,
) -> String {
    if max_width <= 0.0 || text.is_empty() {
        return String::new();
    }
    let snapped = snapped_size(font_size_px, scale);
    let fragment = MeasurableFragment {
        text: text.to_string(),
        width: measure_text_internal(text, snapped, font_family, scale),
        font_size: font_size_px,
        font_family: font_family.to_string(),
    };
    let Some(kept) =
        truncate_fragments_with_ellipsis(std::slice::from_ref(&fragment), max_width, scale)
    else {
        return text.to_string();
    };
    let kept: String = text.chars().take(kept[0]).collect();
    format!("{kept}{ELLIPSIS}")
}

/// Measured wrap of ONE string with no elements in it, always in collapsing mode
/// with clipping overflow. See [`wrap_text_with_style`] for why this family is
/// still here and what would retire it.
/// Returns Vec<(line_text, measured_width)>
pub fn wrap_text_measured(
    text: &str,
    max_width: f32,
    font_size_px: f32,
    font_family: &str,
    scale: f32,
) -> Vec<(String, f32)> {
    wrap_text_with_style(
        text,
        max_width,
        font_size_px,
        font_family,
        scale,
        WhiteSpace::Normal,
        TextOverflow::Clip,
    )
}

/// The single-string greedy wrap: white-space and ellipsis aware, for ONE
/// string with no elements in it. Returns `(line text, measured width)`.
///
/// ## Why this is still here, and what would retire it
///
/// R-5b replaced the block path's only caller of the `wrap_text*` family with
/// the inline formatting context in `layout.rs`, because a line must be able to
/// break at the edge of an inline element and a single-string wrapper cannot see
/// an element boundary. That left this path with no production caller and a
/// second greedy-fill implementation beside the real one, which is the same blind
/// spot that hid `text-overflow: ellipsis` regressing silently: the renderer
/// truncates again at paint time, so the pixels stayed right while the layout
/// tree stopped truncating.
///
/// The inline formatting context does NOT call through here and cannot.
/// This returns `Vec<(String, f32)>` — whole lines with no positions, no
/// per-fragment attribution and no `vertical-align` — and the context needs all
/// three, because a fragment's box hangs from the baseline and an element
/// fragmented across two lines has to appear on both. Routing the context
/// through this signature would trade requirement 2 away for tidiness.
///
/// So ONE decision is shared — [`truncate_fragments_with_ellipsis`], where the
/// ellipsis goes, which is the decision C-1 showed must not be made twice — and
/// the rest deliberately is not shared, because it cannot be. What would retire
/// the rest, in the order it should happen:
///
/// 1. The renderer stops re-wrapping text itself and consumes layout's line
///    boxes. [`wrap_text_measured`], [`wrap_text`], [`wrap_text_with_options`]
///    and this all fall out of production in the same change.
/// 2. Something outside this crate wants to lay out a single string, in which
///    case this is public API rather than dead code and this comment is the
///    wrong comment.
/// 3. Neither. Then the whole family should be DELETED rather than documented a
///    fourth time. It is [`TextLine`] and [`create_text_nodes`] that make
///    deleting it a breaking change, and they are the parts with no argument for
///    existing.
///
/// The dead code this crate's briefs forbid is code that is unreachable AND has
/// no argument for existing. This has an argument. The argument is written down
/// here so that "delete it" is a decision somebody makes on the evidence, rather
/// than an omission somebody notices a year later.
pub fn wrap_text_with_style(
    text: &str,
    max_width: f32,
    font_size_px: f32,
    font_family: &str,
    scale: f32,
    white_space: WhiteSpace,
    text_overflow: TextOverflow,
) -> Vec<(String, f32)> {
    let snapped = snapped_size(font_size_px, scale);
    let ellipsis = text_overflow == TextOverflow::Ellipsis;
    let limit = if max_width <= 0.0 {
        f32::INFINITY
    } else {
        max_width
    };

    // Helpers
    let m = |s: &str| measure_text_internal(s, snapped, font_family, scale);

    match white_space {
        WhiteSpace::Nowrap => {
            // Single line — no wrapping; ellipsis if overflow
            if ellipsis && m(text) > limit {
                let truncated =
                    truncate_with_ellipsis(text, limit, font_size_px, font_family, scale);
                let w = m(&truncated);
                vec![(truncated, w)]
            } else {
                vec![(text.to_string(), m(text))]
            }
        }
        WhiteSpace::Pre => {
            // Preserve \n and spaces; no wrapping, each \n segment is a line
            let mut out = Vec::new();
            for para in text.split('\n') {
                // Keep para as-is (including empty)
                if ellipsis && m(para) > limit {
                    let tr = truncate_with_ellipsis(para, limit, font_size_px, font_family, scale);
                    let w = m(&tr);
                    out.push((tr, w));
                } else {
                    out.push((para.to_string(), m(para)));
                }
            }
            if out.is_empty() {
                out.push((String::new(), 0.0));
            }
            out
        }
        WhiteSpace::PreWrap => {
            // Preserve \n, but wrap each paragraph normally
            let mut out = Vec::new();
            for para in text.split('\n') {
                if para.is_empty() {
                    out.push((String::new(), 0.0));
                    continue;
                }
                // Wrap this paragraph with normal word iteration but keep spaces collapsed per word split?
                // For pre-wrap we preserve spaces/tabs inside para but still wrap at width.
                // Simplify: split on whitespace but keep single-space collapse for measure, then re-join.
                // Preserve that para may have leading/trailing spaces measured; we treat split_whitespace.
                let words: Vec<&str> = para.split_whitespace().collect();
                if words.is_empty() {
                    out.push((para.to_string(), m(para)));
                    continue;
                }
                let mut current = String::new();
                let mut current_w = 0.0;
                for word in words {
                    let candidate = if current.is_empty() {
                        word.to_string()
                    } else {
                        format!("{} {}", current, word)
                    };
                    let candidate_w = m(&candidate);
                    if candidate_w <= limit || current.is_empty() {
                        current = candidate;
                        current_w = candidate_w;
                    } else {
                        out.push((current.clone(), current_w));
                        current = word.to_string();
                        current_w = m(&current);
                    }
                }
                if !current.is_empty() {
                    out.push((current, current_w));
                }
            }
            if out.is_empty() {
                out.push((String::new(), 0.0));
            }
            // ellipsis on single-line pre-wrap? If single para and ellipsis, truncate each line that overflows
            if ellipsis && out.len() == 1 && m(&out[0].0) > limit {
                let tr = truncate_with_ellipsis(&out[0].0, limit, font_size_px, font_family, scale);
                let w = m(&tr);
                out[0] = (tr, w);
            }
            out
        }
        WhiteSpace::Normal | WhiteSpace::PreLine => {
            // Normal: collapse whitespace, wrap by words, preserve \n as paragraph breaks
            let mut lines: Vec<(String, f32)> = Vec::new();
            for para in text.split('\n') {
                if para.trim().is_empty() {
                    // For normal, empty para from \n produces empty line (pre-wrap-like) but collapsed?
                    // Preserve paragraph break as empty line for pre-line; for normal, skip consecutive?
                    // We'll push empty for each \n beyond first? Simpler: if para empty and it's not the only, push empty.
                    if text.contains('\n') {
                        lines.push((String::new(), 0.0));
                    }
                    continue;
                }
                let mut current = String::new();
                let mut current_w = 0.0;
                for word in para.split_whitespace() {
                    let candidate = if current.is_empty() {
                        word.to_string()
                    } else {
                        format!("{} {}", current, word)
                    };
                    let candidate_w = m(&candidate);
                    if candidate_w <= limit || current.is_empty() {
                        current = candidate;
                        current_w = candidate_w;
                    } else {
                        lines.push((current, current_w));
                        current = word.to_string();
                        current_w = m(&current);
                    }
                }
                if !current.is_empty() {
                    lines.push((current, current_w));
                }
            }
            if lines.is_empty() {
                lines.push((String::new(), 0.0));
            }
            // Single-line ellipsis handling: when nowrap would be inferred? For normal with ellipsis, only if wrapping disabled — we are in Normal, so ellipsis not applied unless we detect single-line overflow? Brief says if ellipsis and single-line overflow, truncate.
            // For Normal, ellipsis only meaningful when rendered as single line (e.g., white-space:nowrap + overflow ellipsis). So we keep lines as-is.
            // However if caller passed ellipsis=true with Normal and text would be single line that overflows, we should truncate to single line.
            if ellipsis && lines.len() == 1 && m(&lines[0].0) > limit {
                let tr =
                    truncate_with_ellipsis(&lines[0].0, limit, font_size_px, font_family, scale);
                let w = m(&tr);
                lines[0] = (tr, w);
            } else if ellipsis && lines.len() > 1 {
                // still truncate last? No, per spec single-line only.
            }
            lines
        }
    }
}

/// Three-argument legacy wrapper: one string, an `i32` limit, a font size, no
/// style. See [`wrap_text_with_style`] for why this family is still here.
pub fn wrap_text(text: &str, max_width: i32, font_size_px: f32) -> Vec<TextLine> {
    let max_w = max_width as f32;
    let scale = current_scale();
    let measured = wrap_text_measured(text, max_w, font_size_px, "system-ui", scale);
    let metrics = FontMetrics::from_font_size(font_size_px);
    let line_height = metrics.line_height.round() as i32;
    // Fallback if measured empty?
    if measured.is_empty() {
        return vec![TextLine {
            text: String::new(),
            width: 0,
            height: line_height,
        }];
    }
    let snapped = snapped_size(font_size_px, scale);
    measured
        .into_iter()
        .map(|(s, w)| {
            // The line's height comes from the run that is actually in it, not
            // from the font's nominal line height: two lines whose text reaches
            // different heights get different heights.
            let run = measure_text_metrics(&s, snapped, "system-ui", scale);
            TextLine {
                text: s,
                width: w.round() as i32,
                height: line_box_height(&run, &metrics),
            }
        })
        .collect()
}

/// As [`wrap_text_with_style`], with an `i32` limit, a `WhiteSpace`, a
/// `TextOverflow` and a [`TextLine`] out. The last production caller of this was
/// the block child loop, and the inline formatting context is what replaced it.
/// See [`wrap_text_with_style`] for why this family is still here.
pub fn wrap_text_with_options(
    text: &str,
    max_width: i32,
    font_size_px: f32,
    font_family: &str,
    scale: f32,
    white_space: WhiteSpace,
    text_overflow: TextOverflow,
) -> Vec<TextLine> {
    let measured = wrap_text_with_style(
        text,
        max_width as f32,
        font_size_px,
        font_family,
        scale,
        white_space,
        text_overflow,
    );
    let metrics = FontMetrics::from_font_size(snapped_size(font_size_px, scale));
    let line_height = metrics.line_height.round() as i32;
    let snapped = snapped_size(font_size_px, scale);
    if measured.is_empty() {
        return vec![TextLine {
            text: String::new(),
            width: 0,
            height: line_height,
        }];
    }
    measured
        .into_iter()
        .map(|(s, w)| {
            let run = measure_text_metrics(&s, snapped, font_family, scale);
            TextLine {
                text: s,
                width: w.round() as i32,
                height: line_box_height(&run, &metrics),
            }
        })
        .collect()
}

/// One [`LayoutNode`] per wrapped line, for a single string with no elements in
/// it. It wraps with [`wrap_text`], so it has NO `vertical-align`, no baseline
/// alignment and no `text-overflow` post-pass: this is pre-IFC behaviour.
///
/// This is the part of the family with the weakest claim to existing. A caller
/// wanting inline formatting has to use `compute_layout`; a caller wanting what
/// this gives should first ask whether it is what they want. `source_index` is a
/// parameter here, so a caller CAN produce a tree the renderer will resolve, and
/// nothing in this crate does. See [`wrap_text_with_style`] for the record.
pub fn create_text_nodes(
    text: &str,
    x: i32,
    y: i32,
    max_width: i32,
    font_size_px: f32,
    source_index: Option<usize>,
) -> Vec<LayoutNode> {
    let lines = wrap_text(text, max_width, font_size_px);
    let mut nodes = Vec::new();
    let mut cur_y = y;

    for line in lines {
        nodes.push(LayoutNode {
            rect: Rect {
                x,
                y: cur_y,
                w: line.width,
                h: line.height,
            },
            z_index: 0,
            display_none: false,
            source_index,
            scroll_x: 0,
            scroll_y: 0,
            clip: None,
            stacking_context: false,
            scroll_height: line.height,
            max_scroll_y: 0,
            scrollable: false,
            children: vec![],
        });
        cur_y += line.height;
    }

    nodes
}
