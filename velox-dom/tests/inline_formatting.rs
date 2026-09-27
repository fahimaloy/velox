//! An inline formatting context, through `compute_layout` only.
//!
//! No rendering anywhere in this file, and that is deliberate rather than
//! convenient: `render_vnode_to_raster_png` does not run `compute_layout`, so a
//! test built on it would pass whether or not the layout was right. Every
//! assertion here is on the geometry `compute_layout` returns, which is the
//! project's preferred evidence form.
//!
//! ## How to read the expected values
//!
//! One synthetic measurer is registered for the whole binary, in
//! `tests/common/mod.rs`, because `set_skia_measurer` is a process global with
//! no unregister: a binary that registers can never observe the no-measurer
//! fallback, and a binary that does not register can never observe a real font.
//! Writing one measurer in two files is how they drift apart.
//!
//! Its WIDTH is 0.5em per character, the same as the no-measurer fallback's, so
//! every width in this file is also the width the fallback would give and the
//! numbers below are about the VERTICAL model alone. Its ascent is 2.0em and
//! its descent 0.5em for a run with an ascender or a descender, and 0.30em and
//! 0 for a run with neither, which is 2.5em and 0.30em against the default
//! face's 1.362em strut. So:
//!
//!   * `ccc`, `ooo`, `xxx` and friends are UNDER the strut, and a line holding
//!     one is exactly as tall as the strut.
//!   * anything holding `b`, `d`, `f`, `h`, `k`, `l`, `t` or a capital, or
//!     `g`, `j`, `p`, `q`, `y`, is OVER it, and that run decides the line's
//!     height.
//!
//! Every expected height below is therefore either 21.792 -> 22 (the STRUT, which
//! is `FontMetrics::from_font_size`'s `ascent`/`descent`, 1.069em + 0.293em read
//! out of the default face's font file and not out of this code -- NOT
//! `FontMetrics::heuristic_vertical`, which is the labelled INK guess at
//! 0.8em + 0.4em and is a different pair of numbers entirely) or 2.5 * 16 = 40
//! (the measurer, from `tests/common/mod.rs`). None of them is recomputed from
//! the implementation.

mod common;

use common::{FONT_ASCENT_EM, STRUT_EM, SYNTHETIC_ASCENT_EM, register_synthetic};
use velox_dom::layout::{LayoutNode, compute_layout};
use velox_dom::{VNode, h};

/// The base font size every case in this file uses.
const FS: i32 = 16;
/// The line box a strut-only line comes to, from the font file's 1.362em.
const STRUT: i32 = (FS as f32 * STRUT_EM).round() as i32;
/// A line whose run's ink (2.5em from `tests/common`) is taller than the strut.
const TALL_RUN: i32 = 40;

fn text(s: &str) -> VNode {
    VNode::Text(s.to_string())
}

/// `compute_layout` returns the ROOT ELEMENT'S OWN node, and a root is laid out
/// at the viewport's size, so a root's auto height is the viewport's height and
/// says nothing at all about its content. Every auto-height and shrink-to-fit
/// case below therefore wraps its box in an unconstrained parent and reads
/// `.children[0]`.
fn inner_of(v: &VNode) -> LayoutNode {
    compute_layout(&h("div", vec![], vec![v.clone()]), 600, 600).children[0].clone()
}

/// A div whose content is one `pre` line, in an unconstrained parent, so its
/// auto height is that line box's height.
fn one_line_h(s: &str) -> i32 {
    register_synthetic();
    let st = format!("font-size:{FS}px;white-space:pre");
    inner_of(&h("div", vec![("style", st.as_str())], vec![text(s)]))
        .rect
        .h
}

fn pre_div(children: Vec<VNode>) -> VNode {
    let st = format!("font-size:{FS}px;white-space:pre");
    h("div", vec![("style", st.as_str())], children)
}

// ---------------------------------------------------------------------------
// The shape of the tree
// ---------------------------------------------------------------------------

/// The renderer and the hit tester both resolve a `LayoutNode` by looking
/// `source_index` up in the VNode children, and BOTH SKIP the node's whole
/// subtree when they cannot. So the layout tree has to mirror the VNode tree
/// level for level, which is the constraint that shapes everything else here.
#[test]
fn every_emitted_node_carries_the_index_of_the_vnode_it_came_from() {
    register_synthetic();
    let d = inner_of(&pre_div(vec![
        h("b", vec![], vec![text("ccc")]),
        text(" ooo"),
        h("i", vec![], vec![text("xxx")]),
        text(" ccc"),
    ]));
    let idx = |n: &LayoutNode| n.source_index.expect("a node with no index is unreachable");
    assert_eq!(
        d.children.len(),
        4,
        "four VNode children, four layout nodes"
    );
    assert_eq!(idx(&d.children[0]), 0, "the <b>");
    assert_eq!(idx(&d.children[1]), 1, "the text between");
    assert_eq!(idx(&d.children[2]), 2, "the <i>");
    assert_eq!(idx(&d.children[3]), 3, "the text after");
    assert_eq!(d.children[0].children.len(), 1);
    assert_eq!(idx(&d.children[0].children[0]), 0, "the <b>'s own text");
}

