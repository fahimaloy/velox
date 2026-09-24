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
static SKIA_MEASURER: RwLock<Option<fn(&str, f32, &str, f32) -> f32>> = RwLock::new(None);
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

/// Register a Skia-backed measurer (called by velox-renderer at init).
pub fn set_skia_measurer(f: fn(&str, f32, &str, f32) -> f32) {
    if let Ok(mut g) = SKIA_MEASURER.write() {
        *g = Some(f);
    }
}

fn measure_text_internal(text: &str, font_size_px: f32, font_family: &str, scale: f32) -> f32 {
    if let Ok(g) = SKIA_MEASURER.read() {
        if let Some(f) = *g {
            return f(text, font_size_px, font_family, scale);
        }
    }
    measure_heuristic(text, font_size_px, scale)
}

/// Public measure for tests / layout: snapped, family-aware (family ignored in heuristic).
pub fn measure_text(text: &str, font_size_px: f32, font_family: &str, scale: f32) -> f32 {
    measure_text_internal(text, font_size_px, font_family, scale)
}

/// Legacy heuristic width single-char estimate for divergence test — old 0.6 ratio.
#[allow(dead_code)]
fn measure_heuristic_old(text: &str, font_size_px: f32) -> f32 {
    font_size_px * 0.6 * text.chars().count() as f32
}

/// Truncate `text` to fit `max_width` px, appending "…" when overflow.
fn truncate_with_ellipsis(
    text: &str,
    max_width: f32,
    font_size_px: f32,
    font_family: &str,
    scale: f32,
) -> String {
    const ELLIPSIS: &str = "\u{2026}";
    if max_width <= 0.0 || text.is_empty() {
        return String::new();
    }
    let snapped = snapped_size(font_size_px, scale);
    // measure with internal (Skia or fallback)
    if measure_text_internal(text, snapped, font_family, scale) <= max_width {
        return text.to_string();
    }
    let ellipsis_w = measure_text_internal(ELLIPSIS, snapped, font_family, scale);
    let avail = (max_width - ellipsis_w).max(0.0);
    let mut cur = String::new();
    for ch in text.chars() {
        let candidate = format!("{cur}{ch}");
        if measure_text_internal(&candidate, snapped, font_family, scale) <= avail {
            cur = candidate;
        } else {
            break;
        }
    }
    format!("{cur}{ELLIPSIS}")
}

/// Core Skia-backed wrap: white-space + ellipsis aware.
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

/// Legacy wrapper kept for layout.rs: `wrap_text(text, max_width:i32, font_size_px:f32) -> Vec<TextLine>`
/// Delegates to Skia-backed measured version with default family "system-ui" and current viewport scale.
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
    measured
        .into_iter()
        .map(|(s, w)| TextLine {
            text: s,
            width: w.round() as i32,
            height: line_height,
        })
        .collect()
}

/// Extended wrap for layout that respects style white-space/ellipsis extracted elsewhere.
/// Used by layout.rs when it parses style string for those props.
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
    if measured.is_empty() {
        return vec![TextLine {
            text: String::new(),
            width: 0,
            height: line_height,
        }];
    }
    measured
        .into_iter()
        .map(|(s, w)| TextLine {
            text: s,
            width: w.round() as i32,
            height: line_height,
        })
        .collect()
}

/// Create layout nodes for wrapped text lines
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
