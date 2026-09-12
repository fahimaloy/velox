use velox_dom::{
    Props, h,
    layout::{Rect, compute_layout},
};

#[test]
fn block_stacks_children_and_uses_style_size() {
    // Use block-level divs instead of text nodes to test vertical stacking
    let root = h(
        "div",
        Props::new().set("style", "width: 300px; height: 100px;"),
        vec![
            h("div", Props::new().set("style", "height: 20px;"), vec![]),
            h("div", Props::new().set("style", "height: 30px;"), vec![]),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(
        lt.rect,
        Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 100
        }
    );
    assert_eq!(lt.children.len(), 2);
    // First child at y=0 (with padding)
    assert_eq!(lt.children[0].rect.y, 0);
    // Second child should be below first (y >= first child's height)
    assert!(
        lt.children[1].rect.y >= lt.children[0].rect.h,
        "Expected children[1].rect.y ({}) >= children[0].rect.h ({})",
        lt.children[1].rect.y,
        lt.children[0].rect.h
    );
}

// ===== Flexbox Tests =====

#[test]
fn flex_row_basic() {
    // Flex row should place children horizontally
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 50px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.rect.w, 300);
    assert_eq!(lt.rect.h, 100);
    assert_eq!(lt.children.len(), 2);
    // Children should be placed horizontally
    assert_eq!(lt.children[0].rect.x, 0);
    assert!(
        lt.children[1].rect.x > lt.children[0].rect.x,
        "Second child should be to the right of first child"
    );
}

#[test]
fn flex_column_basic() {
    // Flex column should stack children vertically
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-direction: column; width: 200px; height: 300px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 80px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 120px; height: 60px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.rect.w, 200);
    assert_eq!(lt.rect.h, 300);
    assert_eq!(lt.children.len(), 2);
    // Children should be stacked vertically
    assert_eq!(lt.children[0].rect.y, 0);
    assert!(
        lt.children[1].rect.y > lt.children[0].rect.y,
        "Second child should be below first child"
    );
}

#[test]
fn flex_grow_distributes_space() {
    // Two items with flex-grow: 1 should split space equally
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 400px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "flex-grow: 1; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "flex-grow: 1; height: 40px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // Both items should have roughly equal width (200px each)
    let w0 = lt.children[0].rect.w;
    let w1 = lt.children[1].rect.w;
    assert!(
        (w0 - w1).abs() <= 2,
        "Items with equal flex-grow should have similar widths: {} vs {}",
        w0,
        w1
    );
    assert!(
        (190..=210).contains(&w0),
        "Item width should be ~200px, got {}",
        w0
    );
}

#[test]
fn flex_grow_proportional() {
    // flex-grow: 1 and flex-grow: 2 should give 1/3 and 2/3 of space
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "flex-grow: 1; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "flex-grow: 2; height: 40px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    let w0 = lt.children[0].rect.w;
    let w1 = lt.children[1].rect.w;
    // Second item should be roughly twice the width of first
    let ratio = w1 as f32 / w0 as f32;
    assert!(
        ratio > 1.5 && ratio < 2.5,
        "Second item should be ~2x wider: w0={}, w1={}, ratio={}",
        w0,
        w1,
        ratio
    );
}

#[test]
fn flex_gap_spacing() {
    // Flex with gap should space items properly
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; width: 400px; height: 100px; gap: 20px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 40px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 3);
    // Items should have gaps between them
    let gap1 = lt.children[1].rect.x - (lt.children[0].rect.x + lt.children[0].rect.w);
    let gap2 = lt.children[2].rect.x - (lt.children[1].rect.x + lt.children[1].rect.w);
    assert!(
        gap1 >= 15,
        "Gap between items should be ~20px, got {}",
        gap1
    );
    assert!(
        gap2 >= 15,
        "Gap between items should be ~20px, got {}",
        gap2
    );
}

