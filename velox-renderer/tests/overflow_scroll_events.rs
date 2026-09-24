//! Scrollable overflow pipeline tests (CX-07, F-08/F-10/F-12/F-22):
//! deepest scrollable hit-testing, wheel clamping via ScrollState, and
//! offset application that shifts layout rects (so render + hit-testing
//! agree). Pure layout manipulation — no Skia required.

use std::collections::HashMap;

use velox_dom::layout::compute_layout;
use velox_dom::{Props, h};
use velox_renderer::events::{
    apply_scroll_offsets, apply_wheel_scroll, hit_test_scrollable, node_at_path,
};

fn props(s: &str) -> Props {
    Props::from_inline(s)
}

/// Nested scroll containers: root (200x600, overflow:auto) containing an
/// inner scrollable (100x300, overflow:auto, 600px child) and a tall sibling.
fn nested_tree() -> velox_dom::VNode {
    h(
        "div",
        props("width:200px; height:600px; overflow:auto;"),
        vec![
            h(
                "div",
                props("width:100px; height:300px; overflow:auto;"),
                vec![h("div", props("width:100px; height:600px;"), vec![])],
            ),
            h("div", props("width:200px; height:800px;"), vec![]),
        ],
    )
}

#[test]
fn hit_test_finds_deepest_scrollable() {
    let lo = compute_layout(&nested_tree(), 800, 600);
    assert!(lo.scrollable, "root scrollable (content exceeds 600)");
    // Inside the inner container: deepest wins.
    let inner_path = hit_test_scrollable(&lo, 50.0, 50.0).expect("scrollable under cursor");
    assert_eq!(inner_path, vec![0]);
    let inner = node_at_path(&lo, &inner_path).expect("path resolves");
    assert!(inner.scrollable);
    assert_eq!(inner.max_scroll_y, 300);
    // Beside the inner container but inside the root: only the root matches.
    assert_eq!(
        hit_test_scrollable(&lo, 150.0, 50.0),
        Some(vec![]),
        "root (empty path) is scrollable at x=150"
    );
    // Outside the root entirely: nothing.
    assert_eq!(hit_test_scrollable(&lo, 250.0, 50.0), None);
}

#[test]
fn wheel_scroll_clamps_and_shifts_children() {
    let root = h(
        "div",
        props("width:200px; height:100px; overflow:auto;"),
        vec![h("div", props("width:200px; height:300px;"), vec![])],
    );
    let lo = compute_layout(&root, 800, 600);
    assert!(lo.scrollable);
    assert_eq!(lo.max_scroll_y, 200);

    let mut offsets = HashMap::new();
    // Overscroll down: clamps to max.
    assert!(apply_wheel_scroll(&lo, 50.0, 50.0, &mut offsets, 1000.0));
    assert_eq!(offsets.get(&vec![]).copied(), Some(200.0));
    // Overscroll up: clamps to 0.
    assert!(apply_wheel_scroll(&lo, 50.0, 50.0, &mut offsets, -5000.0));
    assert_eq!(offsets.get(&vec![]).copied(), Some(0.0));
    // Already at 0 and wheeling up: no change -> no redraw needed.
    assert!(!apply_wheel_scroll(&lo, 50.0, 50.0, &mut offsets, -10.0));
    assert_eq!(offsets.get(&vec![]).copied(), Some(0.0));
    // In-range wheel accumulates.
    assert!(apply_wheel_scroll(&lo, 50.0, 50.0, &mut offsets, 60.0));
    assert_eq!(offsets.get(&vec![]).copied(), Some(60.0));

    // Apply the stored offset like the render loop does: the child subtree
    // shifts by -scroll_y while the container rect and its clip stay fixed.
    let mut lo2 = compute_layout(&root, 800, 600);
    let clip = lo2.clip.expect("scrollable has clip");
    let child_y_before = lo2.children[0].rect.y;
    let mut path = Vec::new();
    apply_scroll_offsets(&mut lo2, &offsets, &mut path);
    assert_eq!(lo2.scroll_y, 60);
    assert_eq!(lo2.children[0].rect.y, child_y_before - 60);
    assert_eq!((lo2.clip.unwrap().x, lo2.clip.unwrap().y), (clip.x, clip.y));
    assert_eq!(lo2.rect.y, 0, "container itself does not move");
}

