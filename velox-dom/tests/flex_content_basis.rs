//! REGRESSION GUARD for the flex base-size defect, at the layout-rect level.
//!
//! Runs by default: no features, no Skia. The pixel-level proof of the same
//! defect lives in `todo_item_pixels.rs` (feature-gated); this file is what
//! `cargo test --workspace` actually exercises.
//!
//! These assert OBSERVED engine output against what CSS requires. Nothing here
//! re-implements the flex algorithm -- if it did, it would agree with a broken
//! engine and prove nothing.
//!
//! THE DEFECT, in three layers that all had to be fixed:
//!
//! 1. The child's available cross size was passed into `at()`'s available-WIDTH
//!    slot and its main size into the available-HEIGHT slot, so in a row flex
//!    container a non-grow child came out exactly the container's cross
//!    (vertical) size wide.
//! 2. The resolved basis was only written onto the child when the item had
//!    `flex-grow > 0 || flex-shrink > 0`, so `flex: 0 0 20px` and `flex: none`
//!    -- both grow 0, shrink 0 -- had a correct basis computed and then
//!    discarded.
//! 3. A block child with no declared `width` FILLS whatever width it is given,
//!    so an auto-sized item was measured at the line's full main size instead
//!    of its content size. Every auto item then claimed the whole row and the
//!    `flex: 1` sibling collapsed to zero.

use velox_dom::{VNode, h, layout::compute_layout};

const W: i32 = 400;

/// The `.todo-item` row from `veloxc/templates/project/src/components/TodoItem.vx`,
/// INCLUSIVE of the `input.checkbox` the pixel proof omits, with styles inline
/// because `compute_layout` takes no stylesheet.
fn todo_item() -> VNode {
    h(
        "div",
        vec![(
            "style",
            "display:flex; align-items:center; gap:10px; padding:12px 16px;",
        )],
        vec![
            h(
                "input",
                vec![("class", "checkbox"), ("style", "width: 18px;")],
                vec![],
            ),
            h(
                "span",
                vec![
                    ("class", "todo-text"),
                    ("style", "flex: 1; font-size: 15px;"),
                ],
                vec![VNode::Text("Buy milk".into())],
            ),
            h(
                "button",
                vec![("style", "padding: 4px 10px; font-size: 14px;")],
                vec![VNode::Text("×".into())],
            ),
        ],
    )
}

/// A row container of a given width holding one child with a given style.
fn row(width: i32, child_style: &str, text: &str) -> VNode {
    let row_style = format!("display:flex; width:{width}px; height:40px;");
    h(
        "div",
        vec![("style", row_style.as_str())],
        vec![h(
            "div",
            vec![("style", child_style)],
            vec![VNode::Text(text.into())],
        )],
    )
}

/// The same, but with an INDEFINITE cross axis -- no explicit `height`.
///
/// This is the shape the real `.todo-item` has, and it is the one that matters:
/// a row container with a definite height passes its height into the width slot
/// and a child then comes out exactly that wide, whereas with an INDEFINITE
/// cross the unconstrained sentinel (`i32::MAX as f32`) reaches the width slot
/// instead. Testing only the definite-height shape silently dodges the worse of
/// the two -- a definite-height row hides the `i32::MAX` leak completely, and a
/// width-less child lands on the container height, which happens to be constant
/// across container widths and so also passes a "does not track the container"
/// assertion. Both of those mistakes were made here first and caught by
/// reverting the fix and watching the tests stay green.
fn row_indefinite_cross(width: i32, child_style: &str, text: &str) -> VNode {
    let row_style = format!("display:flex; width:{width}px;");
    h(
        "div",
        vec![("style", row_style.as_str())],
        vec![h(
            "div",
            vec![("style", child_style)],
            vec![VNode::Text(text.into())],
        )],
    )
}

/// The owner-reported shape. The `flex: 1` label must actually receive the
/// row's free space, and the non-grow button must not swallow it.
#[test]
fn a_flex_one_label_owns_the_row_and_its_siblings_do_not() {
    // `compute_layout` returns the root itself, so the row's three items are
    // `laid.children` directly -- not `laid.children[0].children`.
    let laid = compute_layout(&todo_item(), W, 60);
    let kids = &laid.children;
    let (checkbox, label, button) = (kids[0].rect.w, kids[1].rect.w, kids[2].rect.w);
    println!("checkbox={checkbox} label(flex:1)={label} button={button}");

    // Content box is 400 - 32 padding = 368, spent as three items plus two
    // 10px gaps, so the items share 348. The button takes its content (27).
    assert!(
        kids[0].rect.x == 16,
        "left padding lost: first child at x={} not 16",
        kids[0].rect.x
    );
    assert!(
        checkbox == 18,
        "explicitly-sized checkbox moved: {checkbox} != 18"
    );
    assert!(
        label > 200,
        "the `flex: 1` label got {label}px of a 330px share -- it did not grow"
    );
    assert!(
        button < 100,
        "the non-grow button took {button}px -- it absorbed the row's main axis"
    );
    assert_eq!(
        checkbox + label + button + 20,
        W - 32,
        "items plus gaps must exactly fill the 368px content box"
    );
    assert!(
        kids[2].rect.x == 357 && kids[2].rect.x + button == 384,
        "the button must end at the right padding edge, got x={} w={button}",
        kids[2].rect.x
    );
}

