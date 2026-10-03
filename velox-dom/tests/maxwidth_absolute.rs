//! R-4: `max-width` as a real width constraint, and `position: absolute`
//! resolving against a real containing block.
//!
//! Every assertion here is pixel-exact on `LayoutNode.rect` straight out of
//! `compute_layout`, so the evidence is layout geometry with no window,
//! compositor or Skia involved.

use velox_dom::{
    Props, VNode, h,
    layout::{LayoutNode, Rect, compute_layout},
    text,
};

/// Find a laid-out child by the index it had in the VNode list.
///
/// Out-of-flow children are appended after their in-flow siblings so they paint
/// above them, so a child's position in `children` cannot be used to identify it.
fn child_at(lt: &LayoutNode, source_index: usize) -> &LayoutNode {
    lt.children
        .iter()
        .find(|c| c.source_index == Some(source_index))
        .unwrap_or_else(|| {
            panic!(
                "no child with source_index {source_index}; saw {:?}",
                lt.children
                    .iter()
                    .map(|c| (c.source_index, c.rect))
                    .collect::<Vec<_>>()
            )
        })
}

/// The out-of-flow box in the containing-block tests: 50x30 with `top`/`left`
/// set, so its placement is decided purely by which containing block it gets.
fn offset_abs_child() -> VNode {
    h(
        "div",
        Props::new().set(
            "style",
            "position: absolute; top: 10px; left: 20px; width: 50px; height: 30px;",
        ),
        vec![],
    )
}

// ===== Containing blocks (CSS 2.1 §10.1) =====

/// The containing block proof, in all three directions. One absolute child with
/// fixed offsets, three ancestor chains, three different answers:
///
/// 1. a `position: relative` ancestor — its PADDING box is the containing block;
/// 2. a static ancestor — the containing block is inherited from above, and a
///    static box's containing block is its parent's CONTENT box, so the
///    grandparent's padding shows up;
/// 3. no ancestor at all — the initial containing block, the viewport, whose
///    origin is (0, 0) regardless of any padding above.
///
/// The three are 20px apart in y, so no reading of the code can satisfy two of
/// them at once.
#[test]
fn absolute_resolves_against_positioned_ancestor_or_initial_containing_block() {
    // A style with 20px of padding on top: the padding edge and the content edge
    // are then 20px apart, and so is the content box's origin and the viewport's.
    let root = |middle: VNode| {
        h(
            "div",
            Props::new().set("style", "width: 400px; padding-top: 20px;"),
            vec![middle],
        )
    };

    // 1. Positioned ancestor: its padding box, which is 20px above its content box.
    let positioned = compute_layout(
        &root(h(
            "div",
            Props::new().set(
                "style",
                "position: relative; margin-left: 100px; margin-top: 40px; \
                 width: 200px; height: 120px; padding-top: 20px;",
            ),
            vec![offset_abs_child()],
        )),
        400,
        300,
    );
    // 2. Same box, static: no positioned ancestor, so the containing block comes
    //    from above — the root's content box, which starts 20px down.
    let unpositioned = compute_layout(
        &root(h(
            "div",
            Props::new().set(
                "style",
                "margin-left: 100px; margin-top: 40px; width: 200px; height: 120px;",
            ),
            vec![offset_abs_child()],
        )),
        400,
        300,
    );
    // 3. No ancestor: the initial containing block is the viewport.
    let no_ancestor = compute_layout(&root(offset_abs_child()), 400, 300);

    let positioned_rect = positioned.children[0].children[0].rect;
    let unpositioned_rect = unpositioned.children[0].children[0].rect;
    let no_ancestor_rect = no_ancestor.children[0].rect;

    assert_eq!(
        positioned.children[0].rect,
        Rect {
            x: 100,
            y: 60,
            w: 200,
            h: 140
        },
        "ancestor at (100, 60); 20px of padding-top adds to the 120px height"
    );
    assert_eq!(
        positioned_rect,
        Rect {
            x: 120,
            y: 70,
            w: 50,
            h: 30
        },
        "left/top resolve against the relative ancestor's PADDING box at (100, 60). \
         Its content box would have given y = 80"
    );

    assert_eq!(
        unpositioned_rect,
        Rect {
            x: 20,
            y: 30,
            w: 50,
            h: 30
        },
        "with a static parent the containing block is the one inherited from above, \
         here the root's content box at (0, 20), so y = 20 + 10 = 30"
    );

    assert_eq!(
        no_ancestor_rect,
        Rect {
            x: 20,
            y: 10,
            w: 50,
            h: 30
        },
        "with no ancestor the initial containing block is the viewport, so the \
         root's 20px of padding is irrelevant and y = 10"
    );

    assert_ne!(positioned_rect, unpositioned_rect);
    assert_ne!(unpositioned_rect, no_ancestor_rect);
    assert_ne!(positioned_rect, no_ancestor_rect);
}