#[test]
fn wheel_outside_scrollable_is_noop() {
    let root = h(
        "div",
        props("width:200px; height:100px; overflow:auto;"),
        vec![h("div", props("width:200px; height:300px;"), vec![])],
    );
    let lo = compute_layout(&root, 800, 600);
    let mut offsets = HashMap::new();
    assert!(!apply_wheel_scroll(&lo, 400.0, 50.0, &mut offsets, 100.0));
    assert!(
        offsets.is_empty(),
        "no offset recorded outside the container"
    );
}

#[test]
fn wheel_ignores_overflow_hidden() {
    let root = h(
        "div",
        props("width:200px; height:100px; overflow:hidden;"),
        vec![h("div", props("width:200px; height:300px;"), vec![])],
    );
    let lo = compute_layout(&root, 800, 600);
    assert!(!lo.scrollable, "hidden is not wheel-scrollable");
    assert_eq!(hit_test_scrollable(&lo, 50.0, 50.0), None);
}

#[test]
fn wheel_on_root_uses_empty_path_key() {
    let root = h(
        "div",
        props("width:200px; height:100px; overflow:auto;"),
        vec![h("div", props("width:200px; height:300px;"), vec![])],
    );
    // Point over the root where no deeper *scrollable* exists: the root itself
    // (empty path) is the wheel target.
    let lo = compute_layout(&root, 800, 600);
    let mut offsets = HashMap::new();
    assert!(apply_wheel_scroll(&lo, 150.0, 50.0, &mut offsets, 50.0));
    assert_eq!(
        offsets.get(&vec![]).copied(),
        Some(50.0),
        "root keyed by empty path"
    );
}

#[test]
fn wheel_seeds_from_deprecated_scroll_top() {
    // overflow:auto container with synthetic scroll-top: the first wheel
    // continues from the synthetic offset instead of jumping to 0.
    let root = h(
        "div",
        props("width:200px; height:100px; overflow:auto; scroll-top:50px;"),
        vec![h("div", props("width:200px; height:300px;"), vec![])],
    );
    let lo = compute_layout(&root, 800, 600);
    assert!(lo.scrollable);
    assert_eq!(lo.scroll_y, 50);

    let mut offsets = HashMap::new();
    assert!(apply_wheel_scroll(&lo, 50.0, 50.0, &mut offsets, 30.0));
    assert_eq!(
        offsets.get(&vec![]).copied(),
        Some(80.0),
        "wheel continues from synthetic scroll-top"
    );

    // Applying shifts children from the synthetic -50 position to -80.
    let mut lo2 = compute_layout(&root, 800, 600);
    let mut path = Vec::new();
    apply_scroll_offsets(&mut lo2, &offsets, &mut path);
    assert_eq!(lo2.scroll_y, 80);
    assert_eq!(lo2.children[0].rect.y, -80);
}

#[test]
fn apply_scroll_offsets_clamps_stored_offset_to_max() {
    let root = h(
        "div",
        props("width:200px; height:100px; overflow:auto;"),
        vec![h("div", props("width:200px; height:300px;"), vec![])],
    );
    let mut offsets = HashMap::new();
    offsets.insert(vec![], 1000.0);
    let mut lo = compute_layout(&root, 800, 600);
    let child_y_before = lo.children[0].rect.y;
    let mut path = Vec::new();
    apply_scroll_offsets(&mut lo, &offsets, &mut path);
    assert_eq!(lo.scroll_y, 200, "stored offset clamped to max_scroll_y");
    assert_eq!(lo.children[0].rect.y, child_y_before - 200);
}

#[test]
fn wheel_targets_deepest_and_shifts_only_that_subtree() {
    let mut offsets = HashMap::new();
    let lo = compute_layout(&nested_tree(), 800, 600);
    // Wheel inside the inner container: only the inner offset advances.
    assert!(apply_wheel_scroll(&lo, 50.0, 50.0, &mut offsets, 1000.0));
    assert_eq!(offsets.get(&vec![0usize]).copied(), Some(300.0));
    assert_eq!(offsets.len(), 1);

    let mut lo2 = compute_layout(&nested_tree(), 800, 600);
    let sibling_y_before = lo2.children[1].rect.y;
    let mut path = Vec::new();
    apply_scroll_offsets(&mut lo2, &offsets, &mut path);
    let inner = &lo2.children[0];
    assert_eq!(inner.rect.y, 0, "inner container itself stays in place");
    assert_eq!(
        inner.children[0].rect.y, -300,
        "inner content shifted to the clamped offset"
    );
    assert_eq!(
        lo2.children[1].rect.y, sibling_y_before,
        "sibling untouched"
    );
    assert_eq!(lo2.scroll_y, 0, "root offset untouched");
}