/// No width may ever come out as the `UNCONSTRAINED_CROSS_SIZE` sentinel.
/// Before the fix every one of these was 2147483647.
///
/// NOTE: the flex item is `laid.children[0]` -- indexing one level deeper
/// lands on the text node inside it and measures the text, not the item.
#[test]
fn no_flex_item_leaks_the_unconstrained_size_sentinel() {
    for child_style in [
        "flex: none;",
        "flex: 0 0 auto;",
        "flex: 0 0 20px;",
        "flex: 0 0 20%;",
        "flex: 0 0 content;",
        "flex: 0 0 max-content;",
    ] {
        let laid = compute_layout(&row_indefinite_cross(W, child_style, "x"), W, 60);
        let w = laid.children[0].rect.w;
        println!("{child_style:<22} -> width {w}");
        assert!(
            (0..W).contains(&w),
            "`{child_style}` produced width {w}, which is not a real width"
        );
    }
}

/// A definite `flex-basis` with no explicit `width` must be honoured exactly.
/// Before the fix the basis was computed correctly and then thrown away by the
/// grow/shrink gate, leaving the child at the full line width.
#[test]
fn a_definite_flex_basis_is_honoured_without_an_explicit_width() {
    for (child_style, want) in [("flex: 0 0 20px;", 20), ("flex: 0 0 20%;", 80)] {
        // Both cross-axis cases: with a definite container height the child
        // would otherwise come out that tall, and with an indefinite one the
        // unconstrained sentinel reached the width slot instead.
        let definite = compute_layout(&row(W, child_style, ""), W, 60);
        let definite_w = definite.children[0].rect.w;
        let indefinite = compute_layout(&row_indefinite_cross(W, child_style, ""), W, 60);
        let indefinite_w = indefinite.children[0].rect.w;
        println!(
            "{child_style:<20} -> definite {definite_w}, indefinite {indefinite_w} (want {want})"
        );
        assert_eq!(definite_w, want, "`{child_style}` was not honoured");
        assert_eq!(indefinite_w, want, "`{child_style}` was not honoured");
    }
}

/// The indefinite-cross case used to land on a 128px grid -- `round(row_width /
/// 128) * 128`, exact at every sampled width -- because the child's width was
/// measured against `i32::MAX` and then shrunk proportionally, so tiny f32
/// rounding in a ~2^31 division decided the winner.
///
/// A non-growing child with real text content must be sized to its content and
/// must not move at all as the container grows.
#[test]
fn a_content_sized_flex_item_does_not_move_with_the_container() {
    let child = "flex: none;";
    let mut widths = Vec::new();
    for row_w in [100, 300, 500, 700, 900, 1100, 1200] {
        let laid = compute_layout(&row_indefinite_cross(row_w, child, "hello"), row_w, 60);
        let w = laid.children[0].rect.w;
        widths.push(w);
        println!("row {row_w:>4} -> item {w}");
        assert!(
            w > 0 && w < row_w,
            "at row width {row_w} the child came out {w}"
        );
    }
    let first = widths[0];
    assert!(
        widths.iter().all(|&w| w == first),
        "a content-sized `flex: none` item tracked the container width: {widths:?}"
    );
}

/// A definite container height must never become a child's WIDTH. This is
/// layer 1 of the defect, isolated: a bare, width-less div in a row flex
/// container of definite height used to come out exactly that tall.
///
/// The child's width must be its CONTENT width and be completely independent
/// of the container's height. (Its height is the text line box, which legitimately
/// exceeds a 20px container -- that is not what this test is about.)
#[test]
fn a_row_container_height_is_never_a_childs_width() {
    let mut widths = Vec::new();
    for ch in [20, 40, 60, 90, 120] {
        let row_style = format!("display:flex; width:400px; height:{ch}px;");
        let tree = h(
            "div",
            vec![("style", row_style.as_str())],
            vec![h("div", vec![], vec![VNode::Text("hello".into())])],
        );
        let laid = compute_layout(&tree, W, 200);
        let child = &laid.children[0];
        println!(
            "container height {ch:>3} -> child rect {}x{}",
            child.rect.w, child.rect.h
        );
        widths.push(child.rect.w);
    }
    // The bug made the child's width equal the container's cross size, so the
    // widths tracked the heights: 20, 40, 60, 90, 120. They are now the
    // content width for every one of them.
    //
    // NOTE: a per-iteration `w != ch` assertion would be a FALSE POSITIVE at
    // ch == 40, because the content width of "hello" is itself 40. Constancy
    // is the real invariant, and it is what actually distinguishes the two.
    assert_eq!(
        widths,
        vec![40, 40, 40, 40, 40],
        "child width must be the content width and independent of the container height"
    );
}