/// A percentage `position: relative` offset is a percentage of the PARENT's content
/// width (CSS 2.1 §10.1: that is the containing block of an in-flow box), and the
/// box it displaces carries that same displacement into the containing block it
/// establishes for its own absolutely positioned descendants.
///
/// The child's own movement is asserted alongside the grandchild's, because the
/// child's own movement was already right before the fix: the parent's child pass
/// always had the parent's content width to hand. Only the grandchild's position
/// could disagree, which is why a test that stopped at the child passed for as long
/// as every relative offset in the suite was in `px`.
///
/// The numbers are chosen so the two bases cannot be confused. The relative box is
/// 50 wide inside a 200-wide parent, so the horizontal base is 200 (10% is 20) and
/// the wrong base is the element's own 50 (10% is 5) — a 15px difference. Vertically
/// the parent is 100 tall and the relative box 20, so the base is 100 (10% is 10) and
/// the wrong base is 20 (10% is 2).
///
/// The grandchild's expected origin is the relative box's own origin and NOT that
/// origin plus the offset again: the containing block's x is `elem_x + rel_dx`, and
/// `elem_x` is the box's position BEFORE the displacement, so the sum is exactly the
/// box's final x. The defect showed up precisely because the two halves disagreed —
/// the box landed at 20 while the containing block stayed at 5.
#[test]
fn a_percentage_relative_offset_moves_the_containing_block_it_establishes_by_the_same_amount() {
    let tree = |parent_extra: &str, rel_extra: &str| {
        h(
            "div",
            Props::new().set(
                "style",
                format!("width: 200px; height: 100px; {parent_extra}"),
            ),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    format!("position: relative; width: 50px; height: 20px; {rel_extra}"),
                ),
                vec![h(
                    "div",
                    Props::new().set(
                        "style",
                        "position: absolute; left: 0; top: 0; width: 10px; height: 10px;",
                    ),
                    vec![],
                )],
            )],
        )
    };

    // The control: the same tree with the offsets in `px`, which is the shape every
    // pre-existing relative test used and which agrees with the percentage case.
    let control = compute_layout(&tree("", "left: 20px; top: 10px;"), 400, 300);
    let pct = compute_layout(&tree("", "left: 10%; top: 10%;"), 400, 300);

    for (label, lt) in [("px", &control), ("%", &pct)] {
        let rel = child_at(lt, 0);
        assert_eq!(
            rel.rect,
            Rect {
                x: 20,
                y: 10,
                w: 50,
                h: 20
            },
            "{label}: 10% of the 200px parent and of its 100px height is 20 and 10, \
             and so is the explicit px pair"
        );
        assert_eq!(
            child_at(rel, 0).rect,
            Rect {
                x: 20,
                y: 10,
                w: 10,
                h: 10
            },
            "{label}: the absolute grandchild resolves against the DISPLACED padding \
             box, so left:0/top:0 puts it on the relative box's own origin, (20, 10). \
             If the displacement were computed against the box's own 50px width and \
             20px height instead, the box would still land at 20 while the containing \
             block stayed at (5, 2) — the grandchild 15px and 8px out"
        );
    }
}

/// The far edge of the box: `right` and `bottom` resolve percentages against the
/// same parent content box as `left` and `top`, and displace the same amount. CSS
/// 2.1 §10.3.7 for the vertical case: an over-constrained absolutely positioned
/// box ignores `bottom` in preference to `top`, but the *percentage basis* of a
/// `bottom` on an in-flow relative box is still its containing block's box.
#[test]
fn a_percentage_relative_offset_from_the_far_edge_uses_the_same_parent_base() {
    let tree = |rel_extra: &str| {
        h(
            "div",
            Props::new().set("style", "width: 200px; height: 100px;"),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    format!("position: relative; width: 50px; height: 20px; {rel_extra}"),
                ),
                vec![h(
                    "div",
                    Props::new().set(
                        "style",
                        "position: absolute; left: 0; top: 0; width: 10px; height: 10px;",
                    ),
                    vec![],
                )],
            )],
        )
    };
    let control = compute_layout(&tree("right: 20px; bottom: 10px;"), 400, 300);
    let pct = compute_layout(&tree("right: 10%; bottom: 10%;"), 400, 300);
    for (label, lt) in [("px", &control), ("%", &pct)] {
        let rel = child_at(lt, 0);
        assert_eq!(
            rel.rect.x, -20,
            "{label}: right: 10% displaces leftwards by 20 (10% of the 200px parent \
             content width), matching right: 20px. Measured against the box's own 50px \
             width it would displace by 5 and land at -5"
        );
        assert_eq!(
            rel.rect.y, -10,
            "{label}: bottom: 10% displaces upwards by 10 (10% of the parent's 100px \
             height), matching bottom: 10px. Measured against the box's own 20px \
             height it would displace by 2 and land at -2"
        );
        assert_eq!(
            child_at(rel, 0).rect.x,
            -20,
            "{label}: and the containing block it establishes moves with it, so the \
             grandchild is on the box's own origin at -20 and not at -5"
        );
    }
}

/// The flex path resolves the same percentage base, because the flex placement pass
/// hands `apply_relative_position` the container's `content_w` exactly as the block
/// child pass does. One item in a `flex-start` row sits at the container's content
/// origin, so there is no free-space redistribution to confuse the reading and the
/// assertions never touch a sibling.
///
/// ## What this test deliberately does NOT assert
///
/// It compares the percentage tree against the `px` tree rather than against an
/// absolute number for the grandchild, and the reason is a SEPARATE PRE-EXISTING
/// DIVERGENCE that this round did not introduce and is not fixing.
///
/// In the flex path the relative item is laid out at the origin (CSS Flexbox §9.2:
/// the static position of a flex item is the flex container's content-box start) and
/// the placement pass then displaces it, so an absolute grandchild is positioned
/// against a containing block at the item's UN-displaced origin and the item's
/// displacement is applied to the finished subtree on top of it. The grandchild
/// therefore ends up one offset further out than the item: at (40, 20) while the item
/// is at (20, 10), where a browser would put it at (20, 10). Verified by probe, with
/// the grandchild's own `apply_absolute_position` computing the correct 20 and
/// something later in the flex placement adding the 20 again. The same divergence is
/// present with `px` offsets, so it is not the percentage-base defect this round
/// fixed, and pinning its number here would put a wrong number into the suite behind
/// a confident assertion.
///
/// Comparing against the `px` control instead is what this round's change actually
/// governs, and it is sensitive: before the fix, `left: 10%` moved the item 5px
/// (10% of its own 50) while `left: 20px` moved it 20px, and this test failed.
#[test]
fn a_percentage_relative_offset_in_a_flex_container_uses_the_containers_content_width() {
    let tree = |rel_extra: &str| {
        h(
            "div",
            Props::new().set(
                "style",
                "display: flex; justify-content: flex-start; \
                 width: 200px; height: 100px;",
            ),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    format!("position: relative; width: 50px; height: 20px; {rel_extra}"),
                ),
                vec![h(
                    "div",
                    Props::new().set(
                        "style",
                        "position: absolute; left: 0; top: 0; width: 10px; height: 10px;",
                    ),
                    vec![],
                )],
            )],
        )
    };
    let control = compute_layout(&tree("left: 20px; top: 10px;"), 400, 300);
    let pct = compute_layout(&tree("left: 10%; top: 10%;"), 400, 300);
    let c = child_at(&control, 0);
    let p = child_at(&pct, 0);
    assert_eq!(
        p.rect.x,
        c.rect.x,
        "10% of the 200px flex container's content width and an explicit 20px must \
         move the item by the same amount. 10% of the item's own 50px would move it \
         5px instead, leaving it at {} rather than {}",
        c.rect.x,
        c.rect.x - 15
    );
    assert_eq!(
        p.rect.y, c.rect.y,
        "and the same vertically: 10% of the container's 100px height is 10, matching \
         top: 10px, not 10% of the item's own 20px"
    );
    assert_eq!(
        child_at(p, 0).rect,
        child_at(c, 0).rect,
        "the absolute grandchild moves with whichever base the item was displaced by, \
         so the percentage tree and the px tree agree on it"
    );
}