/// An inline element with no text in its subtree contributes no measurable
/// piece, so it would be missing from the tree entirely and the renderer could
/// not reach anything under it. It still gets a box, and it is zero wide.
#[test]
fn an_inline_element_with_no_text_still_gets_a_box() {
    register_synthetic();
    let d = inner_of(&pre_div(vec![h("span", vec![], vec![]), text("ccc")]));
    assert_eq!(
        d.children.len(),
        2,
        "the empty span must not vanish from the tree"
    );
    let empty = &d.children[0];
    assert_eq!(empty.source_index, Some(0));
    assert_eq!(empty.rect.w, 0, "a box with nothing in it is zero wide");
    assert_eq!(
        d.children[1].rect.x, 0,
        "and it takes no room: the text starts at the line's left edge"
    );
}

// ---------------------------------------------------------------------------
// Line boxes
// ---------------------------------------------------------------------------

/// A block container's height is the sum of its line boxes', so a div holding
/// exactly one line IS that line box's height. This is the only honest way to
/// read a line box's height out of the tree: a text fragment's own box is its
/// font's content area HANGING FROM THE BASELINE, so it is not the line box and
/// its top is not the line's top.
#[test]
fn a_line_is_as_tall_as_the_run_in_it_when_the_run_is_taller_than_the_strut() {
    assert_eq!(
        one_line_h("Hg"),
        TALL_RUN,
        "2.5em of ink over a 1.362em strut"
    );
    assert_eq!(one_line_h("xxx"), STRUT, "0.30em of ink under the strut");
}

/// Two lines in one container are two line boxes, and each is decided by its own
/// content.
#[test]
fn two_lines_are_two_independent_line_boxes() {
    register_synthetic();
    let d = inner_of(&pre_div(vec![text("Hg\nxxx")]));
    assert_eq!(
        d.rect.h,
        TALL_RUN + STRUT,
        "one tall line, then one strut line"
    );
    assert_eq!(
        d.children[0].rect.y,
        ((SYNTHETIC_ASCENT_EM - FONT_ASCENT_EM) * FS as f32).round() as i32,
        "a fragment hangs from the baseline, so it starts below its line's top \
         by exactly the amount its ink overshot the strut"
    );
    assert_eq!(
        d.children[1].rect.y, TALL_RUN,
        "the second line starts below the first"
    );
}

/// Every fragment's box is the line's own font's content area, whatever its ink.
/// This is what makes `vertical-align` legible in the geometry at all: the ink
/// decides the LINE, and the box is what the alignment moves.
#[test]
fn a_text_fragment_box_is_its_own_fonts_content_box_hanging_from_the_baseline() {
    register_synthetic();
    let d = inner_of(&pre_div(vec![h(
        "span",
        vec![("style", "font-size:32px")],
        vec![text("xxx")],
    )]));
    let frag = &d.children[0].children[0];
    assert_eq!(
        frag.rect.h,
        (32.0 * STRUT_EM).round() as i32,
        "a 32px run's box is 32px's content area, not its 0.30em of x-height ink"
    );
    assert_eq!(
        d.rect.h, frag.rect.h,
        "so the line is the 32px content area, not the 16px strut it is laid out \
         in: a run in a larger font grows the line even when its own x-height \
         ink is tiny"
    );
    assert_eq!(
        d.children[0].rect.h, frag.rect.h,
        "and the inline element's box is the union of its fragments"
    );
}

#[test]
fn a_run_that_overshoots_the_strut_makes_the_line_taller_without_taller_boxes() {
    register_synthetic();
    let d = inner_of(&pre_div(vec![text("Hg\nccc")]));
    for line in &d.children {
        assert_eq!(
            line.rect.h, STRUT,
            "the 2.5em ink grew its LINE, and no box on it"
        );
    }
    assert_eq!(d.rect.h, TALL_RUN + STRUT);
}

// ---------------------------------------------------------------------------
// Wrapping, including across inline element boundaries
// ---------------------------------------------------------------------------

