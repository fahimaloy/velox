use velox_dom::{Props, h, layout::compute_layout, text};

fn gap_between(parent_lo: &velox_dom::layout::LayoutNode, a_idx: usize, b_idx: usize) -> i32 {
    let a = &parent_lo.children[a_idx];
    let b = &parent_lo.children[b_idx];
    b.rect.y - (a.rect.y + a.rect.h)
}

#[test]
fn sibling_positive_collapse_is_max() {
    // header mb 12 + card mt 16 => 16 gap (max)
    let parent = h(
        "div",
        Props::new(),
        vec![
            h(
                "div",
                Props::from_inline("margin-bottom:12px; height:10px"),
                vec![text("a")],
            ),
            h(
                "div",
                Props::from_inline("margin-top:16px; height:10px"),
                vec![text("b")],
            ),
        ],
    );
    let lo = compute_layout(&parent, 800, 600);
    // children are block rects; gap should be max(12,16)=16
    let gap = gap_between(&lo, 0, 1);
    assert_eq!(
        gap, 16,
        "sibling positive collapse should be max, got {}",
        gap
    );
}

#[test]
fn negative_collapse() {
    // mb -8 + mt 4 => -4 per spec (positive 4 + negative -8)
    let parent = h(
        "div",
        Props::new(),
        vec![
            h(
                "div",
                Props::from_inline("margin-bottom:-8px; height:10px"),
                vec![text("a")],
            ),
            h(
                "div",
                Props::from_inline("margin-top:4px; height:10px"),
                vec![text("b")],
            ),
        ],
    );
    let lo = compute_layout(&parent, 800, 600);
    let gap = gap_between(&lo, 0, 1);
    assert_eq!(
        gap, -4,
        "negative collapse should be sum (-8+4=-4), got {}",
        gap
    );
}

#[test]
fn parent_through_collapse() {
    // parent border/padding 0 => child's 20 collapses through, parent y stays 0
    let child = h(
        "div",
        Props::from_inline("margin-top:20px; height:10px"),
        vec![text("hi")],
    );
    let parent = h("div", Props::new(), vec![child]);
    let lo = compute_layout(&parent, 800, 600);
    assert_eq!(
        lo.rect.y, 0,
        "parent border/padding 0 => child's margin collapses through, parent y 0"
    );
    // also verify child is offset by 20 from parent content
    let child_lo = &lo.children[0];
    // child y should be 20 (collapsed through)
    assert_eq!(
        child_lo.rect.y, 20,
        "child should be at 20 due to collapsed-through margin, got {}",
        child_lo.rect.y
    );
}

#[test]
fn both_negative_collapse_is_most_negative() {
    // mb -10 + mt -6 => -10 (most negative)
    let parent = h(
        "div",
        Props::new(),
        vec![
            h(
                "div",
                Props::from_inline("margin-bottom:-10px; height:10px"),
                vec![text("a")],
            ),
            h(
                "div",
                Props::from_inline("margin-top:-6px; height:10px"),
                vec![text("b")],
            ),
        ],
    );
    let lo = compute_layout(&parent, 800, 600);
    let gap = gap_between(&lo, 0, 1);
    assert_eq!(
        gap, -10,
        "both negative should be most negative -10, got {}",
        gap
    );
}

#[test]
fn empty_block_collapse_through() {
    // empty block with mt 10 mb 15 between two siblings: empty collapses to max(10,15)=15,
    // then gap to next sibling mt 12 => max(15,12)=15
    let parent = h(
        "div",
        Props::new(),
        vec![
            h(
                "div",
                Props::from_inline("margin-bottom:5px; height:10px"),
                vec![text("a")],
            ),
            h(
                "div",
                Props::from_inline("margin-top:10px; margin-bottom:15px"),
                vec![],
            ),
            h(
                "div",
                Props::from_inline("margin-top:12px; height:10px"),
                vec![text("c")],
            ),
        ],
    );
    let lo = compute_layout(&parent, 800, 600);
    // Gap from first to third should account for collapsed empty
    // first bottom 5 vs empty collapsed 15 => 15; then 15 vs 12 => 15
    // So third's y should be first_y +10 +15
    let first = &lo.children[0];
    let third = &lo.children[2];
    let gap = third.rect.y - (first.rect.y + first.rect.h);
    assert_eq!(
        gap, 15,
        "empty block through collapse gap should be 15, got {}",
        gap
    );
}