/// The containing block is the ancestor's PADDING box, not its content box and
/// not its border box. `left: 0; top: 0` therefore puts the child on the padding
/// edge — outside the padding, inside the border.
#[test]
fn absolute_containing_block_is_the_padding_box() {
    let root = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![h(
            "div",
            Props::new().set(
                "style",
                "position: relative; margin-left: 100px; margin-top: 40px; \
                 width: 200px; height: 120px; padding: 10px; border: 5px solid black;",
            ),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    "position: absolute; top: 0; left: 0; width: 20px; height: 20px;",
                ),
                vec![],
            )],
        )],
    );

    let lt = compute_layout(&root, 400, 300);
    let positioned = &lt.children[0];

    assert_eq!(
        positioned.rect,
        Rect {
            x: 100,
            y: 40,
            w: 230,
            h: 150
        },
        "content-box sizing: declared 200x120 plus 10px padding and 5px border \
         on every side"
    );

    assert_eq!(
        positioned.children[0].rect,
        Rect {
            x: 105,
            y: 45,
            w: 20,
            h: 20
        },
        "left:0/top:0 must land on the padding edge (border edge + 5px border). \
         The content origin would be (115, 55) and the border origin (100, 40)"
    );

    // `right`/`bottom` resolve against the same padding box: 105 + 220 - 20.
    let right_bottom = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![h(
            "div",
            Props::new().set(
                "style",
                "position: relative; margin-left: 100px; margin-top: 40px; \
                 width: 200px; height: 120px; padding: 10px; border: 5px solid black;",
            ),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    "position: absolute; bottom: 0; right: 0; width: 20px; height: 20px;",
                ),
                vec![],
            )],
        )],
    );
    assert_eq!(
        compute_layout(&right_bottom, 400, 300).children[0].children[0].rect,
        Rect {
            x: 305,
            y: 165,
            w: 20,
            h: 20
        },
        "right:0/bottom:0 must resolve against the 220x140 padding box"
    );

    // With NEITHER offset pair the box is not placed on the padding edge at all:
    // it goes where the flow would have put it, inside the padding. The padding
    // edge is at (105, 45) and the content origin the static position uses is
    // (115, 55) — 10px of padding on each axis, so the two cannot be confused.
    let static_positioned = compute_layout(
        &h(
            "div",
            Props::new().set("style", "width: 400px;"),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    "position: relative; margin-left: 100px; margin-top: 40px; \
                     width: 200px; height: 120px; padding: 10px; \
                     border: 5px solid black;",
                ),
                vec![h(
                    "div",
                    Props::new().set("style", "position: absolute; width: 20px; height: 20px;"),
                    vec![],
                )],
            )],
        ),
        400,
        300,
    );
    assert_eq!(
        static_positioned.children[0].children[0].rect,
        Rect {
            x: 115,
            y: 55,
            w: 20,
            h: 20
        },
        "an absolute box with no offsets takes the flow position inside the \
         ancestor's PADDING, not its padding edge"
    );
}

/// Nested positioned ancestors: the NEAREST one wins, and it is selected by
/// being an ancestor at a distinct offset, so resolving against the outer one or
/// against the viewport would both produce different numbers.
#[test]
fn nearest_positioned_ancestor_wins() {
    let root = h(
        "div",
        // The 1px top padding stops the first child's margin collapsing through
        // the parent, which would add the ancestors' own top margins a second time
        // and put every box 70px lower than the offsets in the style say.
        Props::new().set("style", "width: 400px; padding-top: 1px;"),
        vec![h(
            "div",
            Props::new().set(
                "style",
                "position: relative; margin-left: 10px; margin-top: 100px; \
                 width: 300px; height: 200px; padding-top: 1px;",
            ),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    "position: relative; margin-left: 20px; margin-top: 30px; \
                     width: 200px; height: 100px;",
                ),
                vec![h(
                    "div",
                    Props::new().set(
                        "style",
                        "position: absolute; top: 0; left: 0; width: 10px; height: 10px;",
                    ),
                    vec![],
                )],
            )],
        )],
    );

    let lt = compute_layout(&root, 400, 300);
    let outer = &lt.children[0];
    let inner = &outer.children[0];

    assert_eq!(
        outer.rect,
        Rect {
            x: 10,
            y: 101,
            w: 300,
            h: 201
        },
        "outer relative ancestor at (10, 100) inside a root whose content starts \
         at y=1, with 1px of padding-top of its own"
    );
    assert_eq!(
        inner.rect,
        Rect {
            x: 30,
            y: 132,
            w: 200,
            h: 100
        },
        "inner sits 20px right and 30px below the outer's content origin"
    );

    assert_eq!(
        inner.children[0].rect,
        Rect {
            x: 30,
            y: 132,
            w: 10,
            h: 10
        },
        "the inner relative ancestor's padding box is the containing block, not \
         the outer's (10, 101) and not the initial containing block (0, 0)"
    );
}

/// A `position: relative` box is displaced by its own offsets, and the box it
/// establishes for its descendants is the DISPLACED one. The child must therefore
/// land on the same origin as its moved ancestor, not on the ancestor's
/// un-displaced position.
#[test]
fn relative_offset_moves_the_containing_block_it_establishes() {
    let build = |position: &'static str| {
        h(
            "div",
            Props::new().set("style", "width: 400px;"),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    format!("{position}; left: 30px; top: 20px; width: 200px; height: 120px;"),
                ),
                vec![h(
                    "div",
                    Props::new().set(
                        "style",
                        "position: absolute; top: 0; left: 0; width: 10px; height: 10px;",
                    ),
                    vec![],
                )],
            )],
        )
    };

    let lt = compute_layout(&build("position: relative"), 400, 300);
    let ancestor = &lt.children[0];
    let abs_child = ancestor.children[0].rect;

    assert_eq!(
        ancestor.rect,
        Rect {
            x: 30,
            y: 20,
            w: 200,
            h: 120
        },
        "the relative ancestor itself is displaced by (30, 20)"
    );
    assert_eq!(
        abs_child,
        Rect {
            x: 30,
            y: 20,
            w: 10,
            h: 10
        },
        "the child's containing block is the ancestor's DISPLACED padding box, so \
         left:0/top:0 lands at (30, 20) and not at the un-displaced (0, 0)"
    );
}

