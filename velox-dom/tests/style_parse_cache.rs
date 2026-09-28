//! Task 2.1c: guard for the content-keyed style parse cache in
//! `velox-dom/src/layout.rs`.
//!
//! The cache memoises the `split(';')` walk of a style string and the
//! resolution of a declaration, so the risk it introduces is a key that omits
//! an input the parse actually reads. These tests pin the semantics that a
//! wrong key would silently break:
//!
//!   * first-occurrence wins for the lookup helpers, LAST-occurrence wins for
//!     `margin` auto sides (its loop assigns rather than returning), and
//!   * the resolved-value memos are keyed on the size context, so the same
//!     style string under two different containing blocks resolves twice.
//!
//! Every expectation below was captured from BASE (commit 39b406d) by running
//! the same battery through `compute_layout` before and after the change and
//! diffing; see `.superpowers/sdd/2026-09-28-velox-remediation-plan/gates/`
//! `2.1c-diff-base.txt` vs `2.1c-diff-new.txt` (identical, 81/81 lines).

use velox_dom::layout::compute_layout;
use velox_dom::{Props, VNode};

fn el(style: &str, kids: Vec<VNode>) -> VNode {
    VNode::Element {
        tag: "div".to_string(),
        props: Props::from_inline(style),
        children: kids,
    }
}

fn root_rect(style: &str) -> (i32, i32, i32, i32) {
    let lo = compute_layout(&el(style, vec![]), 1024, 768);
    let r = lo.rect;
    (r.x, r.y, r.w, r.h)
}

/// The lookup helpers `return` on the first matching declaration, so a cache
/// that served the last one would size this 99px wide instead of 10px.
#[test]
fn first_occurrence_wins_for_lookup_helpers() {
    assert_eq!(
        root_rect("width: 10px; width: 99px; height: 10px; height: 99px"),
        (0, 0, 10, 10)
    );
}

/// `style_margin_auto_sides` ASSIGNS on every match instead of returning, so
/// for the `margin` SHORTHAND the LAST occurrence wins. This is the one helper
/// whose semantics differ from the rest of the family, and the reason the
/// table keeps both a `first` and a `last` index.
///
/// Note the contrast with the longhand below: `margin-left: 1px; margin-left:
/// 7px` resolves to 1, because the *value* of `margin-left` comes from the
/// first-occurrence lookup path, and only the auto-detection consults the
/// last-occurrence helper. A cache that served `last` everywhere would break
/// this test; one that served `first` everywhere would break the shorthand one.
#[test]
fn margin_shorthand_takes_the_last_occurrence() {
    assert_eq!(
        root_rect("margin: 0 10px; margin: 0 auto; width: 100px; height: 20px"),
        (462, 0, 100, 20),
        "the later `margin: 0 auto` wins, so the box is centred"
    );
    assert_eq!(
        root_rect("margin: 0 auto; margin: 0 10px; width: 100px; height: 20px"),
        (0, 0, 100, 20),
        "the later non-auto `margin` wins, so the box is not centred"
    );
}

/// The counter-case: the `margin-left` VALUE is resolved by the
/// first-occurrence path, so the earlier declaration wins.
#[test]
fn margin_longhand_value_takes_the_first_occurrence() {
    let (x, ..) = root_rect(
        "margin-left: 1px; margin-left: 7px; margin-right: 2px; margin-right: 9px; width: 40px; height: 10px",
    );
    assert_eq!(x, 1, "first margin-left wins for the resolved value");
}

/// Shorthand expansion keeps CSS's 1/2/3/4-value forms.
#[test]
fn shorthand_expansion_keeps_all_four_value_forms() {
    assert_eq!(root_rect("margin: 1px; width: 100px").0, 1);
    assert_eq!(root_rect("margin: 1px 2px; width: 100px").0, 2);
    assert_eq!(root_rect("margin: 1px 2px 3px; width: 100px").0, 2);
    assert_eq!(root_rect("margin: 1px 2px 3px 4px; width: 100px").0, 4);
    // 5 values is not a valid shorthand, so no margin is applied at all.
    assert_eq!(root_rect("margin: 1px 2px 3px 4px 5px; width: 100px").0, 0);
}

/// `auto` in each shorthand position centres the box.
#[test]
fn margin_auto_centres() {
    let want = (462, 0, 100, 768);
    for s in [
        "margin: auto; width: 100px",
        "margin: 0 auto; width: 100px",
        "margin: 0 auto 0 auto; width: 100px",
    ] {
        assert_eq!(root_rect(s), want, "style: {s}");
    }
}

