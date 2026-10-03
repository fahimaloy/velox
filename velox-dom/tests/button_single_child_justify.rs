//! T4 — the single-child `<button>` hack must not discard `justify-content`.
//!
//! `velox-dom/src/layout.rs`'s shared layout tail centres a `<button>`'s only
//! child VERTICALLY (browsers do that by default via the UA button box) and,
//! historically, also rewrote its `x` from `text-align` alone. The flex pass
//! had already computed the correct `x` from `justify-content`
//! (`main_start + extra_space / 2.0`) — the hack overwrote it, so
//! `justify-content: center` was silently ignored on every single-child button
//! that lacked `text-align`.
//!
//! The rule these tests pin: **`text-align` wins when it is PRESENT, otherwise
//! the flow's own horizontal position survives.** The discriminator is
//! presence, not `== "center"` — `text-align: right` has to keep working.
//!
//! Presence INCLUDES an inherited value. `text-align` is in velox-style's
//! `INHERITABLE` set (`velox-style/src/lib.rs:769`), so the cascade has already
//! copied an ancestor's declaration into the button's computed style string by
//! the time `layout.rs` reads it — and a browser really does align a button's
//! inline content by an inherited `text-align`. See
//! `inherited_text_align_counts_as_explicit`.

use velox_dom::layout::{LayoutNode, compute_layout};
use velox_dom::{Props, VNode, h, text};
use velox_style::{Stylesheet, apply_with_cascade};

// ===== harness ==============================================================

/// Lay out a `<button>` with the given style and children, and hand back both
/// the button box and its laid-out children.
fn button(style: &str, kids: Vec<VNode>) -> LayoutNode {
    compute_layout(&h("button", Props::from_inline(style), kids), 200, 100)
}

// ===== tests ================================================================

/// The headline defect: `justify-content: center` and no `text-align`.
/// Today the glyph sits at the content box's LEFT edge.
#[test]
fn flex_justify_content_center_centres_single_child_glyph() {
    let lo = button(
        "display:flex;width:80px;height:30px;justify-content:center",
        vec![text("\u{d7}")],
    );
    let child = &lo.children[0];
    let (cx, cw) = (lo.rect.x, lo.rect.w);

    assert_eq!(
        child.rect.x,
        cx + (cw - child.rect.w) / 2,
        "justify-content:center must centre the glyph: got x={} want {}",
        child.rect.x,
        cx + (cw - child.rect.w) / 2
    );
    assert_ne!(child.rect.x, cx, "the glyph must not stay pinned left");
}

/// `text-align: center` alongside `justify-content: center` — no regression.
#[test]
fn text_align_center_still_centres_alongside_justify_content_center() {
    let lo = button(
        "display:flex;width:80px;height:30px;justify-content:center;text-align:center",
        vec![text("\u{d7}")],
    );
    let child = &lo.children[0];
    assert_eq!(child.rect.x, (lo.rect.w - child.rect.w) / 2);
}

/// The subtlety. `text-align: right` must beat `justify-content: center`.
/// A presence test passes this; an `align == "center"` test does not.
#[test]
fn text_align_right_wins_over_justify_content_center() {
    let lo = button(
        "display:flex;width:80px;height:30px;justify-content:center;text-align:right",
        vec![text("\u{d7}")],
    );
    let child = &lo.children[0];
    assert_eq!(
        child.rect.x,
        lo.rect.w - child.rect.w,
        "text-align:right must put the glyph at the right edge: got x={} w={} box={}",
        child.rect.x,
        child.rect.w,
        lo.rect.w
    );
}

/// `justify-content: flex-end`, no `text-align` — the flex pass's own answer
/// must survive.
#[test]
fn flex_end_without_text_align_puts_glyph_at_the_right_edge() {
    let lo = button(
        "display:flex;width:80px;height:30px;justify-content:flex-end",
        vec![text("\u{d7}")],
    );
    let child = &lo.children[0];
    assert_eq!(
        child.rect.x,
        lo.rect.w - child.rect.w,
        "justify-content:flex-end must put the glyph at the right edge: got x={}",
        child.rect.x
    );
}

/// Non-flex, single child, no `text-align`, no `justify-content`: the fix must
/// not move anything here. Both the block flow and the hack agree on
/// `content_x + 0`, so this pins today's numbers exactly.
#[test]
fn non_flex_single_child_button_without_text_align_is_unchanged() {
    let lo = button("width:80px;height:30px", vec![text("\u{d7}")]);
    let child = &lo.children[0];
    assert_eq!(child.rect.x, 0, "block flow + no text-align == content_x");
    // ... and the vertical centring the hack exists for is still there.
    assert_eq!(child.rect.y, (lo.rect.h - child.rect.h) / 2);
}

/// Non-flex with an explicit `text-align: center`: the fix must still honour it.
/// This is the guard for the "only apply when `display == flex`" mutation —
/// with that gate, a NON-flex button's `text-align: center` is dropped and this
/// test goes RED.
#[test]
fn non_flex_button_with_text_align_center_still_centres() {
    let lo = button(
        "width:80px;height:30px;text-align:center",
        vec![text("\u{d7}")],
    );
    let child = &lo.children[0];
    assert_eq!(
        child.rect.x,
        (lo.rect.w - child.rect.w) / 2,
        "text-align:center on a non-flex button must still centre the glyph: got x={} want {}",
        child.rect.x,
        (lo.rect.w - child.rect.w) / 2
    );
}

/// `text-align: left` is present and is not `center`/`right`: the glyph still
/// pins left. This is the arm a `== "center"` discriminator silently changes.
#[test]
fn explicit_text_align_left_still_pins_left() {
    let lo = button(
        "width:80px;height:30px;text-align:left",
        vec![text("\u{d7}")],
    );
    assert_eq!(lo.children[0].rect.x, 0);
}