/// A flex container establishes a containing block on exactly the same terms as a
/// block container. The offsets are checked from both ends of the box so the
/// assertion pins the padding box's SIZE and origin, not just one corner.
#[test]
fn absolute_inside_positioned_flex_container_uses_its_padding_box() {
    let build = |flex_position: &'static str| {
        h(
            "div",
            // The 20px top padding stops the flex container's margin collapsing
            // through the parent, which would otherwise apply its 70px twice, and
            // it puts the root's content box 20px below the viewport's origin so
            // "containing block inherited from above" and "the viewport" are
            // different answers.
            Props::new().set("style", "width: 400px; padding-top: 20px;"),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    format!(
                        "{flex_position}; display: flex; flex-direction: column; \
                         margin-left: 60px; margin-top: 70px; \
                         width: 300px; height: 200px;"
                    ),
                ),
                vec![h(
                    "div",
                    Props::new().set(
                        "style",
                        "position: absolute; top: 0; left: 0; width: 10px; height: 10px;",
                    ),
                    vec![],
                )],
            )],
        )
    };
    let build_right_bottom = |flex_position: &'static str| {
        h(
            "div",
            Props::new().set("style", "width: 400px; padding-top: 20px;"),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    format!(
                        "{flex_position}; display: flex; flex-direction: column; \
                         margin-left: 60px; margin-top: 70px; \
                         width: 300px; height: 200px;"
                    ),
                ),
                vec![h(
                    "div",
                    Props::new().set(
                        "style",
                        "position: absolute; bottom: 0; right: 0; width: 10px; height: 10px;",
                    ),
                    vec![],
                )],
            )],
        )
    };

    let positioned = compute_layout(&build("position: relative"), 400, 300);
    let flex_container = &positioned.children[0];
    assert_eq!(
        flex_container.rect,
        Rect {
            x: 60,
            y: 90,
            w: 300,
            h: 200
        },
        "the flex container sits at the root's content origin (0, 20) plus its own \
         60px/70px margins"
    );
    assert_eq!(
        flex_container.children[0].rect,
        Rect {
            x: 60,
            y: 90,
            w: 10,
            h: 10
        },
        "left:0/top:0 inside a positioned flex container lands on the container's \
         padding box, not on the initial containing block (0, 0)"
    );
    assert_eq!(
        compute_layout(&build_right_bottom("position: relative"), 400, 300).children[0].children[0]
            .rect,
        Rect {
            x: 350,
            y: 280,
            w: 10,
            h: 10
        },
        "right:0/bottom:0 must resolve against the container's 300x200 padding box"
    );

    // Same container, not positioned: no positioned ancestor anywhere, so the
    // offsets go to the initial containing block.
    let unpositioned = compute_layout(&build("position: static"), 400, 300);
    assert_eq!(
        unpositioned.children[0].children[0].rect,
        Rect {
            x: 0,
            y: 20,
            w: 10,
            h: 10
        },
        "with a static flex container there is no positioned ancestor, so the \
         containing block is the root's content box at (0, 20) — 20px below the \
         viewport origin the positioned case above was measured against"
    );
    assert_eq!(
        compute_layout(&build_right_bottom("position: static"), 400, 300).children[0].children[0]
            .rect,
        Rect {
            x: 390,
            y: 290,
            w: 10,
            h: 10
        },
        "right:0/bottom:0 with no positioned ancestor spans the 400x300 viewport"
    );
}

/// A positioned ancestor with an auto height only has a height once its children
/// have been laid out, so `bottom: 0` against it can only be resolved after the
/// child tree exists. Measured against the provisional height the child would
/// land 270px lower; measured against the final height it sits flush with the
/// ancestor's bottom padding edge.
#[test]
fn absolute_bottom_uses_a_positioned_ancestors_final_auto_height() {
    let root = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![h(
            "div",
            // No `height`, so the box is 30px tall because its only in-flow child
            // is 30px tall.
            Props::new().set("style", "position: relative; width: 200px;"),
            vec![
                h("div", Props::new().set("style", "height: 30px;"), vec![]),
                h(
                    "div",
                    Props::new().set(
                        "style",
                        "position: absolute; bottom: 0; left: 0; width: 10px; height: 10px;",
                    ),
                    vec![],
                ),
            ],
        )],
    );

    let lt = compute_layout(&root, 400, 300);
    let positioned = &lt.children[0];

    assert_eq!(
        positioned.rect,
        Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 30
        },
        "the ancestor's height comes from its in-flow child alone"
    );
    assert_eq!(
        positioned.children[1].rect,
        Rect {
            x: 0,
            y: 20,
            w: 10,
            h: 10
        },
        "bottom:0 must sit against the ancestor's FINAL 30px height, so y = 30 - \
         10 = 20. Resolving against the pre-child height would give y = 290"
    );
}

// ===== Out of flow and static position =====