/// A longhand overrides the shorthand regardless of the order the two appear
/// in. Both orders were measured at BASE; they agree.
#[test]
fn longhand_overrides_shorthand_in_either_order() {
    let want = (0, 0, 122, 54);
    assert_eq!(
        root_rect("padding: 1px 2px 3px 4px; padding-left: 20px; width: 100px; height: 50px"),
        want
    );
    assert_eq!(
        root_rect("padding-left: 20px; padding: 1px 2px 3px 4px; width: 100px; height: 50px"),
        want
    );
}

/// Every unit resolves against the input it actually reads: `%` against the
/// containing block, `em` against the parent font size, `rem` against the
/// root font size, and `vh`/`dvh`/`dvw` against the viewport.
#[test]
fn all_length_units_resolve_against_their_own_input() {
    // font-size 20px => 1em padding = 20, 1rem = 16, 10vh of 768 = 76,
    // 5dvh = 38, 10dvw of 1024 = 102, 2dvw = 20. Height pins the vh+dvh+rem.
    assert_eq!(
        root_rect(
            "font-size: 20px; padding: 1em; width: 50%; height: 10vh; min-height: 5dvh; \
             max-width: 10dvw; border-width: 2dvw; border-top-width: 1rem"
        ),
        (0, 0, 174, 145)
    );
}

/// The resolved-value memos key on the size context. The same style string on
/// two siblings of DIFFERENT widths must resolve to different pixels; a memo
/// keyed on the style string alone would return one sibling's value for both.
#[test]
fn same_style_under_different_containing_blocks_resolves_per_parent() {
    let tree = el(
        "width: 400px; height: 10px",
        vec![
            el("width: 50%; height: 5px", vec![]),
            el("width: 50%; height: 5px", vec![]),
        ],
    );
    // Identical styles, identical parent width -> identical results, and the
    // memo may legitimately be shared here.
    let lo = compute_layout(&tree, 1024, 768);
    assert_eq!(lo.children[0].rect.w, 200);
    assert_eq!(lo.children[1].rect.w, 200);

    // Now the SAME style string under two different containing blocks. Each
    // parent is itself a child, so the 25% child of each must differ.
    let tree = el(
        "width: 400px; height: 100px",
        vec![
            el(
                "width: 50%; height: 50px",
                vec![el("width: 25%; height: 10px", vec![])],
            ),
            el(
                "width: 200px; height: 50px",
                vec![el("width: 25%; height: 10px", vec![])],
            ),
        ],
    );
    let lo = compute_layout(&tree, 1024, 768);
    let a = lo.children[0].children[0].rect.w;
    let b = lo.children[1].children[0].rect.w;
    assert_eq!(a, 50, "25% of 200");
    assert_eq!(b, 50, "25% of 200");
    assert_eq!(lo.children[0].rect.w, 200);
    assert_eq!(lo.children[1].rect.w, 200);
}

/// A percentage must follow ITS OWN parent, not an enclosing block. These two
/// carry the identical style string under parents of different widths, so if
/// the memo were keyed without `parent_size` the second would reuse the first.
#[test]
fn percentage_follows_its_own_parent_not_an_ancestor() {
    let tree = el(
        "width: 400px; height: 200px",
        vec![
            el("width: 50%; height: 20px", vec![]),
            el(
                "width: 100px; height: 20px",
                vec![el("width: 50%; height: 20px", vec![])],
            ),
        ],
    );
    let lo = compute_layout(&tree, 1024, 768);
    assert_eq!(lo.children[0].rect.w, 200, "50% of 400");
    assert_eq!(
        lo.children[1].children[0].rect.w, 50,
        "50% of 100, not of 400"
    );
}

/// `em` resolves against the PARENT's font size, not the element's own. Both
/// children below declare their own `font-size` and both declare
/// `padding: 1em`, but since the parent's font size is 10px for both, 1em is
/// 10px for both and the two boxes have the SAME height. A memo keyed on the
/// element's own font size would wrongly make these differ, and one keyed on
/// the parent would pass here but fail `em_inherits_from_parent_font_size`.
#[test]
fn em_uses_parent_font_size_not_own() {
    let tree = el(
        "font-size: 10px; width: 200px; height: 60px",
        vec![
            el("font-size: 30px; padding: 1em; height: 5px", vec![]),
            el("font-size: 40px; padding: 1em; height: 5px", vec![]),
        ],
    );
    let lo = compute_layout(&tree, 1024, 768);
    assert_eq!(lo.children[0].rect.h, 25, "5 + 2*10 (parent font size)");
    assert_eq!(lo.children[1].rect.h, 25, "5 + 2*10 (parent font size)");
}

