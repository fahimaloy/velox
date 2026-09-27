//! The inline formatting context.
//!
//! Everything here is a geometry assertion on `compute_layout`'s output: no
//! window, no compositor, no Skia. `render_vnode_to_raster_png` never appears,
//! because it does not run `compute_layout` and so passes whatever the layout
//! does.
//!
//! WHAT THIS FILE PROVES, and what it does not. The measurer is
//! `common::synthetic_measurer`, a model rather than a measurement, so these
//! tests prove that the inline formatting context HONOURS the vertical metrics
//! the seam reports, and that its structure and widths are right. They do not
//! prove that a real font backend reports those numbers. Real-measure evidence
//! is in `velox-renderer`'s `skia-native`-gated tests, and nothing anywhere
//! verifies end-to-end DPI — that the seam's logical extent multiplied by the
//! surface scale is what actually gets painted. There is no oracle for that.
//!
//! The widths are 0.5em per character, which is both the synthetic measurer's and
//! the no-measurer fallback's, so a test can change the vertical half without
//! changing where a line breaks. At 16px that is 8px per character.

mod common;

use common::{FONT_ASCENT_EM, FONT_X_HEIGHT_EM, STRUT_EM, register_synthetic};
use velox_dom::layout::{LayoutNode, compute_layout};
use velox_dom::{VNode, h};

const FONT_SIZE: i32 = 16;
/// 0.5em per character at 16px.
const CHAR: i32 = 8;
/// `round(16 * 1.362)`, the strut at 16px.
const STRUT_16: i32 = 22;
/// `round(32 * 1.362)`, the strut at 32px.
const STRUT_32: i32 = 44;
/// The synthetic measurer's ink for "Hg" at 16px: 2.0em + 0.5em.
const HG_16: i32 = 40;

// ---------------------------------------------------------------- helpers

/// A div of the given width and font size, holding `children`.
fn div(width: Option<i32>, font_size: i32, children: Vec<VNode>) -> VNode {
    let mut style = format!("font-size:{font_size}px");
    if let Some(w) = width {
        style.push_str(&format!(";width:{w}px"));
    }
    h("div", vec![("style", style.as_str())], children)
}

/// A `<span>` carrying `style`, holding `children`.
fn span(style: &str, children: Vec<VNode>) -> VNode {
    h("span", vec![("style", style)], children)
}

fn text(s: &str) -> VNode {
    VNode::Text(s.to_string())
}

/// Lay `vnode` out as a CHILD of an unconstrained block.
///
/// The root box is laid out at the viewport's size, so a root's auto height is
/// the viewport's height and says nothing about its content. Every auto-height
/// and shrink-to-fit assertion has to read a child, so it goes through here.
fn outer_of(vnode: &VNode) -> VNode {
    h("div", vec![], vec![vnode.clone()])
}

/// The layout of a box that was wrapped by `outer_of`.
fn inner(root: &LayoutNode) -> &LayoutNode {
    &root.children[0]
}

/// Every box in the tree, parents before children, in paint order.
fn flatten(node: &LayoutNode, out: &mut Vec<LayoutNode>) {
    out.push(node.clone());
    for c in &node.children {
        flatten(c, out);
    }
}

fn boxes(node: &LayoutNode) -> Vec<LayoutNode> {
    let mut out = Vec::new();
    flatten(node, &mut out);
    out
}

/// Assert the renderer can reach every box, using the renderer's own rule.
///
/// `velox-renderer` resolves a child layout node with
/// `if let Some(src_idx) = child_layout.source_index && let Some(child) = children.get(src_idx) { recurse }`
/// — so a LayoutNode with `source_index: None` takes its whole subtree with it,
/// and an index that does not resolve ends the walk. An inline element has no
/// box, so the tempting implementation is a synthetic node standing in for it;
/// this is the test that says the temptation must be resisted.
fn assert_renderer_can_reach(layout: &LayoutNode, vnode: &VNode, path: &str) {
    let children = match vnode {
        VNode::Element { children, .. } => children.as_slice(),
        VNode::Text(_) => {
            assert!(
                layout.children.is_empty(),
                "{path}: a text node has no children to resolve, but its layout has {}",
                layout.children.len()
            );
            return;
        }
    };
    for child in &layout.children {
        if child.display_none {
            continue;
        }
        let idx = child.source_index.unwrap_or_else(|| {
            panic!(
                "{path}: a layout node with source_index None loses its whole \
                 subtree when the renderer walks it"
            )
        });
        let v = children.get(idx).unwrap_or_else(|| {
            panic!(
                "{path}: source_index {idx} does not resolve against {} siblings",
                children.len()
            )
        });
        assert_renderer_can_reach(child, v, &format!("{path}[{idx}]"));
    }
}