/// An out-of-flow box occupies no space in its parent: inserting one between two
/// blocks must not move the second one, and must not change the parent's height.
/// The "without" tree is the control, so the assertion cannot pass by accident.
#[test]
fn absolute_sibling_moves_nothing_around_it() {
    let first = h("div", Props::new().set("style", "height: 20px;"), vec![]);
    let second = h("div", Props::new().set("style", "height: 20px;"), vec![]);

    // Nested inside a root because the top-level element always takes the
    // viewport height, which would hide whether the absolute box contributed
    // anything to its parent's height.
    let build = |with_abs: bool| {
        let mut kids = vec![first.clone()];
        if with_abs {
            kids.push(h(
                "div",
                Props::new().set("style", "position: absolute; width: 40px; height: 15px;"),
                vec![],
            ));
        }
        kids.push(second.clone());
        h(
            "div",
            Props::new().set("style", "width: 400px;"),
            vec![h("div", Props::new().set("style", "width: 400px;"), kids)],
        )
    };

    let without_lt = compute_layout(&build(false), 400, 300);
    let with_lt = compute_layout(&build(true), 400, 300);
    let without = &without_lt.children[0];
    let with_abs = &with_lt.children[0];

    assert_eq!(without.children.len(), 2);
    assert_eq!(child_at(without, 0).rect.y, 0);
    assert_eq!(child_at(without, 1).rect.y, 20);
    assert_eq!(
        without.rect.h, 40,
        "control height is the two blocks stacked"
    );

    assert_eq!(
        with_abs.children.len(),
        3,
        "the absolute box is still reported"
    );
    assert_eq!(
        child_at(with_abs, 0).rect.y,
        0,
        "the block before the absolute box is unmoved"
    );
    assert_eq!(
        child_at(with_abs, 2).rect.y,
        20,
        "the block AFTER the absolute box must not be pushed down by it; an \
         in-flow sibling would sit at y=35"
    );
    assert_eq!(
        with_abs.rect.h, 40,
        "the absolute box must not contribute to the parent's height"
    );
}

/// The static position (CSS 2.1 §10.3.7): an absolute box with no offsets sits
/// where it would have sat in flow, and taking it out of flow leaves a gap
/// nothing else moves into.
#[test]
fn absolute_with_no_offsets_takes_its_static_position() {
    let block = |height: &'static str| {
        h(
            "div",
            Props::new().set("style", format!("height: {height}px;")),
            vec![],
        )
    };
    let root = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![
            block("20"),
            h(
                "div",
                Props::new().set("style", "position: absolute; width: 40px; height: 15px;"),
                vec![],
            ),
            block("20"),
        ],
    );

    let lt = compute_layout(&root, 400, 300);
    assert_eq!(lt.children.len(), 3, "out-of-flow boxes are still reported");

    // Out-of-flow children are appended last so they paint above their siblings.
    let abs_box = lt.children[2].rect;
    assert_eq!(
        abs_box,
        Rect {
            x: 0,
            y: 20,
            w: 40,
            h: 15
        },
        "with neither offset pair given, the box takes its static position: the \
         flow cursor after the first 20px block. Falling back to the containing \
         block's origin would give y=0"
    );
    assert_eq!(
        lt.children[1].rect.y, 20,
        "the following block stays at the flow cursor the static position came \
         from, so nothing collapses into the out-of-flow box's place"
    );
}

/// `position: fixed` is pinned to the viewport and ignores positioned ancestors,
/// while `position: absolute` honours them. Asserted as a pair so the two
/// positions cannot be confused for one another.
#[test]
fn fixed_uses_the_viewport_while_absolute_uses_the_positioned_ancestor() {
    let build = |position: &'static str| {
        h(
            "div",
            Props::new().set("style", "width: 400px;"),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    "position: relative; margin-left: 100px; margin-top: 40px; \
                     width: 200px; height: 120px;",
                ),
                vec![h(
                    "div",
                    Props::new().set(
                        "style",
                        format!(
                            "position: {position}; top: 10px; left: 20px; \
                             width: 50px; height: 30px;"
                        ),
                    ),
                    vec![],
                )],
            )],
        )
    };

    let abs_rect = compute_layout(&build("absolute"), 400, 300).children[0].children[0].rect;
    let fixed_rect = compute_layout(&build("fixed"), 400, 300).children[0].children[0].rect;

    assert_eq!(
        abs_rect,
        Rect {
            x: 120,
            y: 50,
            w: 50,
            h: 30
        },
        "absolute resolves against the relative ancestor's padding box"
    );
    assert_eq!(
        fixed_rect,
        Rect {
            x: 20,
            y: 10,
            w: 50,
            h: 30
        },
        "fixed resolves against the viewport, ignoring the positioned ancestor"
    );
}

// ===== max-width (CSS 2.1 §10.4) =====

#[test]
fn max_width_clamps_a_block_box() {
    for (style, expected) in [
        ("width: 200px; max-width: 60px;", 60),
        ("width: 200px; max-width: 100px;", 100),
        ("width: 200px; max-width: 400px;", 200),
        ("max-width: 60px;", 60),
    ] {
        let root = h("div", Props::new().set("style", style), vec![]);
        let lt = compute_layout(&root, 400, 300);
        assert_eq!(
            lt.rect.w, expected,
            "for `{style}` the used width must be {expected}; the smaller of the \
             available width and the cap wins"
        );
    }
}

/// An absent, `auto` or percentage `max-width` that is not a cap imposes no
/// constraint at all, and neither does one that is. Each case is compared against
/// the identical tree with the declaration removed.
///
/// A NEGATIVE `max-width` is deliberately not in this list. It is not a cap that
/// fails to bind, it is an INVALID declaration: it is dropped by the filter in
/// `used_max_width` and has its own test,
/// `negative_max_width_is_dropped_as_an_invalid_declaration`.
#[test]
fn absent_or_non_binding_max_width_changes_nothing() {
    for style in [
        "width: 200px;",
        "width: 200px; max-width: auto;",
        "width: 200px; max-width: 100vw;",
    ] {
        let root = h("div", Props::new().set("style", style), vec![]);
        assert_eq!(
            compute_layout(&root, 400, 300).rect.w,
            200,
            "`{style}` must leave the width alone; the viewport is 400px so \
             max-width:100vw is not a cap here"
        );
    }
}

