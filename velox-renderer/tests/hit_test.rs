//! Clip-correct hit testing (CX-08, F-11): content clipped by an ancestor's
//! `overflow:hidden`/scroll container must not be clickable, hoverable or
//! focusable outside the intersected clip, and hit-test order must be
//! stacking-context aware. Pure logic tests — no Skia required.

use std::collections::HashMap;

use velox_dom::layout::{Rect, compute_layout};
use velox_dom::{VNode, h};
use velox_renderer::events::{
    StackCtx, apply_scroll_offsets, apply_wheel_scroll, collect_click_targets,
    collect_hover_targets, collect_input_targets, hit_test_click, hit_test_hover, hit_test_input,
};

/// Parent `overflow:hidden` at (0,0) 100x100 with an absolutely positioned
/// child at (90,90) 100x100 that protrudes to (190,190). The protruding
/// part of the child is visually clipped by the parent, so it must not be
/// clickable; the visible part still is (CX-08, F-11).
#[test]
fn clip_rejects_click_outside_parent() {
    let vnode: VNode = h(
        "div",
        vec![
            ("style", "width:100px;height:100px;overflow:hidden;"),
            ("on:click", "parent"),
        ],
        vec![h(
            "button",
            vec![
                (
                    "style",
                    "position:absolute;top:90px;left:90px;width:100px;height:100px;",
                ),
                ("on:click", "child"),
            ],
            vec![],
        )],
    );

    let layout = compute_layout(&vnode, 800, 600);
    // Test setup: parent at (0,0,100,100), child protrudes to (190,190).
    assert_eq!(
        layout.rect,
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100
        }
    );
    assert_eq!(
        layout.children[0].rect,
        Rect {
            x: 90,
            y: 90,
            w: 100,
            h: 100
        }
    );

    let mut targets = Vec::new();
    let mut order = 0;
    collect_click_targets(
        &vnode,
        &layout,
        None,
        StackCtx::ROOT,
        &mut order,
        &mut targets,
    );

    // Protrusion outside the parent clip must not be hittable.
    assert!(
        hit_test_click(&targets, 150.0, 150.0).is_none(),
        "click in the clipped-away protrusion must miss"
    );
    // Inside the parent is still clickable.
    assert!(hit_test_click(&targets, 50.0, 50.0).is_some());
    // The visible part of the child (inside the clip) still hits the child.
    assert_eq!(hit_test_click(&targets, 95.0, 95.0), Some(("child", None)));
}

/// Same geometry as the click test, but for hover targets: the protrusion
/// must not be hoverable outside the parent's clip.
#[test]
fn clip_rejects_hover_outside_parent() {
    let vnode: VNode = h(
        "button",
        vec![
            ("style", "width:100px;height:100px;overflow:hidden;"),
            ("on:click", "parent"),
            ("data-hover-id", "7"),
        ],
        vec![h(
            "button",
            vec![
                (
                    "style",
                    "position:absolute;top:90px;left:90px;width:100px;height:100px;",
                ),
                ("on:click", "child"),
                ("data-hover-id", "9"),
            ],
            vec![],
        )],
    );

    let layout = compute_layout(&vnode, 800, 600);
    let mut targets = Vec::new();
    let mut order = 0;
    collect_hover_targets(
        &vnode,
        &layout,
        None,
        StackCtx::ROOT,
        &mut order,
        &mut targets,
    );

    // Protrusion: the child rect contains (150,150) but the parent clip does
    // not, so no hover target may match.
    assert_eq!(hit_test_hover(&targets, 150.0, 150.0), None);
    // Inside the parent only: the parent hovers.
    assert_eq!(hit_test_hover(&targets, 50.0, 50.0), Some(7));
    // Inside the clipped-visible overlap: the positioned child wins.
    assert_eq!(hit_test_hover(&targets, 95.0, 95.0), Some(9));
}

/// Same geometry for text-input focus targets: a protruding input must not
/// be focusable outside the parent's clip.
#[test]
fn clip_rejects_input_focus_outside_parent() {
    let vnode: VNode = h(
        "div",
        vec![("style", "width:100px;height:100px;overflow:hidden;")],
        vec![h(
            "input",
            vec![
                (
                    "style",
                    "position:absolute;top:90px;left:90px;width:100px;height:100px;",
                ),
                ("type", "text"),
            ],
            vec![],
        )],
    );

    let layout = compute_layout(&vnode, 800, 600);
    let mut targets = Vec::new();
    let mut order = 0;
    let mut path = Vec::new();
    collect_input_targets(
        &vnode,
        &layout,
        None,
        StackCtx::ROOT,
        &mut path,
        &mut order,
        &mut targets,
    );

    assert!(
        hit_test_input(&targets, 150.0, 150.0).is_none(),
        "input protruding past the clip must not be focusable"
    );
    assert!(
        hit_test_input(&targets, 95.0, 95.0).is_some(),
        "visible part of the input stays focusable"
    );
}