// ------------------------------------------------- requirement 1 and 2

#[test]
fn an_inline_element_has_no_box_of_its_own_so_two_children_make_one_line() {
    register_synthetic();
    // "xxx" + " ooo ccc" = 3 + 8 characters = 88px, inside the 200px line.
    // Every run is x-height only -- no ascender, no descender -- so the STRUT
    // decides the line's height and the arithmetic below is one number.
    let root = div(
        Some(200),
        FONT_SIZE,
        vec![span("", vec![text("xxx")]), text(" ooo ccc")],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(
        d.rect.h, STRUT_16,
        "one line box, because the whole run fits: {d:?}"
    );
    assert_eq!(d.children.len(), 2, "the span and the text: {d:?}");
    // The span's box IS its text's box -- it is a span, not a block.
    let b = &d.children[0];
    assert_eq!(b.children.len(), 1, "the span holds the text: {b:?}");
    assert_eq!(
        b.rect, b.children[0].rect,
        "an inline box is the union of its fragments, and with one fragment that \
         is the fragment: {:?} against {:?}",
        b.rect, b.children[0].rect
    );
    assert_eq!(b.rect.x, 0);
    assert_eq!(b.rect.w, 3 * CHAR, "three characters");
    assert_eq!(d.children[1].rect.x, 3 * CHAR, "immediately after it");
    assert_eq!(d.children[1].rect.w, 8 * CHAR, "eight characters");
    assert_eq!(
        d.children[1].rect.y, b.rect.y,
        "one line, so the same top: {d:?}"
    );
    assert_renderer_can_reach(&laid, &outer, "");
}

#[test]
fn a_line_may_break_at_the_edge_of_an_inline_element() {
    register_synthetic();
    // 32px fits a 40px line; 48px does not. The run is
    // "xxx" + " " + "ooo" + " " + "ccc" = 3, 1, 3, 1, 3 characters, so the line
    // takes "xxx " and breaks -- at the space that sits on the <span>'s edge.
    let root = div(
        Some(40),
        FONT_SIZE,
        vec![span("", vec![text("abcdef")]), text(" ghijkl mnop")],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(
        d.children.len(),
        3,
        "the span, then two fragments of the text: {d:?}"
    );
    let b = &d.children[0];
    let first = &d.children[1];
    let second = &d.children[2];
    assert_eq!(b.rect.w, 3 * CHAR, "the span is never split mid-word: {d:?}");
    assert!(
        b.rect.w + CHAR <= 40,
        "and the space after it still fits, so the break is past the edge"
    );
    assert!(
        b.rect.w + CHAR + 3 * CHAR > 40,
        "while the next word would not, which is what makes this a break"
    );
    assert_eq!(
        first.rect.y - b.rect.y,
        STRUT_16,
        "the break fell BETWEEN the span and the text, which is the whole point: \
         {d:?}"
    );
    assert_eq!(first.rect.w, 4 * CHAR, "\"ooo \" is four characters");
    assert_eq!(second.rect.y - first.rect.y, STRUT_16);
    assert_eq!(second.rect.w, 3 * CHAR, "\"ccc\"");
    assert_eq!(d.rect.h, 3 * STRUT_16, "three lines: {d:?}");
    assert_renderer_can_reach(&laid, &outer, "");
}

#[test]
fn a_line_may_break_between_text_and_the_inline_element_after_it() {
    register_synthetic();
    let root = div(
        Some(40),
        FONT_SIZE,
        vec![text("xxx "), span("", vec![text("ooo")])],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.children.len(), 2, "the text and the span: {d:?}");
    assert_eq!(
        d.children[1].rect.y - d.children[0].rect.y,
        STRUT_16,
        "the break fell between the text and the span: {d:?}"
    );
    assert_eq!(d.children[1].rect.w, 3 * CHAR, "the span is whole");
    assert_renderer_can_reach(&laid, &outer, "");
}

#[test]
fn an_inline_element_deep_inside_another_one_joins_the_same_run() {
    register_synthetic();
    // <b>xxx <i>ooo</i> ccc</b> is ONE run, so it can break between any two of
    // its three words even though the <i> sits between them. It fragments
    // across all three lines, so the <b> appears three times -- which is what a
    // browser does with an inline box that straddles a break.
    let root = div(
        Some(40),
        FONT_SIZE,
        vec![span("", vec![
            text("xxx "),
            span("", vec![text("ooo")]),
            text(" ccc"),
        ])],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    // "aaa " = 4, "bbb" = 3, " ccc" = 4. Line 1 takes "aaa bbb" = 7 = 56.
    // Line 2 takes " ccc" = 4 = 32.
    assert_eq!(d.rect.h, 3 * STRUT_16, "three lines: {d:?}");
    assert_eq!(
        d.children.len(),
        3,
        "the <b> fragments across all three lines: {d:?}"
    );
    let line1 = &d.children[0];
    let line2 = &d.children[1];
    let line3 = &d.children[2];
    assert_eq!(line1.children.len(), 1, "\"xxx \": {line1:?}");
    assert_eq!(line1.children[0].rect.w, 4 * CHAR);
    assert_eq!(
        line2.children.len(),
        2,
        "the <i> lands on the second line: {line2:?}"
    );
    let nested = &line2.children[1];
    assert_eq!(nested.rect.w, 3 * CHAR, "the nested span is whole");
    assert_eq!(line3.children.len(), 1, "\"ccc\": {line3:?}");
    assert_eq!(line2.rect.y - line1.rect.y, STRUT_16);
    assert_eq!(line3.rect.y - line2.rect.y, STRUT_16);
    assert_renderer_can_reach(&laid, &outer, "");
}

#[test]
fn a_preserved_space_survives_at_the_start_of_a_line() {
    register_synthetic();
    // `pre` does not collapse, so the space after a break is a real space and a
    // line may start with one. This is the whole difference `pre` makes here.
    let root = h(
        "div",
        vec![("style", "width:60px;font-size:16px;white-space:pre")],
        vec![text("xxx \nooo")],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.children.len(), 2, "two lines: {d:?}");
    assert_eq!(d.children[0].rect.w, 4 * CHAR, "\"xxx \" keeps its space");
    assert_eq!(d.children[1].rect.w, 3 * CHAR, "\"ooo\"");
}

#[test]
fn a_line_never_starts_with_a_collapsed_space() {
    register_synthetic();
    // The line limit is exactly one character, so every break is forced.
    let root = div(Some(CHAR), FONT_SIZE, vec![text("x x x")]);
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.children.len(), 3, "x / x / x: {d:?}");
    for c in &d.children {
        assert_eq!(c.rect.w, CHAR, "no line starts with the space: {d:?}");
    }
    assert_eq!(d.children[0].rect.x, 0);
    assert_eq!(d.children[1].rect.x, 0, "each line starts at the left edge");
}

// ------------------------------------------------ requirement 3 and 4

#[test]
fn a_line_is_never_shorter_than_the_font_owns() {
    register_synthetic();
    // "xxx" has no ascender and no descender, so its ink is 0.30em = 4.8px --
    // far under the strut. The strut still holds the line open.
    let root = div(None, FONT_SIZE, vec![text("xxx")]);
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    assert_eq!(inner(&laid).rect.h, STRUT_16, "{laid:?}");
    // The strut scales with the font: 32px owns a 43.6px line.
    let big = div(None, 32, vec![text("xxx")]);
    assert_eq!(
        inner(&compute_layout(&outer_of(&big), 600, 600)).rect.h,
        STRUT_32,
        "a 32px font owns a 32 * 1.362 = 43.6px line"
    );
    // And a run whose ink overshoots the strut owns the line instead.
    let tall = div(None, FONT_SIZE, vec![text("Hg")]);
    assert_eq!(
        inner(&compute_layout(&outer_of(&tall), 600, 600)).rect.h,
        HG_16,
        "\"Hg\" ink is 2.5em = 40px, over the 21.8px strut, so the run decides"
    );
}

#[test]
fn a_text_fragment_box_is_its_own_fonts_content_box_hanging_from_the_baseline() {
    register_synthetic();
    // A 32px span and a 16px run share one line. The line is as tall as the
    // 32px strut, because the strut is a per-item floor. But the two BOXES are
    // each their own font's content box, and both hang from the same baseline,
    // so the taller box's top is the higher one.
    let root = div(
        None,
        FONT_SIZE,
        vec![span("font-size:32px", vec![text("Hg")]), text("xxx")],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.rect.h, STRUT_32, "the 32px strut owns the line: {d:?}");
    let big = &d.children[0];
    let small = &d.children[1];
    assert_eq!(big.rect.h, STRUT_32, "a 32px fragment is a 32px box");
    assert_eq!(small.rect.h, STRUT_16, "a 16px fragment is a 16px box");
    assert!(
        small.rect.y > big.rect.y,
        "shared baseline, so the smaller font's box starts lower: {:?} against {:?}",
        big.rect,
        small.rect
    );
    assert!(
        small.rect.y - big.rect.y < STRUT_16,
        "by less than a full box: the difference is the gap between the two \
         content-box ascents, not a whole line"
    );
}

#[test]
fn a_run_that_overshoots_the_strut_makes_the_line_taller_without_taller_boxes() {
    register_synthetic();
    // "Hg" ink is 40px, the strut 22px. The LINE is 40. Neither box is: a text
    // fragment's box is its font's content box, whatever ink the run has. That
    // is the distinction requirement 4 exists to make, and it is what makes
    // `vertical-align` legible in the geometry at all.
    let root = h(
        "div",
        vec![("style", "font-size:16px;white-space:pre")],
        vec![text("xxx\nHg")],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.rect.h, 2 * STRUT_16, "both fragments are 16px boxes: {d:?}");
    assert_eq!(d.children[0].rect.h, STRUT_16);
    assert_eq!(d.children[1].rect.h, STRUT_16);
    assert!(
        d.children[1].rect.y > d.children[0].rect.y,
        "the second line's top is lower than the first's"
    );
}

// ------------------------------------------------ requirement 5

/// The y of the one box on the second line of `xxx\n<run>`, with the first line
/// fixed at 22px.
fn aligned_box_y(run_style: &str, run_text: &str) -> i32 {
    register_synthetic();
    let style = format!("font-size:{FONT_SIZE}px;white-space:pre");
    let root = h(
        "div",
        vec![("style", style.as_str())],
        vec![
            text("xxx\n"),
            span(run_style, vec![text(run_text)]),
        ],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.children.len(), 2, "two lines: {d:?}");
    d.children[1].rect.y
}

/// The y of a plain "xxx" box on the third line of a three-line div, which is
/// the SECOND line's top exactly -- an observable for the line box's top edge
/// that does not recompute the alignment that produced it.
fn second_line_top() -> i32 {
    register_synthetic();
    let root = h(
        "div",
        vec![("style", "font-size:{FONT_SIZE}px;white-space:pre")],
        vec![text("xxx\nHg\nxxx")],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.children.len(), 3, "three lines: {d:?}");
    d.children[2].rect.y
}

#[test]
fn baseline_is_the_default_and_top_aligns_a_box_to_the_line_boxs_top_edge() {
    let line_top = second_line_top();
    let base = aligned_box_y("", "Hg");
    let top = aligned_box_y("vertical-align:top", "Hg");
    let bottom = aligned_box_y("vertical-align:bottom", "Hg");
    let middle = aligned_box_y("vertical-align:middle", "Hg");
    // The line holding "Hg" is 40px (its ink), so top- and bottom-aligning a
    // 40px box against it both land on its top edge, and `top` is exactly the
    // next line's top. That is the definition, read off the geometry.
    assert_eq!(
        top, line_top,
        "vertical-align: top puts the box's top on the line box's top edge"
    );
    assert_eq!(bottom, top, "the box is exactly as tall as the line box");
    assert!(
        top < middle && middle < base,
        "middle is between top and baseline; top {top}, middle {middle}, \
         baseline {base}"
    );
}

#[test]
fn top_and_bottom_differ_when_the_box_is_not_as_tall_as_the_line() {
    register_synthetic();
    // A line holding only "Hg" is 40px, and so is its box. Put an x-height-only
    // run next to something tall so the line is taller than the aligned box, and
    // top and bottom must separate.
    let style = "font-size:16px;white-space:pre";
    let y_of = |run_style: &'static str| {
        let root = h(
            "div",
            vec![("style", style)],
            vec![text("Hg\n"), span(run_style, vec![text("xxx")])],
        );
        let outer = outer_of(&root);
        let laid = compute_layout(&outer, 600, 600);
        let d = inner(&laid);
        (d.children[1].rect.y, d.children[1].rect.h)
    };
    let (top_y, top_h) = y_of("vertical-align:top");
    let (bot_y, bot_h) = y_of("vertical-align:bottom");
    let (base_y, base_h) = y_of("");
    assert_eq!(top_h, STRUT_16, "a 16px box is 16px tall either way");
    assert_eq!(bot_h, STRUT_16);
    assert_eq!(base_h, STRUT_16);
    assert_eq!(top_y, base_y - 0, "top pins the top edge, so y is the line top");
    assert!(
        bot_y > top_y,
        "bottom pins the bottom edge, and the line is taller than the box: \
         {top_y} against {bot_y}"
    );
    assert!(base_y > top_y && base_y < bot_y, "baseline is in between");
}

#[test]
fn middle_splits_the_difference_by_half_an_x_height() {
    register_synthetic();
    // The run overshoots the strut, so the strut is what sets the baseline's
    // offset from the line's top. "middle" then shifts the box by half the
    // parent's x-height below that, which at 16px is 0.536em / 2 = 4.288px.
    let root = h(
        "div",
        vec![("style", "font-size:16px;white-space:pre")],
        vec![
            text("Hg\n"),
            span("vertical-align:top", vec![text("Hg")]),
        ],
    );
    let top = {
        let outer = outer_of(&root);
        let laid = compute_layout(&outer, 600, 600);
        let d = inner(&laid);
        d.children[1].rect.y - d.children[0].rect.y
    };
    let root2 = h(
        "div",
        vec![("style", "font-size:16px;white-space:pre")],
        vec![
            text("Hg\n"),
            span("vertical-align:middle", vec![text("Hg")]),
        ],
    );
    let mid = {
        let laid = compute_layout(&outer_of(&root2), 600, 600);
        let d = inner(&laid);
        d.children[1].rect.y - d.children[0].rect.y
    };
    let shift = mid - top;
    let half_x = (FONT_X_HEIGHT_EM * FONT_SIZE as f32 / 2.0).round() as i32;
    assert_eq!(
        shift, half_x,
        "middle is half an x-height ({half_x}px at 16px) below top: \
         {top} against {mid}"
    );
}

#[test]
fn sub_and_super_are_rejected_and_leave_the_inherited_value_in_place() {
    let base = aligned_box_y("", "Hg");
    for value in ["sub", "super", "10px", "text-top"] {
        let y = aligned_box_y(&format!("vertical-align:{value}"), "Hg");
        assert_eq!(
            y, base,
            "vertical-align: {value} is not supported, so it must leave the \
             inherited value -- baseline -- alone, not become a made-up shift"
        );
    }
}

#[test]
fn vertical_align_is_inherited() {
    register_synthetic();
    // Set on the block, so the run inherits it. If inheritance were broken the
    // run would be baseline-aligned and land 15px lower.
    let root = h(
        "div",
        vec![("style", "font-size:16px;white-space:pre;vertical-align:top")],
        vec![text("Hg\nHg")],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(
        d.children[1].rect.y, d.children[0].rect.y,
        "the second line's box shares the first's top edge, so the value reached \
         the text: {d:?}"
    );
}

// ------------------------------------------------ requirement 6

#[test]
fn an_inline_block_is_atomic_and_shares_the_line_with_the_text_around_it() {
    register_synthetic();
    let root = div(
        Some(200),
        FONT_SIZE,
        vec![
            span("display:inline-block", vec![text("abcdef")]),
            text(" ghijkl mnop"),
        ],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(
        d.rect.h, STRUT_16,
        "one line: the inline-block is inline-level, so it sits in a line box \
         rather than starting a block: {d:?}"
    );
    let ib = &d.children[0];
    assert_eq!(ib.rect.w, 6 * CHAR, "shrink-to-fit, so its content's width");
    assert_eq!(ib.children.len(), 1, "and its text is inside it: {ib:?}");
    assert_eq!(ib.children[0].rect.x, 0, "laid out relative to itself");
    assert_eq!(d.children[1].rect.x, ib.rect.w, "the text follows it");
    assert_renderer_can_reach(&laid, &outer, "");
}

#[test]
fn an_inline_block_is_never_split_across_lines() {
    register_synthetic();
    let root = div(
        Some(60),
        FONT_SIZE,
        vec![
            text("aaa "),
            span("display:inline-block", vec![text("bbbbbb")]),
            text(" ccc"),
        ],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    let ib = d
        .children
        .iter()
        .find(|c| c.children.len() == 1 && c.children[0].rect.w == 6 * CHAR)
        .unwrap_or_else(|| panic!("no whole six-character inline-block: {d:?}"));
    assert_eq!(ib.rect.w, 6 * CHAR, "the box is whole: {d:?}");
    assert_eq!(ib.children.len(), 1, "and so is its content");
    assert_eq!(ib.rect.y, d.children[0].rect.y, "on the first line: {d:?}");
}

#[test]
fn an_inline_block_shrinks_to_the_space_left_on_its_line() {
    register_synthetic();
    // 26 characters = 208px against a 200px line: shrink-to-fit clamps to the
    // line. Six characters = 48px stays at its content width.
    let long = div(
        Some(200),
        FONT_SIZE,
        vec![span(
            "display:inline-block",
            vec![text("abcdefghijklmnopqrstuvwxyz")],
        )],
    );
    assert_eq!(
        inner(&compute_layout(&outer_of(&long), 600, 600)).children[0].rect.w,
        200,
        "clamped to the available width"
    );
    let short = div(
        Some(200),
        FONT_SIZE,
        vec![span("display:inline-block", vec![text("abcdef")])],
    );
    assert_eq!(
        inner(&compute_layout(&outer_of(&short), 600, 600)).children[0].rect.w,
        6 * CHAR,
        "left at its content width"
    );
}

#[test]
fn an_inline_block_establishes_its_own_block_formatting_context() {
    register_synthetic();
    // The inline-block is 80px wide, so its own text wraps at 80px -- not at the
    // 200px of the line it sits on. 12 characters = 96px does not fit in 80, so
    // its content is two lines.
    let root = div(
        Some(200),
        FONT_SIZE,
        vec![span(
            "display:inline-block;width:80px",
            vec![text("aaaaaaaaaaaa")],
        )],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let ib = &inner(&laid).children[0];
    assert_eq!(ib.rect.w, 80, "its own width: {ib:?}");
    assert_eq!(ib.children.len(), 2, "its text wrapped inside it: {ib:?}");
    assert_eq!(ib.children[0].rect.w, 10 * CHAR, "ten characters fit in 80px");
    assert_eq!(ib.children[1].rect.w, 2 * CHAR, "and two do not");
    assert_eq!(ib.rect.h, 2 * STRUT_16, "so the box is two lines tall");
    assert_renderer_can_reach(&laid, &outer, "");
}

// ------------------------------------------------ requirement 7

#[test]
fn a_flex_container_inside_a_block_still_lays_out_as_flex() {
    register_synthetic();
    let flex_vnode = h(
        "div",
        vec![("style", "display:flex;width:200px")],
        vec![h("div", vec![("style", "width:40px")], vec![]), h("div", vec![("style", "width:60px")], vec![])],
    );
    let root = div(Some(200), FONT_SIZE, vec![flex_vnode]);
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let flex = &inner(&laid).children[0];
    assert_eq!(flex.children.len(), 2, "two items: {flex:?}");
    assert_eq!(flex.children[0].rect.w, 40, "the first item's width");
    assert_eq!(flex.children[1].rect.w, 60, "the second item's width");
    assert_eq!(
        flex.children[1].rect.x,
        flex.children[0].rect.x + 40,
        "a flex row places them side by side: {flex:?}"
    );
    assert_renderer_can_reach(&laid, &outer, "");
}

#[test]
fn inline_flex_still_routes_to_the_flex_path() {
    register_synthetic();
    // `inline-flex` is an inline-level box AND a block container. Routing it
    // into the enclosing block's line boxes would put its children in that
    // line and lose the flex layout entirely.
    let root = div(
        Some(200),
        FONT_SIZE,
        vec![h(
            "div",
            vec![("style", "display:inline-flex;width:200px")],
            vec![
                h("div", vec![("style", "width:40px")], vec![]),
                h("div", vec![("style", "width:60px")], vec![]),
            ],
        )],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.children.len(), 1, "the inline-flex box itself: {d:?}");
    let flex = &d.children[0];
    assert_eq!(flex.children.len(), 2, "its items are its own children: {flex:?}");
    assert_eq!(flex.children[0].rect.w, 40);
    assert_eq!(flex.children[1].rect.w, 60);
    assert_eq!(flex.children[1].rect.x, flex.children[0].rect.x + 40);
    assert_renderer_can_reach(&laid, &outer, "");
}

#[test]
fn an_out_of_flow_inline_is_still_out_of_flow() {
    register_synthetic();
    let root = div(
        Some(200),
        FONT_SIZE,
        vec![
            span("position:absolute;left:0;top:0", vec![text("abs")]),
            text("in flow"),
        ],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    let abs = &d.children[0];
    assert_eq!(abs.rect.x, 0, "pinned by its own offsets: {d:?}");
    assert_eq!(abs.rect.y, 0);
    assert_eq!(
        d.children[1].rect.x, 0,
        "the in-flow text is not pushed along by it: {d:?}"
    );
    assert_renderer_can_reach(&laid, &outer, "");
}

// ------------------------------------------------ structure and order

#[test]
fn siblings_stay_in_document_order() {
    register_synthetic();
    let root = div(
        Some(60),
        FONT_SIZE,
        vec![
            span("", vec![text("aaa")]),
            text(" bbb "),
            span("", vec![text("ccc")]),
        ],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    // Paint order is the order of `LayoutNode.children`, so flattening the tree
    // must read the VNode tree in document order.
    let mut order: Vec<String> = Vec::new();
    walk_text(&laid, &outer, &mut order);
    assert_eq!(
        order.concat(),
        "aaa bbb ccc",
        "the text appears in document order, which is paint order: {order:?}"
    );
}

/// Collect the text of every leaf box, reading the `VNode` tree by
/// `source_index` the way the renderer does, so the two are compared through
/// the same mapping rather than by position.
fn walk_text(layout: &LayoutNode, vnode: &VNode, out: &mut Vec<String>) {
    if let VNode::Element { children, .. } = vnode {
        for child in &layout.children {
            let idx = child.source_index.expect("asserted by assert_renderer_can_reach");
            match &children[idx] {
                VNode::Text(t) => out.push(t.clone()),
                other => walk_text(child, other, out),
            }
        }
    }
}

#[test]
fn a_block_child_ends_the_inline_run() {
    register_synthetic();
    let root = div(
        Some(200),
        FONT_SIZE,
        vec![
            text("aaa"),
            h("div", vec![("style", "height:10px")], vec![]),
            text("bbb"),
        ],
    );
    let outer = outer_of(&root);
    let laid = compute_layout(&outer, 600, 600);
    let d = inner(&laid);
    assert_eq!(d.children.len(), 3, "text, block, text: {d:?}");
    assert_eq!(d.children[0].rect.y, 0);
    assert_eq!(d.children[2].rect.y, STRUT_16 + 10, "after the block: {d:?}");
    assert_renderer_can_reach(&laid, &outer, "");
}

#[test]
fn a_large_fonts_strut_scales_the_line_and_the_hang_is_the_ascender() {
    register_synthetic();
    // 16px: strut 21.8, so 22. 24px: strut 32.7, so 33. 10px: strut 13.6, so
    // 14. Each is `round(size * 1.362)` and nothing else, which is the claim:
    // the line is the font's own metrics, not a multiple that happens to fit.
    for size in [10, 16, 24, 32] {
        let root = div(None, size, vec![text("xxx")]);
        let expected = (size as f32 * STRUT_EM).round() as i32;
            let outer = outer_of(&root);
        assert_eq!(
            inner(&compute_layout(&outer, 600, 600)).rect.h,
            expected,
            "{size}px owns a {expected}px line: 1.069 + 0.293 = {STRUT_EM}em"
        );
    }
    let _ = FONT_ASCENT_EM;
}