/// The single most user-visible thing here. Whitespace RUNS are the break
/// opportunities, so a break can land between two inline elements as easily as
/// inside one text node.
#[test]
fn a_line_may_break_at_the_edge_of_an_inline_element() {
    register_synthetic();
    // 40px / 8px per char = 5 chars a line.
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:40px")],
        vec![h("b", vec![], vec![text("ccc")]), text(" ooo ccc")],
    ));
    let x = |n: &LayoutNode| n.rect.x;
    let y = |n: &LayoutNode| n.rect.y;
    let w = |n: &LayoutNode| n.rect.w;
    assert_eq!(
        d.children.len(),
        4,
        "the break splits the one text VNode in two"
    );
    assert_eq!(
        (x(&d.children[0]), y(&d.children[0]), w(&d.children[0])),
        (0, 0, 24)
    );
    assert_eq!(
        (x(&d.children[1]), y(&d.children[1]), w(&d.children[1])),
        (24, 0, 8),
        "the space hangs on the first line, where it was written"
    );
    assert_eq!(
        (x(&d.children[2]), y(&d.children[2]), w(&d.children[2])),
        (0, STRUT, 32),
        "\"ooo \" is the second line"
    );
    assert_eq!(
        (x(&d.children[3]), y(&d.children[3]), w(&d.children[3])),
        (0, 2 * STRUT, 24)
    );
    assert_eq!(d.rect.h, 3 * STRUT, "three lines");
}

/// The same break with the inline element in the middle of the whitespace run.
#[test]
fn an_inline_element_deep_inside_another_one_joins_the_same_run() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:40px")],
        vec![h(
            "b",
            vec![],
            vec![
                text("ccc "),
                h("i", vec![], vec![text("ooo")]),
                text(" ccc"),
            ],
        )],
    ));
    assert_eq!(d.children.len(), 3, "one <b> node per line it is on");
    assert_eq!(d.children[0].rect.w, 32, "\"ccc \"");
    assert_eq!(d.children[1].rect.y, STRUT, "the middle line");
    assert_eq!(d.children[1].rect.w, 32, "\"ooo \"");
    assert_eq!(
        d.children[1].children.len(),
        2,
        "the <i> and the space after it"
    );
    assert_eq!(d.children[2].rect.y, 2 * STRUT, "the last line");
    assert_eq!(d.children[2].rect.w, 24, "\"ccc\"");
    assert_eq!(
        d.children[2].children[0].source_index,
        Some(2),
        "and it is the <b>'s own third child, not the <i> again"
    );
}

/// A word is unbreakable. `overflow-wrap: normal` is the CSS default and this
/// does not implement anything else.
#[test]
fn a_word_is_never_split_across_lines() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:40px")],
        vec![text("cccccccccc")],
    ));
    assert_eq!(
        d.children.len(),
        1,
        "one fragment, however far it overflows"
    );
    assert_eq!(d.children[0].rect.w, 80, "5 chars fit, 10 were written");
}

/// A break before an atomic that does not fit moves it to the NEXT line, where
/// the whole line is available to it.
#[test]
fn an_inline_block_moves_to_the_next_line_rather_than_overflowing() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:40px")],
        vec![
            text("ccc "),
            h(
                "span",
                vec![("style", "display:inline-block")],
                vec![text("ccc ccc ccc")],
            ),
        ],
    ));
    assert_eq!(d.children[1].rect.y, STRUT, "it starts the second line");
    assert_eq!(d.children[1].rect.x, 0, "at the line's left edge");
    assert_eq!(
        d.children[1].rect.w, 40,
        "and is sized against the line it is ON, not the one it was offered"
    );
}

// ---------------------------------------------------------------------------
// Inline elements and block children
// ---------------------------------------------------------------------------

/// An inline element is a SPAN, not a block box: CSS 2.1 §9.4.2 gives it no box
/// at all, so this rect is the union of its fragments and nothing more.
#[test]
fn an_inline_element_has_no_box_of_its_own() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px")],
        vec![text("Hg "), h("b", vec![], vec![text("ccc")])],
    ));
    let b = &d.children[1];
    assert_eq!(b.rect.w, 24, "exactly its text, not the 200px line");
    assert_eq!(
        b.rect.h, STRUT,
        "exactly its own font's content area, not the line box it is on"
    );
    assert_eq!(
        d.rect.h, TALL_RUN,
        "the line is the 2.5em run beside it, NOT grown by the <b>: an inline \
         element has no box, so it is not 40px tall"
    );
}