/// ...and the converse: one `padding: 1em` string under two DIFFERENT parent
/// font sizes resolves differently. This is the pair of assertions that
/// together pin `parent_font_size` into the memo key.
#[test]
fn em_inherits_from_parent_font_size() {
    let tree = el(
        "font-size: 10px; width: 200px; height: 200px",
        vec![
            el(
                "font-size: 30px; width: 200px; height: 50px",
                vec![el("padding: 1em; width: 50px; height: 5px", vec![])],
            ),
            el(
                "font-size: 40px; width: 200px; height: 50px",
                vec![el("padding: 1em; width: 50px; height: 5px", vec![])],
            ),
        ],
    );
    let lo = compute_layout(&tree, 1024, 768);
    let a = lo.children[0].children[0].rect;
    let b = lo.children[1].children[0].rect;
    assert_eq!((a.w, a.h), (110, 65), "50 + 2*30, 5 + 2*30");
    assert_eq!((b.w, b.h), (130, 85), "50 + 2*40, 5 + 2*40");
    assert_ne!(
        a.w, b.w,
        "the same style string must resolve per parent font size"
    );
}

/// `rem` resolves against the ROOT font size only, so these two agree even
/// though their parents' font sizes differ. The counter-test to the `em` one.
#[test]
fn rem_ignores_parent_font_size() {
    let tree = el(
        "font-size: 10px; width: 300px; height: 200px",
        vec![
            el(
                "font-size: 30px; width: 300px; height: 20px",
                vec![el("padding: 1rem; width: 40px; height: 5px", vec![])],
            ),
            el(
                "font-size: 40px; width: 300px; height: 20px",
                vec![el("padding: 1rem; width: 40px; height: 5px", vec![])],
            ),
        ],
    );
    let lo = compute_layout(&tree, 1024, 768);
    for (i, c) in [0usize, 1].iter().enumerate() {
        let g = lo.children[*c].children[0].rect;
        assert_eq!((g.w, g.h), (72, 37), "case {i}: 40 + 2*16, 5 + 2*16");
    }
}

/// Viewport units resolve against the viewport, which is not a property of the
/// tree at all, so it is the input most likely to be forgotten in a key.
#[test]
fn viewport_units_use_the_viewport() {
    // 1024 x 768 viewport: 50vw = 512, 25vh = 192, 10vw = 102.
    for s in [
        "width: 50vw; height: 25vh; width: 200px",
        "width: 50dvw; height: 25dvh; width: 200px",
    ] {
        let lo = compute_layout(
            &el(s, vec![el("width: 10vw; height: 5px", vec![])]),
            1024,
            768,
        );
        assert_eq!((lo.rect.w, lo.rect.h), (512, 192), "style: {s}");
        assert_eq!(lo.children[0].rect.w, 102, "10vw of 1024, style: {s}");
    }
}

/// A percentage resolves against ITS OWN containing block. The two
/// grandchildren carry the identical style string under parents of different
/// widths, so a memo that omitted `parent_size` would return one of these
/// twice.
#[test]
fn percentage_memo_is_keyed_on_the_containing_block() {
    let tree = el(
        "width: 400px; height: 200px",
        vec![
            el(
                "width: 200px; height: 20px",
                vec![el("padding: 25%; width: 10px; height: 5px", vec![])],
            ),
            el(
                "width: 80px; height: 20px",
                vec![el("padding: 25%; width: 10px; height: 5px", vec![])],
            ),
        ],
    );
    let lo = compute_layout(&tree, 1024, 768);
    let a = lo.children[0].children[0].rect;
    let b = lo.children[1].children[0].rect;
    assert_eq!((a.w, a.h), (110, 105), "25% of 200 => 50; 10+2*50, 5+2*50");
    assert_eq!((b.w, b.h), (50, 45), "25% of 80 => 20; 10+2*20, 5+2*20");
    assert_ne!(
        a.w, b.w,
        "identical style strings, different containing blocks"
    );
}

/// Border percentages take the same key, through `style_border_widths`.
#[test]
fn border_percentage_memo_is_keyed_on_the_containing_block() {
    let tree = el(
        "width: 400px; height: 200px",
        vec![
            el(
                "width: 200px; height: 20px",
                vec![el("border-width: 5%; width: 20px; height: 10px", vec![])],
            ),
            el(
                "width: 100px; height: 20px",
                vec![el("border-width: 5%; width: 20px; height: 10px", vec![])],
            ),
        ],
    );
    let lo = compute_layout(&tree, 1024, 768);
    let a = lo.children[0].children[0].rect;
    let b = lo.children[1].children[0].rect;
    assert_eq!((a.w, a.h), (40, 30), "5% of 200 => 10");
    assert_eq!((b.w, b.h), (30, 20), "5% of 100 => 5");
}