/// Velox drops a negative `max-width` as an invalid declaration, so it constrains
/// nothing and the used width is left alone. That is spec-conforming behaviour, not
/// a divergence, and this test pins it so that changing the filter in
/// `used_max_width` has to be a deliberate act.
///
/// The spec, in the spec's own words: CSS 2.1 §10.4 prose states that "Negative
/// values for 'min-width' and 'max-width' are illegal", and css-sizing-3 §3.2 states
/// of `<length-percentage [0,∞]>` that "Negative values are invalid". An invalid
/// declaration is dropped, so the used width is whatever it would have been without
/// it.
///
/// NO ENGINE HAS BEEN CONSULTED for that reading, and this test does not pretend
/// otherwise. An earlier version of it was named `..._is_a_known_divergence` and its
/// failure message claimed a browser "may clamp the used width to 0". That was not
/// true: no browser was checked, and the spec says the opposite of what the label
/// implied. A label asserting something unverified is worse than no label, because
/// it converts an unverified reading into an invariant, so the claim is gone.
///
/// If this assertion is failing, the filter in `used_max_width` changed. Update this
/// label, and add the new behaviour to the deviation list in the same change if it is
/// a divergence.
#[test]
fn negative_max_width_is_dropped_as_an_invalid_declaration() {
    for style in [
        "width: 200px; max-width: -50px;",
        "width: 200px; max-width: -1%;",
        "width: 200px; max-width: -0.5px;",
    ] {
        let root = h("div", Props::new().set("style", style), vec![]);
        assert_eq!(
            compute_layout(&root, 400, 300).rect.w,
            200,
            "`{style}` is an invalid declaration — CSS 2.1 §10.4 says of \
             'min-width' and 'max-width' that \"Negative values ... are illegal\" — so \
             it is dropped and constrains nothing, leaving 200. That is the spec's \
             reading, not a measurement: no engine has been consulted for it. If this \
             assertion is failing, the filter in `used_max_width` changed: update \
             this label, and add the behaviour to the deviation list in the same \
             change if it is a divergence."
        );
    }
}

/// A `min-width` floor is applied on the block flow, and it WINS over
/// `max-width` when the two conflict.
///
/// This is a precedence test, not a "a clamp exists" test. It is checking that
/// `width: 500px; min-width: 200px; max-width: 100px` resolves to 200, which is
/// only true if the floor is applied AFTER the cap. Splitting this into a
/// separate min-case and a separate max-case would throw away the only thing it
/// uniquely checks — that the two constraints are ordered — so it stays as one
/// assertion on one box. The negative direction (a max alone still capping, and
/// a min alone still flooring) is covered by the neighbouring max-width tests and
/// by `block_min_width_*` in this file.
///
/// The specs: CSS 2.1 §10.4 resolves the used width with a three-step algorithm
/// whose final step re-runs the width rules with `width` set to `min-width`, and
/// css-sizing-3 §3.1 states flatly that "the minimum size constraint is always
/// the strongest constraint". Both give 200px here.
///
/// This is not a browser measurement and is not claimed to be one; it is what the
/// cited specifications require, which is the same standard
/// `negative_max_width_is_dropped_as_an_invalid_declaration` is held to. That
/// test was previously named `min_width_is_a_known_divergence_on_a_block_box` and
/// asserted 100 — a test named for a divergence that no longer exists is a trap
/// for the next reader, so it was renamed to say what it now proves.
#[test]
fn block_min_width_overrides_max_width() {
    let root = h(
        "div",
        Props::new().set("style", "width: 500px; min-width: 200px; max-width: 100px;"),
        vec![],
    );
    assert_eq!(
        compute_layout(&root, 400, 300).rect.w,
        200,
        "block-flow min-width must be applied AFTER max-width, so the 200px \
         min-width overrides the 100px max-width (CSS 2.1 s10.4 and \
         css-sizing-3 s3.1, 'the minimum size constraint is always the strongest \
         constraint'). A value of 100 here means the clamp order was inverted: \
         the cap is being applied last and winning."
    );
}

/// An absent or `auto` `min-width` imposes NO constraint. Both of these must
/// leave the box at its uncapped width.
///
/// `auto` parses to `None` in the length table, and `None` means "unconstrained",
/// never `0`.
///
/// HONEST SCOPE — this test does NOT catch a `None`→`0` clamp, and it is not
/// written as if it did. Falsified: adding `.or(Some(0))` to `used_min_width`'s
/// filter leaves all 26 tests in this file green, because `floor(300, 0) == 300`
/// — a floor of 0 cannot raise a non-negative width, so no ordinary box
/// distinguishes "unconstrained" from "floored at 0". The two are distinguishable
/// in exactly one narrow window, where the used width is smaller than the box's
/// own padding+border (probe: `padding: 20px` in a 30px viewport, `rect.w` 30
/// correct vs 40 under the falsification). No test here pins that, because the
/// value velox returns there is itself wrong — a browser floors the content width
/// at 0 and yields a 40px border box — so asserting 30 would enshrine a
/// `content_size_for` bug as expected behaviour. That is a separate defect,
/// recorded as DEFERRED in the 4.8a report, not this task's to fix.
/// The invariant is carried by the types instead: `used_min_width` returns
/// `Option<i32>` and `floor_to_min_width` returns early on `None`, so there is no
/// number for `0` to be confused with.
#[test]
fn block_min_width_auto_and_absent_leave_the_box_unclamped() {
    let absent = h("div", Props::new().set("style", "width: 300px;"), vec![]);
    assert_eq!(
        compute_layout(&absent, 400, 300).rect.w,
        300,
        "an absent min-width must impose no constraint, so the declared 300px \
         width stands"
    );

    let auto = h(
        "div",
        Props::new().set("style", "width: 300px; min-width: auto;"),
        vec![],
    );
    assert_eq!(
        compute_layout(&auto, 400, 300).rect.w,
        300,
        "`min-width: auto` must impose no constraint, so the declared 300px \
         width stands. If this is 0, the clamp is reading a `None` from the \
         length table as zero — `auto` parses to None, and None means \
         'unconstrained', not '0'."
    );
}

/// A plain `min-width` floor with no conflicting `max-width` raises the used
/// width. Together with the `auto` case above this brackets the clamp: it fires
/// on a real length, and it stays out of the way otherwise.
#[test]
fn block_min_width_raises_the_used_width() {
    let root = h(
        "div",
        Props::new().set("style", "width: 50px; min-width: 200px;"),
        vec![],
    );
    assert_eq!(
        compute_layout(&root, 400, 300).rect.w,
        200,
        "a block box narrower than its min-width is floored up to the 200px \
         min-width"
    );
}