/// A block child ends the run. This is the requirement that says an inline run
/// and the block children around it are two different things.
#[test]
fn a_block_child_ends_the_inline_run() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px")],
        vec![
            text("ccc"),
            h("div", vec![("style", "height:10px")], vec![]),
            text("ccc"),
        ],
    ));
    assert_eq!(d.children.len(), 3);
    assert_eq!(
        d.children[1].rect.y, STRUT,
        "the block starts below the first line"
    );
    assert_eq!(
        d.children[1].rect.h, 10,
        "and is a block: 10px, not the strut"
    );
    assert_eq!(
        d.children[2].rect.y,
        STRUT + 10,
        "the run after it starts a fresh line below the block, and the line it \
         ended with is charged once, not twice"
    );
    assert_eq!(d.rect.h, 2 * STRUT + 10);
}

// ---------------------------------------------------------------------------
// inline-block: the atomic inline box
// ---------------------------------------------------------------------------

#[test]
fn an_inline_block_is_atomic_and_shares_the_line_with_the_text_around_it() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px")],
        vec![
            h(
                "span",
                vec![("style", "display:inline-block")],
                vec![text("ccc")],
            ),
            text(" ooo ccc"),
        ],
    ));
    let (ib, txt) = (&d.children[0], &d.children[1]);
    assert_eq!(
        (ib.rect.x, ib.rect.w),
        (0, 24),
        "sized to its content, not the 200px line"
    );
    assert_eq!(txt.rect.x, 24, "the text after it starts where it ends");
    assert!(
        (txt.rect.y - ib.rect.y).abs() < STRUT,
        "one line, so both are in it: {:?}",
        (ib.rect.y, txt.rect.y)
    );
    assert_eq!(d.children.len(), 2, "the atomic was not split");
}

#[test]
fn an_inline_block_is_never_split_across_lines() {
    register_synthetic();
    // 12 characters with spaces: there IS a break opportunity, and the atomic
    // still refuses to take one, because it is a block formatting context.
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px")],
        vec![h(
            "span",
            vec![("style", "display:inline-block;width:80px")],
            vec![text("ccc ccc ccc ccc")],
        )],
    ));
    let ib = &d.children[0];
    assert_eq!(
        d.children.len(),
        1,
        "one node: the atomic is not a run of fragments"
    );
    assert_eq!(ib.rect.w, 80, "its declared width");
    assert_eq!(
        ib.rect.h,
        2 * STRUT,
        "it WRAPPED INSIDE ITSELF, which is the whole difference between an \
         atomic inline and the text around it"
    );
    assert_eq!(ib.children.len(), 2, "its own two line boxes");
}

/// CSS 2.1 §10.3.5: an inline-block's width is its max-content width clamped by
/// the space available on the line.
#[test]
fn an_inline_block_shrinks_to_the_space_left_on_its_line() {
    register_synthetic();
    // 72px of content, offered 40px of line.
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:40px")],
        vec![h(
            "span",
            vec![("style", "display:inline-block")],
            vec![text("ccc ccc ccc")],
        )],
    ));
    assert_eq!(d.children[0].rect.w, 40, "72px clamped to the 40px line");
    assert_eq!(
        d.children[0].rect.h,
        3 * STRUT,
        "so it wrapped to three lines"
    );
}

/// An atomic inline participates in the line's baseline.
#[test]
fn an_inline_block_shares_the_baseline_of_the_text_beside_it() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px")],
        vec![
            text("Hg "),
            h(
                "span",
                vec![("style", "display:inline-block")],
                vec![text("ccc")],
            ),
        ],
    ));
    assert_eq!(
        d.children[0].rect.y, d.children[1].rect.y,
        "the atomic's box is aligned by its own baseline, which here coincides \
         with the text's"
    );
    assert_eq!(
        d.rect.h, TALL_RUN,
        "and the line is the tall run's, not the strut's"
    );
}

// ---------------------------------------------------------------------------
// vertical-align
// ---------------------------------------------------------------------------