/// Border widths: the `border` shorthand, `border-width`, the four individual
/// longhands, and a longhand overriding the shorthand.
#[test]
fn border_width_shorthand_and_longhand() {
    assert_eq!(
        root_rect("border: 3px solid red; width: 100px; height: 30px"),
        (0, 0, 106, 36)
    );
    assert_eq!(
        root_rect("border-width: 2px; width: 100px; height: 30px"),
        (0, 0, 104, 34)
    );
    assert_eq!(
        root_rect(
            "border-left-width: 1px; border-top-width: 2px; border-right-width: 3px; \
             border-bottom-width: 4px; width: 100px; height: 30px"
        ),
        (0, 0, 104, 36)
    );
    // The longhand wins on the left, the shorthand supplies the other three.
    assert_eq!(
        root_rect("border: 2px solid red; border-left-width: 7px; width: 100px; height: 30px"),
        (0, 0, 109, 34)
    );
    assert_eq!(
        root_rect(
            "border-left: 5px solid red; border-top: 6px solid red; width: 100px; height: 30px"
        ),
        (0, 0, 105, 36)
    );
    // A percentage border resolves against the containing block.
    assert_eq!(
        root_rect("border-width: 10%; width: 200px; height: 30px"),
        (0, 0, 404, 234)
    );
}

/// Empty declarations, declarations with no colon, and empty values are all
/// skipped, and a later valid declaration is still found.
#[test]
fn malformed_declarations_are_skipped() {
    assert_eq!(
        root_rect(";;  ; width : 20px ;;height: 12px;   ; nonsense ; width: 3px;"),
        (0, 0, 20, 12),
        "empty decls and colon-less decls are skipped; the key is trimmed"
    );
    assert_eq!(root_rect("width:; height:; width: 8px"), (0, 0, 1024, 768));
    assert_eq!(root_rect(""), (0, 0, 1024, 768));
}

/// Property names are compared case-sensitively, so `Width` is not `width`.
#[test]
fn property_names_are_case_sensitive() {
    assert_eq!(
        root_rect("Width: 50px; width: 7px; HEIGHT: 9px; height: 8px"),
        (0, 0, 7, 8)
    );
}

/// The cache is content-keyed, so three siblings sharing one style string
/// under one parent all lay out identically, and the one that differs does so
/// only through its content.
#[test]
fn identical_sibling_styles_share_a_cache_entry() {
    let tree = el(
        "width: 300px; height: 10px",
        vec![
            el("padding: 10%; width: 50px", vec![el("height: 4px", vec![])]),
            el("padding: 10%; width: 50px", vec![el("height: 4px", vec![])]),
            el("padding: 10%; width: 50px", vec![el("height: 4px", vec![])]),
        ],
    );
    let lo = compute_layout(&tree, 1024, 768);
    let w0 = lo.children[0].rect.w;
    for (i, c) in lo.children.iter().enumerate() {
        assert_eq!(c.rect.w, w0, "sibling {i} shares the style string");
    }
}

// ---------------------------------------------------------------------------
// Context-variation guard.
//
// Everything above lays out ONE tree. These lay the SAME style string out
// repeatedly with different ambient context, which is the only way to falsify
// a key component: the memo is a `thread_local` that survives from one
// `compute_layout` call to the next, so an input left out of the key shows up
// as the second layout silently reusing the first one's value.
//
// This is not a synthetic shape. It is a resize, or a font-size change, or a
// route change re-laying the same subtree. Those are ordinary events.
// ---------------------------------------------------------------------------

/// Lay out `n` at the given viewport and return the rects of its children.
fn lay(n: &VNode, vw: i32, vh: i32) -> Vec<(i32, i32, i32, i32)> {
    compute_layout(n, vw, vh)
        .children
        .iter()
        .map(|c| (c.rect.x, c.rect.y, c.rect.w, c.rect.h))
        .collect()
}

/// Lay out a single childless element and return its own rect. Separate from
/// [`lay`] on purpose: these trees carry the style under test on the root, so
/// reading `children` would return an empty vec and assert nothing.
fn lay_root(n: &VNode, vw: i32, vh: i32) -> (i32, i32, i32, i32) {
    let r = compute_layout(n, vw, vh).rect;
    (r.x, r.y, r.w, r.h)
}