/// Scroll composition: after wheel scrolling, the child subtree rects are
/// shifted by `apply_scroll_offsets`, but only the region inside the
/// container's clip stays hittable — the part of the shifted child that
/// protrudes below the container must not be clickable.
#[test]
fn scrolled_protrusion_not_hittable_outside_container() {
    let vnode: VNode = h(
        "div",
        vec![("style", "width:100px;height:100px;overflow:auto;")],
        vec![h(
            "button",
            vec![
                ("style", "width:100px;height:300px;"),
                ("on:click", "tall-child"),
            ],
            vec![],
        )],
    );
    let lo = compute_layout(&vnode, 800, 600);
    assert!(lo.scrollable);

    // Scroll 60 logical px like the render loop: wheel, then apply offsets.
    let mut offsets = HashMap::new();
    assert!(apply_wheel_scroll(&lo, 50.0, 50.0, &mut offsets, 60.0));
    let mut scrolled = compute_layout(&vnode, 800, 600);
    let mut path = Vec::new();
    apply_scroll_offsets(&mut scrolled, &offsets, &mut path);
    assert_eq!(scrolled.children[0].rect.y, -60, "child shifted up");

    let mut targets = Vec::new();
    let mut order = 0;
    collect_click_targets(
        &vnode,
        &scrolled,
        None,
        StackCtx::ROOT,
        &mut order,
        &mut targets,
    );

    // Inside the container clip and inside the shifted child: hittable.
    assert_eq!(
        hit_test_click(&targets, 50.0, 40.0),
        Some(("tall-child", None))
    );
    // The child's shifted rect spans y=-60..240, but only the container's
    // clip region (y=0..100) is hittable: y=150 is clipped away.
    assert_eq!(
        hit_test_click(&targets, 50.0, 150.0),
        None,
        "scrolled-out protrusion must not be clickable"
    );
}

/// Stacking contexts form groups: a descendant's z-index is trapped inside
/// its ancestor's stacking context. `a` (z-index:1) contains `g`
/// (z-index:100); sibling `b` (z-index:50) must still win the overlap
/// because the whole `a` group paints below `b`.
#[test]
fn stacking_group_traps_descendant_z_index() {
    let vnode: VNode = h(
        "div",
        vec![("style", "width:200px;height:200px;")],
        vec![
            h(
                "div",
                vec![
                    (
                        "style",
                        "position:absolute;top:0;left:0;width:100px;height:100px;z-index:1;",
                    ),
                    ("on:click", "a"),
                ],
                vec![h(
                    "button",
                    vec![
                        (
                            "style",
                            "position:absolute;top:0;left:0;width:100px;height:100px;z-index:100;",
                        ),
                        ("on:click", "g"),
                    ],
                    vec![],
                )],
            ),
            h(
                "button",
                vec![
                    (
                        "style",
                        "position:absolute;top:50px;left:50px;width:100px;height:100px;z-index:50;",
                    ),
                    ("on:click", "b"),
                ],
                vec![],
            ),
        ],
    );
    let layout = compute_layout(&vnode, 800, 600);
    let mut targets = Vec::new();
    let mut order = 0;
    collect_click_targets(
        &vnode,
        &layout,
        None,
        StackCtx::ROOT,
        &mut order,
        &mut targets,
    );

    // Overlap of a/g/b: the sibling with z-index 50 is above the whole
    // z-index 1 group (the descendant's z-index 100 cannot escape it).
    assert_eq!(hit_test_click(&targets, 75.0, 75.0), Some(("b", None)));
    // Overlap of a and g only: the deeper child g paints above its parent.
    assert_eq!(hit_test_click(&targets, 10.0, 10.0), Some(("g", None)));
}

/// Within the same stacking depth, the higher z-index wins regardless of
/// source order (z-index:10 declared before z-index:1).
#[test]
fn same_depth_prefers_higher_z_index() {
    let vnode: VNode = h(
        "div",
        vec![("style", "width:200px;height:200px;")],
        vec![
            h(
                "button",
                vec![
                    (
                        "style",
                        "position:absolute;top:0;left:0;width:100px;height:100px;z-index:10;",
                    ),
                    ("on:click", "high"),
                ],
                vec![],
            ),
            h(
                "button",
                vec![
                    (
                        "style",
                        "position:absolute;top:0;left:0;width:100px;height:100px;z-index:1;",
                    ),
                    ("on:click", "low"),
                ],
                vec![],
            ),
        ],
    );
    let layout = compute_layout(&vnode, 800, 600);
    let mut targets = Vec::new();
    let mut order = 0;
    collect_click_targets(
        &vnode,
        &layout,
        None,
        StackCtx::ROOT,
        &mut order,
        &mut targets,
    );

    assert_eq!(hit_test_click(&targets, 50.0, 50.0), Some(("high", None)));
}

/// A positioned element creates a stacking context and paints above in-flow
/// static siblings, independent of source order.
#[test]
fn positioned_overlay_hits_above_static_sibling() {
    let vnode: VNode = h(
        "div",
        vec![("style", "width:200px;height:200px;")],
        vec![
            h(
                "button",
                vec![
                    ("style", "width:100px;height:100px;"),
                    ("on:click", "under"),
                ],
                vec![],
            ),
            h(
                "button",
                vec![
                    (
                        "style",
                        "position:absolute;top:0;left:0;width:100px;height:100px;",
                    ),
                    ("on:click", "overlay"),
                ],
                vec![],
            ),
        ],
    );
    let layout = compute_layout(&vnode, 800, 600);
    let mut targets = Vec::new();
    let mut order = 0;
    collect_click_targets(
        &vnode,
        &layout,
        None,
        StackCtx::ROOT,
        &mut order,
        &mut targets,
    );

    assert_eq!(
        hit_test_click(&targets, 50.0, 50.0),
        Some(("overlay", None)),
        "positioned overlay must hit above the static sibling"
    );
}