/// A `min-width` below the used width changes nothing — the floor is a floor,
/// not a preferred size. This is the case a clamp written with the wrong
/// comparison (or one that always assigned rather than took a max) would break.
#[test]
fn block_min_width_below_the_used_width_is_inert() {
    let root = h(
        "div",
        Props::new().set("style", "width: 300px; min-width: 50px;"),
        vec![],
    );
    assert_eq!(
        compute_layout(&root, 400, 300).rect.w,
        300,
        "a min-width below the used width must not shrink the box; the declared \
         300px width stands"
    );
}

/// A percentage `max-width` resolves against the containing block, so it tracks
/// the parent. The three-level case distinguishes the containing block (150) from
/// the grandparent (100) and from the viewport (200) at the same time.
#[test]
fn percentage_max_width_resolves_against_the_containing_block() {
    let flat = h(
        "div",
        Props::new().set("style", "width: 200px;"),
        vec![h(
            "div",
            Props::new().set("style", "width: 400px; max-width: 50%;"),
            vec![],
        )],
    );
    assert_eq!(
        compute_layout(&flat, 400, 300).children[0].rect.w,
        100,
        "50% of the 200px containing block"
    );

    let nested = h(
        "div",
        Props::new().set("style", "width: 200px;"),
        vec![h(
            "div",
            Props::new().set("style", "width: 300px;"),
            vec![h(
                "div",
                Props::new().set("style", "max-width: 50%;"),
                vec![],
            )],
        )],
    );
    let lt = compute_layout(&nested, 400, 300);
    assert_eq!(lt.children[0].rect.w, 300, "the middle box is 300px wide");
    assert_eq!(
        lt.children[0].children[0].rect.w, 150,
        "50% of the 300px containing block. 100 would mean it resolved against the \
         grandparent and 200 would mean the viewport"
    );
}

/// The cap reaches the whole subtree, not just the box's own rect: children are
/// measured against the clamped content width. An unbreakable child that cannot
/// compress below the cap overflows rather than widening its parent.
#[test]
fn max_width_narrows_children_and_lets_them_overflow() {
    let build = |parent: &'static str| {
        h(
            "div",
            Props::new().set("style", parent),
            vec![
                h(
                    "div",
                    Props::new().set("style", "width: 300px; height: 10px;"),
                    vec![],
                ),
                h("div", Props::new().set("style", "height: 10px;"), vec![]),
            ],
        )
    };

    let clamped = compute_layout(&build("width: 400px; max-width: 100px;"), 400, 300);
    assert_eq!(clamped.rect.w, 100, "the parent is clamped to its cap");
    assert_eq!(
        clamped.children[0].rect,
        Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 10
        },
        "a fixed 300px child cannot compress below the cap, so it overflows to the \
         right instead of widening the parent"
    );
    assert!(
        clamped.children[0].rect.x + clamped.children[0].rect.w > clamped.rect.w,
        "the child must genuinely stick out past the clamped parent"
    );
    assert_eq!(
        child_at(&clamped, 1).rect,
        Rect {
            x: 0,
            y: 10,
            w: 100,
            h: 10
        },
        "the second child has no width of its own, so it takes the clamped content \
         width: the cap has to land before children are measured"
    );

    let unclamped = compute_layout(&build("width: 400px;"), 400, 300);
    assert_eq!(unclamped.rect.w, 400, "without the cap the parent is 400px");
    assert_eq!(unclamped.children[0].rect.w, 300);
    assert_eq!(
        child_at(&unclamped, 1).rect.w,
        400,
        "without the cap the auto-width child fills all 400px"
    );
}

/// Text wraps to the clamped content width, so the cap changes the number of line
/// boxes. This is the behaviour the clamp's position in the function is chosen
/// for: a cap applied after the child pass would move the box and leave the text
/// wrapped to the old width.
#[test]
fn max_width_narrows_the_text_wrap_width() {
    // 39 characters at the default 9.6px advance is ~374px: one line in 400px,
    // several in the 80px the clamp leaves behind.
    let long = "the quick brown fox jumps over the lazy";
    let build =
        |parent: &'static str| h("div", Props::new().set("style", parent), vec![text(long)]);

    let unclamped = compute_layout(&build("width: 400px;"), 400, 300);
    let clamped = compute_layout(&build("width: 400px; max-width: 80px;"), 400, 300);

    assert_eq!(
        unclamped.children.len(),
        1,
        "one 400px line fits the whole string at the default font size"
    );
    assert!(
        clamped.children.len() >= 3,
        "the same string must wrap to several lines once the content width is \
         clamped to 80px; got {} line box(es)",
        clamped.children.len()
    );
    assert!(
        clamped.children.iter().all(|c| c.rect.w <= 80),
        "every line box must fit the clamped 80px content width, got {:?}",
        clamped
            .children
            .iter()
            .map(|c| c.rect.w)
            .collect::<Vec<_>>()
    );
}

/// `max-width` also caps the cross axis of a column flex container, where the
/// flex algorithm's own main-axis clamp cannot see it. Stretching sets the item's
/// width, so the cap has to be re-applied after stretch and the item has to be
/// centred against its capped width.
#[test]
fn max_width_clamps_a_column_flex_item_cross_size() {
    let build = |item: &'static str| {
        h(
            "div",
            Props::new().set("style", "width: 200px;"),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    "display: flex; flex-direction: column; width: 200px; height: 200px;",
                ),
                vec![h("div", Props::new().set("style", item), vec![])],
            )],
        )
    };

    let capped = compute_layout(&build("max-width: 60px; height: 20px;"), 400, 300);
    let uncapped = compute_layout(&build("height: 20px;"), 400, 300);

    assert_eq!(
        uncapped.children[0].children[0].rect.w, 200,
        "control: with no cap the item stretches to the container's 200px width"
    );
    assert_eq!(
        capped.children[0].children[0].rect.w, 60,
        "max-width caps the column flex item's width, which the stretch pass \
         would otherwise set to the full 200px cross size"
    );
    assert_eq!(
        capped.children[0].children[0].rect.x, 0,
        "the default cross alignment is flex-start, so the cap moves only the width"
    );

    // A centred item must be centred against the capped width, not the stretched one.
    let centred = compute_layout(
        &build("max-width: 60px; height: 20px; align-self: center;"),
        400,
        300,
    );
    let item = centred.children[0].children[0].rect;
    assert_eq!(item.w, 60);
    assert_eq!(
        item.x, 70,
        "centred inside a 200px container a 60px item starts at (200 - 60) / 2 = 70"
    );
}