/// `2em` resolves against the PARENT's font size, so the same style string
/// under a 20px parent and a 40px parent must resolve twice. Dropping
/// `parent_font_size` from the length memo makes both return 40.
#[test]
fn the_same_string_under_two_parent_font_sizes_resolves_twice() {
    let mk = |pfs: &str| {
        el(
            &format!("font-size: {pfs}; width: 400px; height: 5px"),
            vec![el("width: 2em; height: 3px", vec![])],
        )
    };
    assert_eq!(lay(&mk("20px"), 1024, 768), vec![(0, 0, 40, 3)]);
    assert_eq!(lay(&mk("40px"), 1024, 768), vec![(0, 0, 80, 3)]);
}

/// `vw`/`vh` resolve against the viewport, so a resize must invalidate. Three
/// viewports rather than two: a key that is merely incomplete, but degenerate
/// over the values a two-step test happens to use, would still pass a pair.
#[test]
fn a_resize_invalidates_viewport_units() {
    let t = el("width: 10vw; height: 4px", vec![]);
    assert_eq!(lay_root(&t, 1000, 500), (0, 0, 100, 4));
    assert_eq!(lay_root(&t, 2000, 500), (0, 0, 200, 4));
    assert_eq!(lay_root(&t, 3000, 500), (0, 0, 300, 4));
}

/// The dynamic viewport units take the same path and must invalidate the same
/// way; `10dvw` of 2048 is 204.8, which rounds to 205.
#[test]
fn a_resize_invalidates_dynamic_viewport_units() {
    let t = el("width: 10dvw; height: 5dvh", vec![]);
    assert_eq!(lay_root(&t, 1024, 768), (0, 0, 102, 38));
    assert_eq!(lay_root(&t, 2048, 1536), (0, 0, 205, 77));
}

/// The `border-width` memo carries the same context as the length memo, and is
/// a separate cache, so it needs its own guard: 2em of a 20px parent is a 40px
/// border on a 200x60 box, and 2em of a 40px parent is an 80px one.
#[test]
fn a_parent_font_size_change_invalidates_the_border_memo() {
    let mk = |pfs: &str| {
        el(
            &format!("font-size: {pfs}; width: 400px; height: 5px"),
            vec![el("border-width: 2em; width: 200px; height: 60px", vec![])],
        )
    };
    assert_eq!(lay(&mk("20px"), 1024, 768), vec![(0, 0, 280, 140)]);
    assert_eq!(lay(&mk("40px"), 1024, 768), vec![(0, 0, 360, 220)]);
}

/// And the border memo's viewport input, again separately from the length memo.
#[test]
fn a_resize_invalidates_viewport_units_in_the_border_memo() {
    let t = el("border-width: 2vw; width: 400px; height: 60px", vec![]);
    assert_eq!(lay_root(&t, 1024, 768), (0, 0, 440, 100));
    assert_eq!(lay_root(&t, 2048, 1536), (0, 0, 482, 142));
}

/// The box-sides memo (`padding` shorthand expansion) is a third cache with a
/// third key, and needs its own guard for the same reason.
#[test]
fn a_resize_invalidates_viewport_units_in_the_padding_memo() {
    let t = el("padding: 2vw; width: 400px; height: 60px", vec![]);
    assert_eq!(lay_root(&t, 1024, 768), (0, 0, 440, 100));
    assert_eq!(lay_root(&t, 2048, 1536), (0, 0, 482, 142));
}

/// The `font-size` memo is a fourth cache. It is only observable when something
/// downstream actually consumes the size, so the child pads in `em`: the
/// parent's font size has to reach the resolution twice, once to size the font
/// and once to pad the box.
#[test]
fn a_parent_font_size_change_invalidates_the_font_size_memo() {
    let mk = |pfs: &str| {
        el(
            &format!("font-size: {pfs}; width: 400px; height: 5px"),
            vec![el(
                "font-size: 2em; padding: 1em; width: 50px; height: 5px",
                vec![],
            )],
        )
    };
    assert_eq!(lay(&mk("20px"), 1024, 768), vec![(0, 0, 90, 45)]);
    assert_eq!(lay(&mk("40px"), 1024, 768), vec![(0, 0, 130, 85)]);
}