/// The four supported values, on a box SMALLER than the line, so that all four
/// land somewhere different and none of them can pass by accident.
///
/// The box is 8px of font in a 16px line, so its content area is 10.896 -> 11
/// and the line is the 16px strut. That makes the four positions derivable from
/// two other layouts rather than from this code: `top` is the line's top edge,
/// and `bottom` is the line's top plus the line's height minus the box's height.
#[test]
fn all_four_supported_alignments_land_somewhere_different() {
    register_synthetic();
    let y_of = |al: &str| -> i32 {
        let st = format!("font-size:8px;vertical-align:{al}");
        let d = inner_of(&pre_div(vec![
            text("ccc\n"),
            h("span", vec![("style", st.as_str())], vec![text("ccc")]),
        ]));
        assert_eq!(
            d.children[0].rect.h, STRUT,
            "the first line is a strut line"
        );
        d.children[1].rect.y
    };
    let top = y_of("top");
    let bottom = y_of("bottom");
    let baseline = y_of("baseline");
    let middle = y_of("middle");
    assert_eq!(top, STRUT, "`top` puts the box at the line box's top edge");
    assert!(
        top < baseline,
        "a baseline-aligned box is below the line's top edge"
    );
    assert!(top < bottom, "and so is a bottom-aligned one");
    // The remaining orderings are not asserted. `middle` grows the line by its
    // own extent, and an 8px strut centred on a 16px baseline hangs below the
    // line, so where each of the three lands relative to the others depends on
    // two things this file does not fix. What matters is that the four are four
    // different boxes and that `top` is where CSS 2.1 §10.8.1 says it is.
    for (name, v) in [
        ("baseline", baseline),
        ("bottom", bottom),
        ("middle", middle),
    ] {
        assert_ne!(v, top, "a {name}-aligned box is not a top-aligned one");
    }
}

/// `baseline` is the initial value, so a box that declares nothing sits on the
/// line's baseline.
#[test]
fn baseline_is_the_default() {
    register_synthetic();
    let declared = inner_of(&pre_div(vec![
        text("ccc\n"),
        h(
            "span",
            vec![("style", "font-size:8px;vertical-align:baseline")],
            vec![text("ccc")],
        ),
    ]));
    let absent = inner_of(&pre_div(vec![
        text("ccc\n"),
        h("span", vec![("style", "font-size:8px")], vec![text("ccc")]),
    ]));
    assert_eq!(
        declared.children[1].rect.y, absent.children[1].rect.y,
        "saying `baseline` and saying nothing are the same box"
    );
}

/// `vertical-align` is an INHERITED property (CSS 2.1 §10.8.1), so it has to
/// reach a text node that is not inside the element that declared it.
#[test]
fn vertical_align_is_inherited() {
    register_synthetic();
    let st = format!("font-size:{FS}px;white-space:pre;vertical-align:top");
    let d = inner_of(&h(
        "div",
        vec![("style", st.as_str())],
        vec![text("Hg\nHg")],
    ));
    assert_eq!(
        d.rect.h,
        2 * STRUT,
        "both lines are strut lines: an 8px-free top-aligned 2.5em run does not \
         grow a line, because what is aligned is its BOX, not its ink"
    );
    assert_eq!(
        d.children[0].rect.y, 0,
        "the first box is at the first line's top"
    );
    assert_eq!(
        d.children[1].rect.y, STRUT,
        "and the second at the second line's top"
    );
}

/// `sub` and `super` are NOT supported, and `VerticalAlign::parse` returning
/// `None` for them is the only thing keeping that honest: a declaration naming
/// one is not understood, so the inherited value stands and the box lands where
/// an unaligned box lands. This is a test of the SUBSET being legible, and it
/// would fail loudly if someone later added a value to the enum without either
/// implementing it or naming it here.
#[test]
fn sub_and_super_are_not_supported_and_say_so_by_not_being_parsed() {
    use velox_dom::style::VerticalAlign;
    assert_eq!(
        VerticalAlign::parse("sub"),
        None,
        "documented as unsupported"
    );
    assert_eq!(
        VerticalAlign::parse("super"),
        None,
        "documented as unsupported"
    );
    assert_eq!(VerticalAlign::parse("text-top"), None, "also unsupported");
    assert_eq!(
        VerticalAlign::parse("10%"),
        None,
        "a length is legal CSS and is not read as baseline"
    );
    register_synthetic();
    let sub = inner_of(&pre_div(vec![
        text("ccc\n"),
        h(
            "span",
            vec![("style", "font-size:8px;vertical-align:sub")],
            vec![text("ccc")],
        ),
    ]));
    let plain = inner_of(&pre_div(vec![
        text("ccc\n"),
        h("span", vec![("style", "font-size:8px")], vec![text("ccc")]),
    ]));
    assert_eq!(
        sub.children[1].rect.y, plain.children[1].rect.y,
        "an unparsed value leaves the box exactly where no value would"
    );
}

// ---------------------------------------------------------------------------
// What this task did not change
// ---------------------------------------------------------------------------