/// Multi-child button: the hack is skipped (children.len() == 1 is false) and
/// must STAY skipped, so the flex pass's positions reach the children intact.
/// This is the shape `.toggle` / `.check` have in the real template.
#[test]
fn multi_child_button_skips_the_hack() {
    let lo = button(
        "display:flex;width:80px;height:30px;justify-content:center",
        vec![text("A"), text("B")],
    );
    assert_eq!(lo.children.len(), 2);
    let (a, b) = (&lo.children[0], &lo.children[1]);
    assert!(
        a.rect.x > 0,
        "two 10px items centred in 80px must start after x=0, got {}",
        a.rect.x
    );
    assert_eq!(b.rect.x, a.rect.x + a.rect.w, "the items stay side by side");
    // Vertical centring belongs to the hack, so with two children it must NOT
    // be applied — proof the guard is still `children.len() == 1`.
    assert_eq!(a.rect.y, 0);
}

/// M4's anchor. A NON-flex single-child button gets its vertical centring ONLY
/// from the hack's `child.rect.y = elem_y + bt + pt + offset_y`; neither the
/// block flow nor flex does it here. Removing the vertical half turns this RED.
#[test]
fn vertical_centre_of_a_non_flex_single_child_button_is_kept() {
    let lo = button("width:80px;height:30px", vec![text("\u{d7}")]);
    let child = &lo.children[0];
    assert!(
        child.rect.h < lo.rect.h,
        "test is vacuous unless the glyph is shorter than the box: h={} box={}",
        child.rect.h,
        lo.rect.h
    );
    assert_eq!(
        child.rect.y,
        (lo.rect.h - child.rect.h) / 2,
        "the glyph must be vertically centred in the button"
    );
}

/// The real `.remove` control from `velox-cli/templates/project/src/components/
/// TodoItem.vx:154-168`: `display:flex; justify-content:center; width:26px;
/// height:26px; padding:0; border:1px solid`. Content box is 26x26 inset by the
/// 1px border, so the glyph belongs at `1 + (26 - w) / 2`, not at 1.
#[test]
fn todo_remove_glyph_is_centred_in_its_content_box() {
    let lo = button(
        "display:flex;align-items:center;justify-content:center;\
         width:26px;height:26px;padding:0;border:1px solid transparent;\
         font-size:16px;line-height:1",
        vec![text("\u{d7}")],
    );
    // Border-box is 26 + 2*1 = 28; content starts at x=1 and is 26 wide.
    assert_eq!(lo.rect.x, 0);
    assert_eq!(lo.rect.w, 28);
    let child = &lo.children[0];
    assert_eq!(
        child.rect.x,
        1 + (26 - child.rect.w) / 2,
        "the `.remove` glyph must be centred in its 26px content box: got x={} w={}",
        child.rect.x,
        child.rect.w
    );
}

/// Documents the inheritance decision: `text-align` is in velox-style's
/// `INHERITABLE` set, so an ancestor's `text-align: right` is a real author
/// request reaching the button's inline content — exactly as in a browser —
/// and therefore counts as PRESENT.
#[test]
fn inherited_text_align_counts_as_explicit() {
    // `border: none; padding: 0` neutralise the UA button box so the content box
    // is the border box and the expected x is unambiguous.
    let sheet = Stylesheet::parse(
        ".wrap { text-align: right; }\
         .b { display:flex; width:80px; height:30px; justify-content:center;\
              border:none; padding:0; }",
    );
    let tree = h(
        "div",
        Props::from_class("wrap"),
        vec![h("button", Props::from_class("b"), vec![text("\u{d7}")])],
    );
    let lo = compute_layout(&apply_with_cascade(&tree, &sheet), 200, 100);

    let b = lo
        .children
        .iter()
        .find(|c| c.source_index == Some(0))
        .expect("button");
    assert_eq!(
        (b.rect.x, b.rect.w),
        (0, 80),
        "the button must be 80 wide with no border or padding: got {:?} {:?}",
        b.rect.x,
        b.rect.w
    );
    let child = &b.children[0];
    assert_eq!(
        child.rect.x,
        b.rect.w - child.rect.w,
        "an inherited text-align:right must right-align the glyph: got x={} box={} w={}",
        child.rect.x,
        b.rect.w,
        child.rect.w
    );
}

/// The gate that catches "only apply the fix when `display == flex`".
///
/// A non-flex button whose single child is an ELEMENT (not a text node). The
/// block/inline flow applies `text-align` to text runs, so a text-child button
/// would centre correctly even with the fix disabled; an element child gets no
/// such treatment, so the hack is the only thing that positions it and gating
/// the hack on `display == flex` drops the alignment entirely.
#[test]
fn non_flex_button_element_child_honours_text_align() {
    let child = || {
        h(
            "span",
            Props::from_inline("display:block;width:8px;height:8px"),
            vec![],
        )
    };

    let centred = button("width:80px;height:30px;text-align:center", vec![child()]);
    assert_eq!(
        centred.children[0].rect.x,
        (centred.rect.w - centred.children[0].rect.w) / 2,
        "a non-flex button with an ELEMENT child must still honour text-align:center: got x={}",
        centred.children[0].rect.x
    );

    let right = button("width:80px;height:30px;text-align:right", vec![child()]);
    assert_eq!(
        right.children[0].rect.x,
        right.rect.w - right.children[0].rect.w,
        "a non-flex button with an ELEMENT child must still honour text-align:right: got x={}",
        right.children[0].rect.x
    );

    // And with no text-align at all, nothing moves it: it stays at content_x.
    let plain = button("width:80px;height:30px", vec![child()]);
    assert_eq!(plain.children[0].rect.x, 0);
}