/// The same for the font-size memo's viewport input.
#[test]
fn a_resize_invalidates_viewport_units_in_the_font_size_memo() {
    let t = || {
        el(
            "font-size: 5vw; width: 400px; height: 5px",
            vec![el("padding: 1em; width: 50px; height: 5px", vec![])],
        )
    };
    assert_eq!(lay(&t(), 1024, 768), vec![(0, 0, 152, 107)]);
    assert_eq!(lay(&t(), 2048, 1536), vec![(0, 0, 254, 209)]);
}

// --- the one component that CANNOT be falsified ---------------------------
//
// `root_font_size` appears in all four memos, and no test can falsify it,
// because it is not ambient at all: `compute_layout` seeds the single `at()`
// call with the literal `DEFAULT_ROOT_FONT_SIZE` and threads it unchanged
// through the entire descent. It is 16.0 for every node in every pass.
//
// These two tests therefore assert the OPPOSITE of the ones above: that a
// change to the root's own `font-size` does NOT move `rem`. They are here to
// pin the reason the key component is unfalsifiable. If `root_font_size` ever
// stops being a constant -- if the root's own font-size starts feeding it --
// these fail, and that is the moment the key component becomes testable.

#[test]
fn rem_ignores_the_roots_own_font_size_because_root_font_size_is_constant() {
    // Both layouts declare a different font-size on the root element, and both
    // must still resolve 1rem against the fixed 16px default.
    let a = el(
        "font-size: 20px; width: 400px; height: 5px",
        vec![el("padding: 1rem; width: 40px; height: 5px", vec![])],
    );
    let b = el(
        "font-size: 40px; width: 400px; height: 5px",
        vec![el("padding: 1rem; width: 40px; height: 5px", vec![])],
    );
    assert_eq!(lay(&a, 1024, 768), vec![(0, 0, 72, 37)]);
    assert_eq!(lay(&b, 1024, 768), vec![(0, 0, 72, 37)]);
    assert_eq!(lay(&a, 1024, 768), lay(&b, 1024, 768));
}

/// The same for the border and length memos, which also carry `root_font_size`.
#[test]
fn rem_in_a_border_is_constant_because_root_font_size_is_constant() {
    let mk = |root_fs: &str| {
        el(
            &format!("font-size: {root_fs}; width: 400px; height: 5px"),
            vec![el("border-width: 2rem; width: 200px; height: 60px", vec![])],
        )
    };
    assert_eq!(lay(&mk("20px"), 1024, 768), vec![(0, 0, 264, 124)]);
    assert_eq!(lay(&mk("40px"), 1024, 768), vec![(0, 0, 264, 124)]);
}

// ---------------------------------------------------------------------------
// Cases added after the falsification sweep (`.superpowers/sdd/2026-09-28-
// velox-remediation-plan/gates/2.1c-falsify.log`) reported that the `vw`, `vh`
// and font-size `parent_font_size` components of the memos were not
// independently observable by any test. Both were weaknesses of THESE tests,
// not of the cache: every viewport test above resizes the ROOT element, and a
// root's `parent_size` IS the viewport, so `parent_size` co-varies with the
// viewport and a memo missing `vw` still produced a different key. The fix is
// to resize a CHILD under a FIXED-WIDTH parent, and to put a GRANDCHILD
// underneath where a child font size is what actually gets consumed (`1em` on
// the child would resolve against the child's PARENT font size instead, so the
// child's own memoised size would go unread).
//
// All expectations are BASE (39b406d) values, diffed over
// `2.1c-diff-base4.txt` vs `2.1c-diff-new4.txt` (247/247 lines identical).
// ---------------------------------------------------------------------------

/// Descend `depth` levels from the root and return that node's rect.
fn lay_at(n: &VNode, depth: usize, vw: i32, vh: i32) -> (i32, i32, i32, i32) {
    let mut lo = compute_layout(n, vw, vh);
    for _ in 0..depth {
        lo = lo.children[0].clone();
    }
    let r = lo.rect;
    (r.x, r.y, r.w, r.h)
}

/// A child carrying `child`'s style under a 200px-wide, 5px-tall parent. The
/// parent's width is FIXED so the containing block cannot stand in for the
/// viewport in any memo key.
fn under_fixed_parent(child: &str) -> VNode {
    el("width: 200px; height: 5px", vec![el(child, vec![])])
}

/// A parent declaring `pfs`, a child sizing itself in `em` off that, and a
/// GRANDCHILD padding in `1em` -- which consumes the child's own memoised font
/// size, the only way that value can go unread otherwise.
fn three_levels(pfs: &str) -> VNode {
    el(
        &format!("font-size: {pfs}; width: 400px; height: 5px"),
        vec![el(
            "font-size: 2em; width: 50px; height: 5px",
            vec![el("padding: 1em; width: 10px; height: 5px", vec![])],
        )],
    )
}