/// The cap is expressed in the same box the element's `width` is, so under the
/// default `content-box` sizing the padding and border sit outside it and the
/// border box is `max-width + padding` wide.
///
/// # The rule, with its citation
///
/// A percentage length resolves against the containing block, and the size of the
/// containing block depends on whether the box is in flow:
///
/// * **Out of flow** (CSS 2.1 §10.3.7): *"For absolutely positioned elements whose
///   containing block is based on a block container element, the percentage is
///   calculated with respect to the width of the **padding box** of that element.
///   This is a change from CSS1, where the percentage width was always calculated
///   with respect to the content box of the parent element."* §10.1 is what makes
///   the padding box the containing block in the first place, so the offsets and the
///   percentages come out of the *same* box. A browser confirms it: `position:
///   absolute; width: 100%` inside `position: relative; padding: 50px; width: 250px`
///   is 350px wide.
/// * **In flow** (§10.1): the containing block of a static block container's child is
///   that container's **content** box, so a percentage is a percentage of the
///   content width.
///
/// So the basis differs between the two paths, and the ancestor's padding box
/// (240) is a different number from its content box (200) precisely when it has
/// padding. The first half of this test is the in-flow rule and the second half is
/// the out-of-flow rule, asserted in the same tree so the two cannot be satisfied
/// by one global choice of basis.
#[test]
fn percentages_resolve_against_the_padding_box_when_out_of_flow_and_the_content_box_when_in_flow() {
    let build = |ancestor_position: &'static str| {
        h(
            "div",
            Props::new().set("style", "width: 400px;"),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    format!(
                        "{ancestor_position}; width: 200px; height: 60px; \
                         padding-left: 20px; padding-right: 20px;"
                    ),
                ),
                vec![h(
                    "div",
                    Props::new().set("style", "width: 200px; max-width: 50%;"),
                    vec![],
                )],
            )],
        )
    };

    // The ancestor's border box is 240 wide, its content box 200.
    for position in ["position: relative", "position: static"] {
        let lt = compute_layout(&build(position), 400, 300);
        let ancestor = &lt.children[0];
        assert_eq!(
            ancestor.rect,
            Rect {
                x: 0,
                y: 0,
                w: 240,
                h: 60
            },
            "ancestor with {position}: 200px of content plus 20px of padding a side"
        );
        assert_eq!(
            child_at(ancestor, 0).rect.w,
            100,
            "with {position} this child is IN FLOW, so the basis is the parent's \
             CONTENT width (CSS 2.1 §10.1): 50% of 200 is 100. The 240px padding \
             box would give 120, and that is the out-of-flow rule asserted below"
        );
    }

    // Percentage padding on an OUT-OF-FLOW child. Its containing block is the
    // ancestor's PADDING box (§10.1), so the basis is that same padding box
    // (§10.3.7).
    let padded_ancestor = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![h(
            "div",
            Props::new().set(
                "style",
                "position: relative; width: 200px; height: 60px; \
                 padding-left: 20px; padding-right: 20px;",
            ),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    "position: absolute; left: 0; top: 0; width: 100px; \
                     height: 10px; padding-left: 10%;",
                ),
                vec![],
            )],
        )],
    );
    let lt = compute_layout(&padded_ancestor, 400, 300);
    assert_eq!(
        lt.children[0].children[0].rect,
        Rect {
            x: 0,
            y: 0,
            w: 124,
            h: 10
        },
        "OUT OF FLOW, so the basis is the ancestor's PADDING box: 10% of 240 is \
         24px of padding, and 100px of declared width makes the box 124 wide \
         (CSS 2.1 §10.3.7). 10% of the 200px content width would be 20px, making \
         it 120 wide"
    );

    // The same basis for `max-width`: an out-of-flow child's percentages are
    // percentages of the padding box, the same box its offsets resolve against.
    let capped_abs = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![h(
            "div",
            Props::new().set(
                "style",
                "position: relative; width: 200px; height: 60px; \
                 padding-left: 20px; padding-right: 20px;",
            ),
            vec![h(
                "div",
                Props::new().set(
                    "style",
                    "position: absolute; left: 0; top: 0; width: 200px; \
                     height: 10px; max-width: 50%;",
                ),
                vec![],
            )],
        )],
    );
    let lt = compute_layout(&capped_abs, 400, 300);
    assert_eq!(
        lt.children[0].children[0].rect,
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 10
        },
        "OUT OF FLOW: max-width: 50% is half the 240px PADDING box (CSS 2.1 \
         §10.3.7), so the 200px declared width is capped to 120. Half the 200px \
         content width would be 100"
    );
}

#[test]
fn max_width_respects_the_box_sizing_model() {
    let content_box = h(
        "div",
        Props::new().set(
            "style",
            "box-sizing: content-box; width: 200px; max-width: 60px; padding: 10px;",
        ),
        vec![],
    );
    assert_eq!(
        compute_layout(&content_box, 400, 300).rect.w,
        80,
        "content-box: the cap applies to the 60px content box, and the 20px of \
         padding is added on top of it"
    );

    let border_box = h(
        "div",
        Props::new().set(
            "style",
            "box-sizing: border-box; width: 200px; max-width: 60px; padding: 10px;",
        ),
        vec![],
    );
    assert_eq!(
        compute_layout(&border_box, 400, 300).rect.w,
        60,
        "border-box: the cap applies to the border box, padding included"
    );
}

/// `max-width` on a flex container caps the container itself, and the container's
/// narrower content box is what its items are then measured against.
#[test]
fn max_width_caps_a_flex_container() {
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; width: 300px; max-width: 120px; height: 100px;",
        ),
        vec![h(
            "div",
            Props::new().set("style", "width: 40px; height: 20px;"),
            vec![],
        )],
    );

    let lt = compute_layout(&root, 400, 300);
    assert_eq!(
        lt.rect.w, 120,
        "the flex container's own width is capped, not just its items'"
    );
    assert_eq!(
        lt.children[0].rect.w, 40,
        "a fixed-width item keeps its width inside the capped container"
    );
}
