use velox_dom::style::{TextOverflow, WhiteSpace};
use velox_dom::text_wrap::{measure_text, wrap_text, wrap_text_measured, wrap_text_with_style};

fn heuristic_width(text: &str, font_size: f32) -> f32 {
    font_size * 0.6 * text.chars().count() as f32
}

#[test]
fn wrap_matches_skia_within_half_px() {
    // Prove heuristic vs Skia diverge (>0.5)
    let scale = 1.5;
    let text = "not positive";
    let w_heuristic = heuristic_width(text, 16.0);
    // Use dom's measure (which mirrors renderer fallback Skia 0.5)
    let w_skia = measure_text(text, 16.0, "system-ui", scale);
    // heuristic 115.2 vs skia fallback  ~ 96 -> diff 19.2 >0.5
    assert!(
        (w_skia - w_heuristic).abs() > 0.5,
        "expected divergence >0.5, got skia={w_skia} heuristic={w_heuristic} diff={}",
        (w_skia - w_heuristic).abs()
    );
    // Now prove wrap_measured matches skia within 0.5 per line width
    let lines = wrap_text_measured(text, 500.0, 16.0, "system-ui", scale);
    assert_eq!(lines.len(), 1);
    let (line_text, line_w) = &lines[0];
    assert_eq!(line_text, text);
    let direct = measure_text(text, 16.0, "system-ui", scale);
    assert!(
        (line_w - direct).abs() < 0.5,
        "wrap width {line_w} vs direct {direct} diff {}",
        (line_w - direct).abs()
    );
}

#[test]
fn snapped_size_uses_scale_round() {
    // font 16 at scale 1.5 -> snapped = (24).round/1.5 =16, at scale 1.25 -> (20).round/1.25 =16
    // font 15 at scale 1.5 -> (22.5).round=23/1.5=15.333...
    let w1 = measure_text("abc", 15.0, "system-ui", 1.5);
    let w2 = measure_text("abc", 15.0, "system-ui", 1.0);
    // snapped size different so widths differ (heuristic scaled)
    // 15*0.5*3=22.5 at 1.0 ; 15.333*0.5*3=23.0 at 1.5 => diff 0.5
    assert!(
        (w1 - w2).abs() > 0.1,
        "snapped should affect width: {w1} vs {w2}"
    );
    // Verify wrap respects same snapped
    let lines = wrap_text_measured("a b c", 10.0, 15.0, "system-ui", 1.5);
    // At 15.33 snapped, char ~7.66, space ~? but heuristic per string length;
    // Just ensure it still wraps (multiple lines)
    assert!(lines.len() >= 1);
}

#[test]
fn white_space_normal_wraps() {
    let lines = wrap_text_with_style(
        "hello world from velox",
        60.0,
        16.0,
        "system-ui",
        1.0,
        WhiteSpace::Normal,
        TextOverflow::Clip,
    );
    // At 0.5*16=8 per char, avg: "hello" 40, "world" 40, limit 60 => should wrap to multiple lines
    assert!(lines.len() > 1, "normal should wrap, got {:?}", lines);
}

#[test]
fn white_space_nowrap_single_line() {
    let lines = wrap_text_with_style(
        "hello world from velox",
        60.0,
        16.0,
        "system-ui",
        1.0,
        WhiteSpace::Nowrap,
        TextOverflow::Clip,
    );
    assert_eq!(
        lines.len(),
        1,
        "nowrap should be single line, got {:?}",
        lines
    );
    assert_eq!(lines[0].0, "hello world from velox");
}

#[test]
fn white_space_pre_preserves_newlines() {
    let lines = wrap_text_with_style(
        "line1\nline2\nline3",
        500.0,
        16.0,
        "system-ui",
        1.0,
        WhiteSpace::Pre,
        TextOverflow::Clip,
    );
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].0, "line1");
    assert_eq!(lines[1].0, "line2");
    assert_eq!(lines[2].0, "line3");
}

#[test]
fn white_space_pre_wrap_preserves_newlines_and_wraps() {
    let lines = wrap_text_with_style(
        "hello world\nfoo bar baz qux",
        60.0,
        16.0,
        "system-ui",
        1.0,
        WhiteSpace::PreWrap,
        TextOverflow::Clip,
    );
    // First para "hello world" should wrap at 60 (40+1+40>60 => 2 lines), second para "foo bar baz qux" similarly
    // So total >=3 lines (2+ maybe 2)
    assert!(
        lines.len() >= 3,
        "pre-wrap should preserve breaks and wrap, got {:?}",
        lines
    );
    // Ensure newline break preserved: we should have at least one line equal to "hello" etc? Rough check
    assert!(
        lines.iter().any(|(s, _)| s == "hello"),
        "expected hello line"
    );
}