#[test]
fn the_length_memo_is_keyed_on_the_viewport_width() {
    let n = under_fixed_parent("width: 10vw; height: 4px");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 100, 4));
    assert_eq!(lay_at(&n, 1, 2000, 1000), (0, 0, 200, 4));
}

#[test]
fn the_length_memo_is_keyed_on_the_viewport_height() {
    let n = under_fixed_parent("width: 10px; height: 10vh");
    assert_eq!(lay_at(&n, 1, 500, 1500), (0, 0, 10, 150));
    assert_eq!(lay_at(&n, 1, 1500, 500), (0, 0, 10, 50));
}

#[test]
fn the_box_sides_memo_is_keyed_on_the_viewport() {
    let n = under_fixed_parent("padding: 2vw; width: 10px; height: 4px");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 50, 44));
    assert_eq!(lay_at(&n, 1, 2000, 1000), (0, 0, 90, 84));
}

#[test]
fn the_border_memo_is_keyed_on_the_viewport_through_the_shorthand() {
    // `border: <len> solid <colour>` is resolved by `style_border_widths` itself
    // rather than delegated to `style_box_sides_full`, so this is the path that
    // exercises the border memo's OWN viewport components.
    let n = under_fixed_parent("border: 2vw solid red; width: 10px; height: 4px");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 50, 44));
    assert_eq!(lay_at(&n, 1, 2000, 1000), (0, 0, 90, 84));
}

#[test]
fn the_border_memo_is_keyed_on_the_viewport_through_border_width() {
    let n = under_fixed_parent("border-width: 2vw; width: 10px; height: 4px");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 50, 44));
    assert_eq!(lay_at(&n, 1, 2000, 1000), (0, 0, 90, 84));
}

#[test]
fn the_font_size_memo_is_keyed_on_the_parent_font_size() {
    // The grandchild's `1em` padding is the child's own font size, so if the
    // memo forgot `parent_font_size` the 40px parent would reuse the 20px
    // parent's answer and the grandchild would come out 90px wide, not 170px.
    assert_eq!(lay_at(&three_levels("20px"), 2, 1024, 1024), (0, 0, 90, 85));
    assert_eq!(
        lay_at(&three_levels("40px"), 2, 1024, 1024),
        (0, 0, 170, 165)
    );
}

#[test]
fn the_font_size_memo_is_keyed_on_the_viewport() {
    let n = el(
        "width: 200px; height: 5px",
        vec![el(
            "font-size: 5vw; width: 50px; height: 5px",
            vec![el("padding: 1em; width: 10px; height: 5px", vec![])],
        )],
    );
    assert_eq!(lay_at(&n, 2, 1000, 2000), (0, 0, 110, 105));
    assert_eq!(lay_at(&n, 2, 2000, 1000), (0, 0, 210, 205));
}

/// The viewport components of all four memos, exercised through whichever
/// property actually routes through each one.
///
/// This is deliberately a "kitchen sink": the sweep showed that
/// `width: 10vw` alone does NOT reach the length memo (a child's width is
/// resolved on another path), so picking one property per memo by inspection
/// was not working. Instead every viewport-relative property the layout
/// engine consults is declared at once, so any memo reachable from any of
/// them is covered. The two viewports differ 2x, so a memo that dropped `vw`
/// or `vh` would return the first viewport's geometry and fail here.
#[test]
fn every_viewport_unit_re_resolves_when_the_viewport_changes() {
    let mk = || {
        el(
            "width: 400px; height: 60px",
            vec![el(
                "box-sizing: border-box; display: flex; flex-direction: column; \
                 width: 10vw; height: 10vh; min-width: 3vw; max-width: 50vw; \
                 min-height: 2vh; max-height: 40vh; \
                 margin: 1vw; margin-left: 2vw; margin-right: 3vw; \
                 padding: 1vw; padding-top: 2vh; padding-bottom: 2vh; \
                 border-width: 1vw; border: 1vw solid red; border-left-width: 2vw; \
                 border-top: 1vh solid red; gap: 1vw; row-gap: 1vh; \
                 flex: 1; flex-basis: 5vw; \
                 line-height: 2vw; \
                 left: 1vw; right: 2vw; top: 1vh; bottom: 2vh",
                vec![el("width: 4px; height: 4px", vec![])],
            )],
        )
    };
    let a = lay_at(&mk(), 1, 1000, 2000);
    let b = lay_at(&mk(), 1, 2000, 1000);
    assert_ne!(
        a, b,
        "a 2x viewport change must change viewport-relative geometry"
    );
    // Values captured from BASE (39b406d); see `2.1c-diff-base4.txt`.
    assert_eq!(a, (20, 10, 100, 200));
    assert_eq!(b, (40, 20, 200, 100));
}