/// `display: inline-flex` does NOT reach the flex path, and did not before this
/// task either: `at` dispatches on `display == "flex"` alone, so `inline-flex`
/// has always fallen to the block path. What this task guarantees is only that
/// it did not get WORSE: an inline-flex container is not flattened into the
/// enclosing line, so its children are not in the line's baseline grid.
///
/// The defect is a known one, in the block path's out-of-flow handling, and it
/// is deliberately left alone here. What this test pins is the R-5b-scoped
/// property, not the pre-existing behaviour of the flex engine.
#[test]
fn inline_flex_is_not_flattened_into_the_enclosing_line() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px")],
        vec![h(
            "div",
            vec![("style", "display:inline-flex;width:200px")],
            vec![
                h("div", vec![("style", "width:40px")], vec![]),
                h("div", vec![("style", "width:60px")], vec![]),
            ],
        )],
    ));
    assert_eq!(
        d.children.len(),
        1,
        "the inline-flex box is a block child, not run material"
    );
    assert_eq!(
        d.children[0].children.len(),
        2,
        "its children are inside it"
    );
}

/// A flex container is NOT inline-level, whatever the tag, so it forms its own
/// block and the flex engine keeps laying it out as flex.
#[test]
fn a_flex_container_inside_a_block_still_lays_out_as_flex() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px")],
        vec![h(
            "div",
            vec![("style", "display:flex")],
            vec![
                h("div", vec![("style", "width:40px")], vec![]),
                h("div", vec![("style", "width:60px")], vec![]),
            ],
        )],
    ));
    assert_eq!(d.children.len(), 1);
    assert_eq!(d.children[0].children.len(), 2);
}

/// An out-of-flow inline element is still out of flow: it is not run material,
/// and it takes no room in the line.
#[test]
fn an_out_of_flow_inline_is_still_out_of_flow() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px")],
        vec![
            h(
                "span",
                vec![("style", "position:absolute;left:0;top:0")],
                vec![text("ccc")],
            ),
            text("ccc"),
        ],
    ));
    assert_eq!(d.children.len(), 2, "one out-of-flow box and one text node");
    assert_eq!(
        d.children[0].source_index,
        Some(1),
        "the in-flow text is in the line"
    );
    assert_eq!(
        d.children[0].rect.x, 0,
        "and it starts at the line's left edge"
    );
    assert_eq!(
        d.children[1].source_index,
        Some(0),
        "the out-of-flow box is not"
    );
    assert_eq!(
        d.rect.h, STRUT,
        "one line: the out-of-flow box did not add one"
    );
}

#[test]
fn text_align_center_still_centres_an_inline_line() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px;text-align:center")],
        vec![h("b", vec![], vec![text("ccc")])],
    ));
    assert_eq!(d.children[0].rect.w, 24, "\"ccc\" is 3 * 0.5em");
    assert_eq!(
        d.children[0].rect.x,
        (200 - 24) / 2,
        "and it is centred on the 200px line"
    );
}

#[test]
fn text_align_right_still_right_aligns_an_inline_line() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "font-size:16px;width:200px;text-align:right")],
        vec![h("b", vec![], vec![text("ccc")])],
    ));
    assert_eq!(d.children[0].rect.x, 200 - 24);
}

// ---------------------------------------------------------------------------
// Fix round 1, C-1: `text-overflow: ellipsis` in the layout tree
// ---------------------------------------------------------------------------

/// C-1. The reviewer's own input, and the assertion is on `compute_layout`'s
/// tree. **This is the test that distinguishes the layout from the paint, and
/// the reason it is this one is worth stating:** the renderer truncates again at
/// paint time from `text_style.ellipsis`, so before the fix the pixels were
/// already right and every render-based check would have passed. A test that
/// read the renderer could not have told the two apart. Reading the layout tree
/// can, because the layout tree is what was wrong.
///
/// 40px of line, 8px per character, so four characters plus the one-character
/// ellipsis is exactly 40. Before the fix the fragment was the full 80.
#[test]
fn an_ellipsis_is_reserved_space_in_the_layout_tree_and_never_overflows_its_line() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![(
            "style",
            "width:40px;white-space:nowrap;text-overflow:ellipsis",
        )],
        vec![text("aaaaaaaaaa")],
    ));
    assert_eq!(
        d.children.len(),
        1,
        "one overflowing line is one fragment, got {:?}",
        d.children.iter().map(|c| c.rect).collect::<Vec<_>>()
    );
    assert_eq!(
        d.children[0].rect.w, 40,
        "four characters plus the ellipsis is exactly the 40px line; anything \
         less means a character was dropped that fitted"
    );
    assert!(
        d.children[0].rect.w <= 40,
        "an ellipsis is RESERVED space, so a truncated fragment can never be \
         wider than the line it is clipped in; got {}",
        d.children[0].rect.w
    );
}