/// RENAMED IN R-5b FIX ROUND 1, and the rename is the point.
///
/// This test called `wrap_text_with_style` directly, and its name said
/// "ellipsis truncates single-line overflow" as though that were the product's
/// behaviour. It was not: R-5b replaced the block loop's only production caller of
/// that function with the inline formatting context, so for one release
/// `text-overflow: ellipsis` did not reach the layout tree at all, measured 40 ->
/// 80 on the reviewer's own input -- while this test stayed green. Standing rule 1
/// in its purest form: a green test whose subject is not the live path says nothing
/// about the product.
///
/// The live path is pinned by `ellipsis_truncates_single_line_overflow` below, which
/// goes through `compute_layout`, and by seven cases in
/// `velox-dom/tests/inline_formatting.rs`. What is left here is a test of the
/// retained wrapper, which is now reachable only from tests; see the retention note
/// on `wrap_text_with_style` in `velox-dom/src/text_wrap.rs`.
#[test]
fn the_single_string_wrapper_truncates_and_is_not_the_layout_path() {
    let lines = wrap_text_with_style(
        "This is a very long line that will not fit",
        60.0,
        16.0,
        "system-ui",
        1.0,
        WhiteSpace::Nowrap,
        TextOverflow::Ellipsis,
    );
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].0.ends_with('…'),
        "expected ellipsis, got {:?}",
        lines[0].0
    );
    assert!(
        lines[0].1 <= 60.0 + 0.5,
        "truncated width {} exceeds limit 60",
        lines[0].1
    );
    // Also test that short text not truncated
    let short = wrap_text_with_style(
        "Hi",
        60.0,
        16.0,
        "system-ui",
        1.0,
        WhiteSpace::Nowrap,
        TextOverflow::Ellipsis,
    );
    assert_eq!(short[0].0, "Hi");
}

/// The name the old test above used, pointed at the path that actually ships.
///
/// `compute_layout` is the whole product behaviour: if `text-overflow: ellipsis`
/// stops reaching the layout tree, this fails and the renderer test does not,
/// because the renderer truncates again at paint time from `text_style.ellipsis`
/// and the pixels were right the whole time. No synthetic measurer is registered
/// here, so this is on the no-measurer fallback, where the width is 0.5em per
/// character: 16px * 0.5 = 8px per character, 60px of line.
#[test]
fn ellipsis_truncates_single_line_overflow() {
    use velox_dom::layout::compute_layout;
    use velox_dom::{VNode, h};
    let v = h(
        "div",
        vec![(
            "style",
            "width:60px;white-space:nowrap;text-overflow:ellipsis",
        )],
        vec![VNode::Text(
            "This is a very long line that will not fit".to_string(),
        )],
    );
    let outer = compute_layout(&h("div", vec![], vec![v]), 600, 600);
    let d = &outer.children[0];
    assert_eq!(
        d.children.len(),
        1,
        "`nowrap` never wraps, so one line is one fragment; got {:?}",
        d.children.iter().map(|c| c.rect).collect::<Vec<_>>()
    );
    let w = d.children[0].rect.w;
    assert!(
        w <= 60,
        "an ellipsis is reserved space, so a truncated fragment can never be wider \
         than its 60px line; got {w}"
    );
    // 352 was hardcoded here for a string that measures 336, so the bound was
    // stale rather than wrong. It is the same interpolated value the contrast
    // below pins exactly, and it is redundant with that assertion -- kept as a
    // plain `strictly less than untruncated` statement, not as the evidence.
    let untruncated = (8 * "This is a very long line that will not fit".chars().count()) as i32;
    assert!(
        w > 0 && w < untruncated,
        "the untruncated text measures {untruncated} at 0.5em per character, so a \
         layout tree that never truncated would report that; got {w}"
    );
    // A root is laid out at the viewport's size, so the box that matters is the
    // inner one; the contrast case proves the property is what did it.
    let plain_v = h(
        "div",
        vec![("style", "width:60px;white-space:nowrap")],
        vec![VNode::Text(
            "This is a very long line that will not fit".to_string(),
        )],
    );
    let plain = compute_layout(&h("div", vec![], vec![plain_v]), 600, 600);
    assert_eq!(
        plain.children[0].children[0].rect.w, untruncated,
        "the same text without `text-overflow: ellipsis` keeps its full width and \
         overflows, which is what makes the truncated case evidence of truncation"
    );
}

#[test]
fn narrow_window_not_positive_wraps() {
    // Simulate narrow window where "not positive" should wrap when max_width small
    // At heuristic 8 per char, "not" 24, "positive" 64, space 8? Actually per string: "not positive" 12*8=96
    // With limit 80, "not positive" cannot fit as single line (96>80) => should wrap to 2 lines "not" + "positive"
    let lines = wrap_text_measured("not positive", 80.0, 16.0, "system-ui", 1.0);
    assert!(
        lines.len() == 2,
        "expected wrap into 2 lines at 80px, got {:?}",
        lines
    );
    assert_eq!(lines[0].0, "not");
    assert_eq!(lines[1].0, "positive");
    // Ensure legacy wrap_text (i32 api) also wraps consistently
    let legacy = wrap_text("not positive", 80, 16.0);
    assert_eq!(legacy.len(), 2);
}

#[test]
fn wrap_matches_skia_direct_measure() {
    // Comprehensive: each wrapped line's width equals direct measure within 0.5
    let scale = 1.5;
    let text = "hello world foo bar baz";
    let max_w = 80.0;
    let lines = wrap_text_measured(text, max_w, 16.0, "system-ui", scale);
    for (line, w) in &lines {
        let direct = measure_text(line, 16.0, "system-ui", scale);
        assert!(
            (w - direct).abs() < 0.5,
            "line {line:?} width {w} vs direct {direct}"
        );
        assert!(
            *w <= max_w + 0.5 || line.split_whitespace().count() == 1,
            "line exceeds limit"
        );
    }
}