// ---------------------------------------------------------------------------
// Second round, after the sweep still could not falsify `vw` or `vh` on their
// own. The memos were demonstrably LIVE (probe counts: len 53, box-sides 16,
// border 4, font-size 1 hits over this battery), so the components were not
// dead weight -- the TESTS were non-independent. Every viewport assertion
// above changes the viewport's WIDTH and HEIGHT together, `(1000, 2000)` ->
// `(2000, 1000)`, which means dropping `vw` is masked by `vh` and dropping `vh`
// is masked by `vw`: neither can be falsified on its own. A memo missing only
// `vw` still sees a different `vh` and misses, returning the correct fresh
// answer by accident.
//
// So these vary ONE axis and hold the other fixed. Expectations are BASE
// (39b406d) values.
// ---------------------------------------------------------------------------

#[test]
fn the_length_memo_is_keyed_on_viewport_width_alone() {
    // Height held at 2000 so only `vw` differs.
    let n = under_fixed_parent("width: 10vw; height: 10vh; min-width: 3vw");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 100, 200));
    assert_eq!(lay_at(&n, 1, 2000, 2000), (0, 0, 200, 200));
}

#[test]
fn the_length_memo_is_keyed_on_viewport_height_alone() {
    // Width held at 1000 so only `vh` differs.
    let n = under_fixed_parent("width: 10vw; height: 10vh; max-height: 40vh");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 100, 200));
    assert_eq!(lay_at(&n, 1, 1000, 1000), (0, 0, 100, 100));
}

#[test]
fn the_box_sides_memo_is_keyed_on_viewport_width_alone() {
    let n = under_fixed_parent("padding: 2vw; width: 10px; height: 4px");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 50, 44));
    assert_eq!(lay_at(&n, 1, 2000, 2000), (0, 0, 90, 84));
}

#[test]
fn the_box_sides_memo_is_keyed_on_viewport_height_alone() {
    let n = under_fixed_parent("padding: 2vh; width: 10px; height: 4px");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 90, 84));
    assert_eq!(lay_at(&n, 1, 1000, 1000), (0, 0, 50, 44));
}

#[test]
fn the_border_memo_is_keyed_on_viewport_width_alone() {
    let n = under_fixed_parent("border: 2vw solid red; width: 10px; height: 4px");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 50, 44));
    assert_eq!(lay_at(&n, 1, 2000, 2000), (0, 0, 90, 84));
}

#[test]
fn the_border_memo_is_keyed_on_viewport_height_alone() {
    let n = under_fixed_parent("border-width: 2vh; width: 10px; height: 4px");
    assert_eq!(lay_at(&n, 1, 1000, 2000), (0, 0, 90, 84));
    assert_eq!(lay_at(&n, 1, 1000, 1000), (0, 0, 50, 44));
}

#[test]
fn the_font_size_memo_is_keyed_on_viewport_width_alone() {
    // Grandchild `1em` padding consumes the child's own font size, which here
    // is `5vw`. Height is constant, so only `vw` differs.
    let mk = || {
        el(
            "width: 400px; height: 400px",
            vec![el(
                "font-size: 5vw; width: 50px; height: 5px",
                vec![el("padding: 1em; width: 10px; height: 5px", vec![])],
            )],
        )
    };
    assert_eq!(lay_at(&mk(), 2, 1000, 2000), (0, 0, 110, 105));
    assert_eq!(lay_at(&mk(), 2, 2000, 2000), (0, 0, 210, 205));
}

#[test]
fn the_font_size_memo_is_keyed_on_viewport_height_alone() {
    // Width held at 1000 so only `vh` differs. Grandchild padding `1em`
    // consumes the child's `5vh` font size, so the memoised value is observable.
    let mk = || {
        el(
            "width: 400px; height: 400px",
            vec![el(
                "font-size: 5vh; width: 50px; height: 5px",
                vec![el("padding: 1em; width: 10px; height: 5px", vec![])],
            )],
        )
    };
    assert_eq!(lay_at(&mk(), 2, 1000, 2000), (0, 0, 210, 205));
    assert_eq!(lay_at(&mk(), 2, 1000, 1000), (0, 0, 110, 105));
}