/// The contrast case, and it is the same input without the property. Before the
/// fix both of these produced a layout tree of 80 and only this one should.
#[test]
fn without_the_property_the_same_text_overflows_its_line_unchanged() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "width:40px;white-space:nowrap")],
        vec![text("aaaaaaaaaa")],
    ));
    assert_eq!(
        d.children[0].rect.w, 80,
        "with no `text-overflow` the fragment keeps its full width and overflows, \
         which is what distinguishes this case from the truncated one"
    );
}

/// The ellipsis lands in the piece where the text ran out, so `<b>aa</b>` is left
/// alone and the text node after it is shortened. A browser puts the ellipsis at
/// the end of the line, not at the end of the first element that did not fit, so
/// this is a real decision and the widths distinguish it: 16 is `aa` untouched and
/// 24 is `" b" + ellipsis`.
#[test]
fn the_ellipsis_lands_in_the_piece_where_the_text_ran_out_not_the_first_one() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![(
            "style",
            "width:40px;white-space:nowrap;text-overflow:ellipsis",
        )],
        vec![h("b", vec![], vec![text("aa")]), text(" bbbbbb")],
    ));
    let b = d
        .children
        .iter()
        .find(|c| c.children.len() == 1)
        .expect("the <b> is a leaf one level down");
    let after = d
        .children
        .iter()
        .find(|c| c.children.is_empty())
        .expect("the text after it is a bare fragment");
    assert_eq!(
        (b.rect.w, after.rect.w),
        (16, 24),
        "the <b> keeps `aa` at 16 and the text piece becomes ` b` + ellipsis at 24; \
         if the ellipsis had gone into the <b> these would be 24 and 16"
    );
}

/// A fixed-width atomic truncates its own content, which is the case a label in a
/// sized box is. A DECLARED `width`, though, is the case that says nothing about
/// `max_content_width`: `at()` takes the declared width as the available width and
/// `target` is never computed, so nothing here is a measurement being clamped.
/// The `available` path below is the one that exercises `max_content_width`.
///
/// What is checked here: a declared 40px box clips 80px of text to 40. What is
/// NOT checked here: anything about the preferred width, because a declared width
/// short-circuits it. The false premise this doc used to carry -- that the
/// shrink-to-fit case is unreachable in this implementation -- is retired by the
/// next test, which exercises it. See the report.
#[test]
fn an_atomic_inline_box_with_a_declared_width_truncates_its_content_to_it() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "width:200px")],
        vec![h(
            "span",
            vec![(
                "style",
                "display:inline-block;width:40px;white-space:nowrap;text-overflow:ellipsis",
            )],
            vec![text("aaaaaaaaaa")],
        )],
    ));
    let atomic = &d.children[0];
    assert_eq!(atomic.rect.w, 40, "the atomic keeps its declared width");
    assert_eq!(
        atomic.children[0].rect.w, 40,
        "and its own content truncates to it, so 80px of text becomes four \
         characters plus an ellipsis and the box does not overflow"
    );
}

/// C-1.3, the case the declared-width test above cannot reach. The atomic has NO
/// declared width, so `at()` is handed a `target` -- `min(max_content_width,
/// available)` -- and the content is 80px of unbreakable text on a 40px line, so
/// `max_content` (80) exceeds `available` (40) and the `min` is the LINE. The
/// second pass therefore lays the subtree out at 40, the line's own limit, and the
/// ellipsis applies to that layout. That is the whole mechanism, and it needs no
/// second measurement: the width the label ends up with IS the target, and the
/// content inside it is the truncation of the subtree laid out AT that target.
///
/// The contrast is the evidence and it is a second test below, not a bound here:
/// 40 on its own is a bare number that a wrong implementation also produces.
#[test]
fn an_atomic_with_no_declared_width_shrink_fits_to_the_line_and_truncates_its_label() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "width:40px")],
        vec![h(
            "span",
            vec![(
                "style",
                "display:inline-block;white-space:nowrap;text-overflow:ellipsis",
            )],
            vec![text("aaaaaaaaaa")],
        )],
    ));
    let atomic = &d.children[0];
    assert_eq!(
        atomic.rect.w, 40,
        "the preferred width is 80, the line is 40, and `min` is the line -- this is \
         the clamp, and `available`, not a declared width, is what put it here"
    );
    assert_eq!(
        atomic.children[0].rect.w, 40,
        "and the label inside it is truncated to that clamp, four characters plus the \
         ellipsis, which is the case the declared-width test cannot reach: nothing \
         here short-circuits the shrink-to-fit"
    );
}