#[test]
fn flex_justify_content_center() {
    // justify-content: center should center items
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; justify-content: center; width: 400px; height: 100px;",
        ),
        vec![h(
            "div",
            Props::new().set("style", "width: 100px; height: 40px;"),
            vec![],
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    // Item should be centered horizontally
    let item = &lt.children[0];
    let offset = item.rect.x;
    assert!(
        offset > 100 && offset < 350,
        "Item should be centered, x={}, expected ~150",
        offset
    );
}

#[test]
fn flex_align_items_center() {
    // align-items: center should center items on cross axis
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; align-items: center; width: 300px; height: 200px;",
        ),
        vec![h(
            "div",
            Props::new().set("style", "width: 80px; height: 60px;"),
            vec![],
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let item = &lt.children[0];
    // Item should be vertically centered
    let offset = item.rect.y;
    assert!(
        offset > 50 && offset < 150,
        "Item should be vertically centered, y={}, expected ~70",
        offset
    );
}

#[test]
fn flex_align_self_override() {
    // align-self should override align-items for individual item
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; align-items: flex-start; width: 300px; height: 200px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 60px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 60px; align-self: flex-end;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // Second item should be lower than first due to align-self: flex-end
    assert!(
        lt.children[1].rect.y > lt.children[0].rect.y,
        "Item with align-self: flex-end should be lower: y1={}, y2={}",
        lt.children[0].rect.y,
        lt.children[1].rect.y
    );
}

#[test]
fn flex_shrink_on_overflow() {
    // Items should shrink when they exceed container width
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 200px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 150px; height: 40px; flex-shrink: 1;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 150px; height: 40px; flex-shrink: 1;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // Total width of children should be <= container width
    let total_w = lt.children[0].rect.w + lt.children[1].rect.w;
    assert!(
        total_w <= 200,
        "Total width of shrunk items should fit container: {} > 200",
        total_w
    );
}

// ===== Text Layout Tests =====

#[test]
fn text_uses_font_metrics() {
    // Text dimensions should scale with font-size
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 200px;"),
        vec![velox_dom::VNode::Text("Hello World".to_string())],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let text_node = &lt.children[0];
    // "Hello World" is 11 chars, at 16px font, char_width ~ 9.6
    // width should be roughly 11 * 9.6 ≈ 105
    assert!(
        text_node.rect.w > 80 && text_node.rect.w < 150,
        "Text width should be ~105px, got {}",
        text_node.rect.w
    );
    // Height should be ~1.2 * 16 = 19.2
    assert!(
        text_node.rect.h > 15 && text_node.rect.h < 25,
        "Text height should be ~19px, got {}",
        text_node.rect.h
    );
}

#[test]
fn text_larger_font() {
    // Larger font size should produce larger text dimensions
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 200px; font-size: 24px;"),
        vec![velox_dom::VNode::Text("Hello".to_string())],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let text_node = &lt.children[0];
    // 5 chars at 24px font, char_width = 14.4, width ≈ 72
    assert!(
        text_node.rect.w > 50 && text_node.rect.w < 100,
        "Text width at 24px should be ~72px, got {}",
        text_node.rect.w
    );
    // Height should be ~1.2 * 24 = 28.8
    assert!(
        text_node.rect.h > 24 && text_node.rect.h < 35,
        "Text height at 24px should be ~29px, got {}",
        text_node.rect.h
    );
}

#[test]
fn text_align_center() {
    // text-align: center should center text lines
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 200px; text-align: center;"),
        vec![velox_dom::VNode::Text("Hello".to_string())],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let text_node = &lt.children[0];
    // Text should be roughly centered
    let expected_center = (400 - text_node.rect.w) / 2;
    assert!(
        (text_node.rect.x - expected_center).abs() < 5,
        "Text should be centered, x={}, expected ~{}",
        text_node.rect.x,
        expected_center
    );
}

#[test]
fn text_align_right() {
    // text-align: right should right-align text
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 200px; text-align: right;"),
        vec![velox_dom::VNode::Text("Hello".to_string())],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let text_node = &lt.children[0];
    // Text should be at the right edge
    let expected_x = 400 - text_node.rect.w;
    assert!(
        (text_node.rect.x - expected_x).abs() < 5,
        "Text should be right-aligned, x={}, expected ~{}",
        text_node.rect.x,
        expected_x
    );
}

// ===== Text Wrapping Tests =====

#[test]
fn text_wrap_splits_lines() {
    // Long text should wrap into multiple lines
    let root = h(
        "div",
        Props::new().set("style", "width: 200px; height: 300px;"),
        vec![velox_dom::VNode::Text(
            "This is a longer piece of text that should wrap".to_string(),
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    // Text should wrap into at least 2 lines
    assert!(
        lt.children.len() >= 2,
        "Long text should wrap into multiple lines, got {} lines",
        lt.children.len()
    );
}

// ===== Layout Context Tests =====

#[test]
fn layout_uses_full_width_for_root() {
    // Root div should fill viewport width
    let root = h(
        "body",
        Props::new(),
        vec![h("div", Props::new().set("style", "height: 50px;"), vec![])],
    );
    let lt = compute_layout(&root, 1024, 768);

    // Root should fill viewport
    assert_eq!(lt.rect.w, 1024, "Root should fill viewport width");
}

#[test]
fn flex_column_renders_children() {
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-direction: column; width: 300px; height: 400px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 100px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 100px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 100px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 3);
    // Each child should be below the previous
    for i in 1..lt.children.len() {
        assert!(
            lt.children[i].rect.y > lt.children[i - 1].rect.y,
            "Child {} should be below child {}",
            i,
            i - 1
        );
    }
}