/// The falsification half of the C-1.3 test, and the reason the assertion above is
/// evidence. The ONLY difference from it is the presence of `text-overflow`, so
/// with the property gone the atomic is still clamped to 40 and its label is still
/// 80 -- the clamp did not move, the truncation did. If the clamp and the
/// truncation ever shared a cause, or the truncation were ignored on this path,
/// these two numbers would come out the same.
#[test]
fn without_the_property_the_shrink_fitted_label_overflows_the_clamp_that_did_not_move() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "width:40px")],
        vec![h(
            "span",
            vec![("style", "display:inline-block;white-space:nowrap")],
            vec![text("aaaaaaaaaa")],
        )],
    ));
    let atomic = &d.children[0];
    assert_eq!(
        atomic.rect.w, 40,
        "the clamp is the line either way -- this number is NOT what the property \
         controls, and the test above's identical 40 is not evidence of anything"
    );
    assert_eq!(
        atomic.children[0].rect.w, 80,
        "without `text-overflow` the label keeps its full 80 and overflows the 40 it \
         was clamped into, which is the whole difference between these two cases and \
         what makes the test above a measurement rather than a coincidence"
    );
}

/// Which lines truncate follows the rule the single-string wrapper used, and for a
/// wrapping block that is a recorded divergence: a browser puts the ellipsis on the
/// last line the block actually clips, which is its last line whether or not the run
/// came out as one line. Two lines here, so neither truncates.
#[test]
fn a_wrapping_block_does_not_truncate_which_is_a_recorded_divergence() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "width:40px;text-overflow:ellipsis")],
        vec![text("aaaa aaaa")],
    ));
    assert_eq!(
        d.children.iter().map(|c| c.rect.w).collect::<Vec<_>>(),
        vec![40, 32],
        "the run wrapped, so the wrapper's rule leaves it alone; matching a browser \
         needs to know which line is the clipped one, which needs the clip region \
         the layout tree does not carry"
    );
}

/// `pre` is the mode that DOES truncate each of its own overflowing lines, because
/// each is its own line box and none of them can ever be the only one.
#[test]
fn a_pre_block_truncates_each_of_its_own_overflowing_lines() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "width:40px;white-space:pre;text-overflow:ellipsis")],
        vec![text("aaaaaaaaaa\nbbbbbbbbbb")],
    ));
    assert_eq!(
        d.children.iter().map(|c| c.rect.w).collect::<Vec<_>>(),
        vec![40, 40],
        "both lines are their own line box and both overflow, so both truncate"
    );
}

// ---------------------------------------------------------------------------
// Fix round 1, C-2: the degenerate atomics
// ---------------------------------------------------------------------------

/// C-2. The `> 0` guard in the shrink-to-fit second pass read as "do not shrink to
/// nothing" and its effect was the opposite: the content width stood at the probe,
/// 4096. An empty atomic is 0 wide, and 10 wide with its own 5px padding on each
/// side.
#[test]
fn an_empty_atomic_inline_box_is_zero_wide_plus_its_own_padding_and_border() {
    register_synthetic();
    for (label, style, want) in [
        ("no padding", "display:inline-block", 0),
        ("5px padding", "display:inline-block;padding:5px", 10),
    ] {
        let d = inner_of(&h(
            "div",
            vec![("style", "width:200px")],
            vec![h("span", vec![("style", style)], vec![])],
        ));
        assert_eq!(
            d.children[0].rect.w, want,
            "an empty atomic ({label}) is its content area of 0 plus its own inset, \
             not the probe width it used to get"
        );
    }
}

/// The same defect by another route: a subtree with no box in it at all measures
/// as nothing, and the same `> 0` guard turned that into 4096.
#[test]
fn an_atomic_inline_box_whose_only_child_is_display_none_is_zero_wide() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "width:200px")],
        vec![h(
            "span",
            vec![("style", "display:inline-block")],
            vec![h(
                "div",
                vec![("style", "display:none")],
                vec![text("aaaaaaaaaa")],
            )],
        )],
    ));
    assert_eq!(
        d.children[0].rect.w, 0,
        "a child that is not laid out contributes no content, so the atomic's \
         content width is 0"
    );
}

/// The SAME guard, a third way, and the reviewer had not listed it: content asking
/// for more than the line has used to be left at the probe width. 80px of
/// unbreakable text on a 40px line, so `min(max_content, available)` is the line.
#[test]
fn an_atomic_inline_box_wider_than_the_line_is_the_line_wide() {
    register_synthetic();
    let d = inner_of(&h(
        "div",
        vec![("style", "width:40px")],
        vec![h(
            "span",
            vec![("style", "display:inline-block")],
            vec![text("aaaaaaaaaa")],
        )],
    ));
    assert_eq!(
        d.children[0].rect.w, 40,
        "a preferred width wider than the line is clamped to the line, which is \
         what `min(max_content, available)` says and what the probe width used to \
         override"
    );
}
